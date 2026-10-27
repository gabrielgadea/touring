// touring-compact-evict: the planner. Pure functions, no I/O: register.ts owns the disk work, so this
// file is exercised without an engine. Contract: CC 2.1.311 `claude-code.d.ts`, `SessionCompactInput`
// (`session.compact`) and `SessionMessage` / `ToolUseSummary` / `ToolResultSummary`.
//
// What it decides: which tool results of the transcript being compacted were already superseded, so the
// summarizer reads a pointer instead of the old bytes. Nothing here calls a model.
import type { SessionMessage, ToolResultSummary } from 'claude-code'

export const DEFAULT_KEEP_LAST = 6
export const DEFAULT_MIN_BYTES = 2048
// Below this a stub saves nothing, so a configured minimum cannot go under it.
const MIN_BYTES_FLOOR = 512
// A call's position is msgIndex * STRIDE + its index in that message, so two parallel calls keep their order.
const STRIDE = 1_000_000

export type EvictConfig = { keepLast: number; minBytes: number }
export type Category = 'superseded-read' | 'repeated-command' | 'persisted-output'

export type Candidate = {
  /** `${messageIndex}:${resultIndex}`, the key applyEvictions looks stubs up by. */
  key: string
  messageIndex: number
  resultIndex: number
  toolUseId: string
  tool: string
  /** The file or command, clipped to one line. */
  label: string
  category: Category
  reason: string
  bytes: number
  lines: number
  /** The result as the model read it: what gets written to the raw file. */
  text: string
  /** Set when Claude Code already persisted the full output: the raw file is that one. */
  persistedPath?: string
}

type Use = { id: string; msgIndex: number; pos: number; tool: string; input: Record<string, unknown>; ok: boolean; failed: boolean }
type Later = {
  mutations: Map<string, { pos: number; tool: string }[]>
  reads: Map<string, number[]>
  commands: Map<string, number[]>
}

// Tools that change a file on disk, and how a stub names the change.
const MUTATING: Readonly<Record<string, string>> = {
  Edit: 'edited later',
  MultiEdit: 'edited later',
  NotebookEdit: 'edited later',
  Write: 'written later',
}

// `<persisted-output>\nOutput too large (48.2KB). Full output saved to: /path/file.txt\n\nPreview ...`
const PERSISTED = /<persisted-output>[\s\S]*?Full output saved to:[ \t]*([^\n]+)/
// A green exit code with red content must not be evicted. Over-matching only costs a missed eviction.
const FAILURE_TEXT =
  /(^|\n)[ \t]*(FAILED|FAIL\b|ERROR\b|errors?(\[E\d+\])?:|fatal:)|Traceback \(most recent call last\)|\bpanicked at\b|\btest result: FAILED\b|\bAssertionError\b|\bnpm ERR!|\b[1-9]\d* (failed|failures?|errors?)\b|\bexit (code|status) [1-9]|command not found|Permission denied|\bblocked\b|\bdenied\b/i
// The tail of a test run is where its verdict is, and a persisted preview is only the head.
const TEST_RUN =
  /\b(pytest|cargo (nextest|test)|npm (run )?test|yarn test|pnpm test|jest|vitest|go test|make test|tox|unittest|rspec|mvn test|gradle test)\b/
const NON_TEXT_TYPES = new Set(['image', 'document', 'pdf', 'audio', 'video'])

const encoder = new TextEncoder()
const byteLength = (s: string): number => encoder.encode(s).length
const str = (v: unknown): string | undefined => (typeof v === 'string' && v.length > 0 ? v : undefined)

function push<K, V>(map: Map<K, V[]>, key: K, value: V): void {
  const list = map.get(key)
  if (list === undefined) map.set(key, [value])
  else list.push(value)
}

function clip(s: string, max: number): string {
  const one = s.replace(/\s+/g, ' ').trim()
  return one.length > max ? `${one.slice(0, max - 3)}...` : one
}

export function fmtBytes(n: number): string {
  return n >= 1024 ? `${(n / 1024).toFixed(1)} KiB` : `${n} B`
}

/** True for `on`, `1`, `true`, `yes` (any case): the only values that arm the mod. */
export function isOn(value: string | undefined): boolean {
  return value !== undefined && ['on', '1', 'true', 'yes'].includes(value.trim().toLowerCase())
}

