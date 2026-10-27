// touring-compact-evict (CC 2.1.311). A deterministic mod: at the compaction boundary only, tool results
// that were already superseded are replaced by a stub that points at the raw. No model call, nothing leaves
// the machine, and it does nothing unless TOURING_COMPACT_EVICT is on.
//
// Contract used (claude-code.d.ts, CC 2.1.311): `session.compact` takes `{ trigger, agentId?, instructions?,
// messages }` and `next({ ...e, messages })` changes what is summarized; `$.env`, `$.fs` and `$.process.run`
// are the only calls made. A hook that throws is skipped and the compaction goes on unchanged (fail-open).
import type { EngineInterface, Register } from 'claude-code'
import { applyEvictions, DEFAULT_KEEP_LAST, DEFAULT_MIN_BYTES, intOr, isOn, planEvictions, stubFor } from './evict.ts'
import type { Candidate } from './evict.ts'

type Disk = EngineInterface

const sha256Hex = async (text: string): Promise<string> => {
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text))
  return Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, '0')).join('')
}

/** Claude Code already persisted these outputs: the pointer is the file it wrote, if that file is still there. */
async function persistedPaths($: Disk, plan: readonly Candidate[]): Promise<Map<string, string>> {
  const out = new Map<string, string>()
  for (const c of plan) {
    if (c.persistedPath !== undefined && (await $.fs.exists(c.persistedPath))) out.set(c.key, c.persistedPath)
  }
  return out
}

/** The 0700 directory the raws go in, made before the first one is written so none is ever readable by others. */
async function privateDir($: Disk): Promise<string | undefined> {
  const home = await $.env.get('HOME')
  if (home === undefined || !home.startsWith('/')) return undefined
  const dir = `${home}/.claude/touring/evicted`
  const made = await $.process.run(['mkdir', '-p', '-m', '700', dir])
  return made.exitCode === 0 ? dir : undefined
}

/**
 * Puts one raw in `dir` under the hash of its content and returns the path, or undefined when it cannot be
 * stored. A file already there with the right size is reused; a link or a directory is never written through.
 */
async function storeRaw($: Disk, dir: string, c: Candidate, fresh: string[]): Promise<string | undefined> {
  const path = `${dir}/${(await sha256Hex(c.text)).slice(0, 32)}.txt`
  try {
    const st = await $.fs.stat(path).catch(() => undefined)
    if (st !== undefined && st.isLink) return undefined
    if (st !== undefined && st.kind === 'file' && st.size === c.bytes) return path
    if (st !== undefined && st.kind !== 'file') return undefined
    await $.fs.write(path, c.text)
    fresh.push(path)
    return path
  } catch {
    return undefined
  }
}

/**
 * Where each candidate's raw lives, keyed by `Candidate.key`. A candidate whose raw cannot be put in a
 * private place is left out, so its result stays in the transcript as it was: no pointer, no eviction.
 */
async function rawPaths($: Disk, plan: readonly Candidate[]): Promise<Map<string, string>> {
  const out = await persistedPaths($, plan)
  const own = plan.filter((c) => c.persistedPath === undefined)
  const dir = own.length > 0 ? await privateDir($) : undefined
  if (dir === undefined) return out
  const fresh: string[] = []
  const stored = new Map<string, string>()
  for (const c of own) {
    const path = await storeRaw($, dir, c, fresh)
    if (path !== undefined) stored.set(c.key, path)
  }
  // 0600 on what this run wrote; if that fails the files stay in the 0700 dir but no pointer is handed out.
  const locked = fresh.length === 0 || (await $.process.run(['chmod', '600', ...fresh])).exitCode === 0
  if (locked) for (const [key, path] of stored) out.set(key, path)
  return out
}

export const register: Register = (on) => {
  // Only a compaction that installs: not `precompute` (ahead of time, would break the cached prefix mid-session).
  on('session.compact', { trigger: ['manual', 'auto', 'plugin'] }, async ($, e, next) => {
    if (!isOn(await $.env.get('TOURING_COMPACT_EVICT'))) return next(e)
    const cfg = {
      keepLast: intOr(await $.env.get('TOURING_COMPACT_EVICT_KEEP_LAST'), DEFAULT_KEEP_LAST),
      minBytes: intOr(await $.env.get('TOURING_COMPACT_EVICT_MIN_BYTES'), DEFAULT_MIN_BYTES),
    }
    const plan = planEvictions(e.messages, cfg)
    if (plan.length === 0) return next(e)
    const paths = await rawPaths($, plan)
    const stubs = new Map<string, string>()
    for (const c of plan) {
      const path = paths.get(c.key)
      if (path !== undefined) stubs.set(c.key, stubFor(c, path))
    }
    return stubs.size === 0 ? next(e) : next({ ...e, messages: applyEvictions(e.messages, stubs) })
  }).catch(($, e, next) => next(e))
}
