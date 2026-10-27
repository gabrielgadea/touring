// Synthetic fixtures and the fake engine the tests stand on. Nothing here is copied from a real transcript.
import type { On, SessionMessage } from 'claude-code'

let serial = 0

/** A text of about `lines * 60` bytes, distinct per tag. */
export const big = (tag: string, lines = 120): string =>
  Array.from({ length: lines }, (_, i) => `${tag} synthetic line ${i} .......................................`).join('\n')

export const userText = (text: string): SessionMessage => ({ role: 'user', text, toolUses: [], handle: `h${++serial}` })
export const assistantText = (text: string): SessionMessage => ({ role: 'assistant', text, toolUses: [], handle: `h${++serial}` })

export type Call = { id: string; tool: string; input: Record<string, unknown>; text?: string; isError?: boolean }

/** An assistant message that makes tool calls; `text` is the outcome the transcript already holds. */
export const calls = (...cs: Call[]): SessionMessage => ({
  role: 'assistant',
  text: '',
  toolUses: cs.map((c) => ({
    tool_use_id: c.id,
    tool: c.tool,
    input: c.input,
    text: c.text ?? 'ok',
    ...(c.isError === true ? { isError: true as const } : {}),
  })),
  handle: `h${++serial}`,
})

export type Res = { id: string; text: string; isError?: boolean; result?: unknown }

/** A user message that carries tool results. */
export const results = (...rs: Res[]): SessionMessage => ({
  role: 'user',
  text: '',
  toolUses: [],
  toolResults: rs.map((r) => ({
    tool_use_id: r.id,
    text: r.text,
    isError: r.isError === true,
    ...(r.result === undefined ? {} : { result: r.result }),
  })),
  handle: `h${++serial}`,
})

/** `k` plain messages that fill the protected tail. */
export const tail = (k = 6): SessionMessage[] =>
  Array.from({ length: k }, (_, i) => (i % 2 === 0 ? userText(`synthetic follow-up ${i}`) : assistantText(`synthetic answer ${i}`)))

export const READ = (id: string, file: string, extra: Record<string, unknown> = {}): Call => ({ id, tool: 'Read', input: { file_path: file, ...extra } })
export const EDIT = (id: string, file: string): Call => ({ id, tool: 'Edit', input: { file_path: file, old_string: 'a', new_string: 'b' } })
export const BASH = (id: string, command: string): Call => ({ id, tool: 'Bash', input: { command } })

const encoder = new TextEncoder()

/**
 * Registers, beneath the plugin, the engine's side of every call the mod makes: an in-memory file system, a
 * recorder for `$.process.run`, and the `session.compact` bottom that hands back what the plugin passed down.
 */
export function engineBelow(on: On, files: Map<string, string> = new Map(), failWrite = false) {
  const timeline: string[] = []
  const ran: string[][] = []
  const seen: { messages?: readonly SessionMessage[] } = {}
  // A stand-in for a call on `$` answers `{ value }` (what the call resolves to) or `{ deny }` (it rejects).
  on('fs.exists', (_$, e) => ({ value: files.has(e.path) }))
  on('fs.stat', (_$, e) => {
    const text = files.get(e.path)
    if (text === undefined) return { deny: 'ENOENT' }
    return { value: { kind: 'file' as const, size: encoder.encode(text).length, mtimeMs: 0, isLink: false } }
  })
  on('fs.write', (_$, e) => {
    if (failWrite) return { deny: 'ENOSPC' }
    files.set(e.path, e.text)
    timeline.push(`write:${e.path}`)
    return { value: undefined }
  })
  on('process.run', (_$, e) => {
    ran.push([...e.argv])
    timeline.push(`run:${e.argv[0]}`)
    return { value: { exitCode: 0, stdout: '', stderr: '', isStdoutTruncated: false, isStderrTruncated: false } }
  })
  on('session.compact', (_$, e) => {
    timeline.push('compact')
    seen.messages = e.messages
    return { messages: e.messages }
  })
  return { files, timeline, ran, seen }
}