/** An env value as an integer >= `min`; anything else (unset, junk, too small) is the fallback. */
export function intOr(value: string | undefined, fallback: number, min = 0): number {
  if (value === undefined || !/^\s*\d+\s*$/.test(value)) return fallback
  const n = Number(value)
  return n >= min ? n : fallback
}

// A structured result that carries an image/pdf block: a message rebuilt from `text` would lose it.
function hasNonText(v: unknown, depth = 0): boolean {
  if (v === null || typeof v !== 'object' || depth > 3) return false
  if (Array.isArray(v)) return v.some((x) => hasNonText(x, depth + 1))
  const o = v as Record<string, unknown>
  if (typeof o.type === 'string' && NON_TEXT_TYPES.has(o.type)) return true
  if ('base64' in o) return true
  return Object.values(o).some((x) => hasNonText(x, depth + 1))
}

function collectUses(messages: readonly SessionMessage[]): Use[] {
  const uses: Use[] = []
  for (const [mi, m] of messages.entries()) {
    for (const [j, t] of (m.toolUses ?? []).entries()) {
      uses.push({
        id: t.tool_use_id,
        msgIndex: mi,
        pos: mi * STRIDE + j,
        tool: t.tool,
        input: t.input ?? {},
        // Answered and not an error: only such a call can supersede an earlier result.
        ok: t.isError !== true && t.text !== undefined,
        failed: t.isError === true,
      })
    }
  }
  return uses
}

const readKey = (u: Use): string | undefined => {
  const path = str(u.input.file_path)
  return path === undefined ? undefined : `${path}\u0000${String(u.input.offset ?? '')}:${String(u.input.limit ?? '')}:${String(u.input.pages ?? '')}`
}

function indexLater(uses: readonly Use[]): Later {
  const later: Later = { mutations: new Map(), reads: new Map(), commands: new Map() }
  for (const u of uses) {
    if (!u.ok) continue
    if (MUTATING[u.tool] !== undefined) {
      const path = str(u.input.file_path) ?? str(u.input.notebook_path)
      if (path !== undefined) push(later.mutations, path, { pos: u.pos, tool: u.tool })
    } else if (u.tool === 'Read') {
      const key = readKey(u)
      if (key !== undefined) push(later.reads, key, u.pos)
    } else if (u.tool === 'Bash') {
      const cmd = str(u.input.command)?.trim()
      if (cmd !== undefined && cmd.length > 0) push(later.commands, cmd, u.pos)
    }
  }
  return later
}

/** Why a Read result is stale, or undefined when it is not. A re-read counts only for the same range. */
function readSupersededBy(u: Use, later: Later): string | undefined {
  const path = str(u.input.file_path)
  if (path === undefined) return undefined
  const mutation = later.mutations.get(path)?.find((m) => m.pos > u.pos)
  if (mutation !== undefined) return MUTATING[mutation.tool]
  const key = readKey(u)
  return key !== undefined && later.reads.get(key)?.some((p) => p > u.pos) === true ? 're-read later' : undefined
}

/**
 * True when the result's text shows a failure. The pattern has no `\s*` after the line start on purpose:
 * `(^|\n)\s*` backtracks quadratically over a long run of blank lines, and a hook that overruns its 10 s
 * is skipped (three crashes switch every installed mod off).
 */
export function hasFailureText(text: string): boolean {
  return FAILURE_TEXT.test(text)
}

function isTestRun(u: Use): boolean {
  const cmd = str(u.input.command)
  return u.tool === 'Bash' && cmd !== undefined && TEST_RUN.test(cmd)
}

function labelOf(u: Use): string {
  if (u.tool === 'Read') return clip(str(u.input.file_path) ?? 'file', 160)
  if (u.tool === 'Bash') return `\`${clip(str(u.input.command) ?? 'command', 120)}\``
  return clip(JSON.stringify(u.input) ?? '', 120)
}

type Classified = { category: Category; reason: string; persistedPath?: string }

function classify(u: Use, text: string, later: Later): Classified | undefined {
  const persisted = PERSISTED.exec(text)?.[1]?.trim()
  if (persisted !== undefined && persisted.startsWith('/')) {
    // Red tests hide in the tail, which a persisted preview does not show.
    if (isTestRun(u) || hasFailureText(text)) return undefined
    return { category: 'persisted-output', reason: 'already persisted on disk', persistedPath: persisted }
  }
  if (u.tool === 'Read') {
    const reason = readSupersededBy(u, later)
    return reason === undefined ? undefined : { category: 'superseded-read', reason }
  }
  if (u.tool === 'Bash') {
    const cmd = str(u.input.command)?.trim()
    if (cmd === undefined || hasFailureText(text)) return undefined
    return later.commands.get(cmd)?.some((p) => p > u.pos) === true ? { category: 'repeated-command', reason: 'repeated later' } : undefined
  }
  return undefined
}

/**
 * The results of message `mi` that may be looked at, or none when the message is pinned (the first, or one of
 * the last `keepLast`) or not safe to rebuild. Exported so the pin is testable apart from the pairing rule.
 */
export function eligibleResults(msg: SessionMessage, mi: number, total: number, keepLast: number): readonly ToolResultSummary[] {
  const results = msg.toolResults
  if (msg.role !== 'user' || results === undefined) return []
  if (mi === 0 || mi >= total - keepLast) return []
  // The rewritten message is rebuilt from `text` and tool blocks, so one holding an image/pdf result stays as it is.
  return results.some((r) => hasNonText(r.result)) ? [] : results
}

function candidateOf(mi: number, ri: number, r: ToolResultSummary, byId: ReadonlyMap<string, Use>, later: Later, minBytes: number): Candidate | undefined {
  const u = byId.get(r.tool_use_id)
  if (u === undefined || u.msgIndex >= mi || u.failed || r.isError || typeof r.text !== 'string') return undefined
  const bytes = byteLength(r.text)
  if (bytes < minBytes) return undefined
  const c = classify(u, r.text, later)
  if (c === undefined) return undefined
  return {
    key: `${mi}:${ri}`,
    messageIndex: mi,
    resultIndex: ri,
    toolUseId: r.tool_use_id,
    tool: u.tool,
    label: labelOf(u),
    category: c.category,
    reason: c.reason,
    bytes,
    lines: r.text.split('\n').length,
    text: r.text,
    ...(c.persistedPath === undefined ? {} : { persistedPath: c.persistedPath }),
  }
}

/**
 * The tool results that may be replaced by a stub. Never a candidate: the first message, the last `keepLast`
 * messages, an error or a hook denial, a result whose failure shows in its text, assistant or user text
 * (only `toolResults` entries are ever read), and a message that also holds an image or pdf result.
 */
export function planEvictions(messages: readonly SessionMessage[], cfg: EvictConfig): Candidate[] {
  const keepLast = Math.max(0, cfg.keepLast)
  const minBytes = Math.max(MIN_BYTES_FLOOR, cfg.minBytes)
  const uses = collectUses(messages)
  const byId = new Map(uses.map((u) => [u.id, u]))
  const later = indexLater(uses)
  const out: Candidate[] = []
  for (const [mi, msg] of messages.entries()) {
    for (const [ri, r] of eligibleResults(msg, mi, messages.length, keepLast).entries()) {
      const c = candidateOf(mi, ri, r, byId, later, minBytes)
      if (c !== undefined) out.push(c)
    }
  }
  return out
}

/** The text that takes the result's place: what it was, why it is safe to drop, and where the raw lives. */
export function stubFor(c: Candidate, rawPath: string): string {
  return (
    `[touring-compact-evict] ${c.tool} result of ${c.label} evicted at compaction ` +
    `(${fmtBytes(c.bytes)}, ${c.lines} lines; ${c.reason}). ` +
    `Raw output: ${rawPath} - read it only if the old output is needed.`
  )
}

/**
 * The messages with each stubbed result replaced. Only the `toolResults` entry changes (`result`, the stored
 * record that carries the bulk, is dropped with it); a rewritten message loses its `handle`, because a message
 * handed back with its handle is taken as the engine's own, whole. Untouched messages are returned as they came.
 */
export function applyEvictions(messages: readonly SessionMessage[], stubs: ReadonlyMap<string, string>): SessionMessage[] {
  return messages.map((m, mi) => {
    if (m.toolResults === undefined) return m
    let changed = false
    const toolResults = m.toolResults.map((r, ri) => {
      const stub = stubs.get(`${mi}:${ri}`)
      if (stub === undefined) return r
      changed = true
      const { result: _bulk, ...rest } = r
      return { ...rest, text: stub }
    })
    if (!changed) return m
    const { handle: _handle, ...rest } = m
    return { ...rest, toolResults }
  })
}
