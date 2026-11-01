// End-to-end through the hook: the test raises `session.compact` on the engine's `$`, the plugin sits above
// the fake engine of helpers.ts, and what the plugin passes down is what the summarizer would read.
import { expect, mock, test } from 'claude-code/testing'
import type { SessionMessage } from 'claude-code'
import { assistantText, BASH, big, calls, EDIT, engineBelow, READ, results, tail, userText } from './helpers.ts'

const ARMED = { TOURING_COMPACT_EVICT: 'on', HOME: '/home/synthetic' }
const RAW_DIR = '/home/synthetic/.claude/touring/evicted/'

/** first message, a Read, an edit of the same file, a closing line, then the protected tail. */
function readThenEdit(readText = big('alpha')): SessionMessage[] {
  return [
    userText('synthetic task'),
    calls(READ('r1', '/repo/a.ts')),
    results({ id: 'r1', text: readText }),
    calls(EDIT('e1', '/repo/a.ts')),
    results({ id: 'e1', text: 'The file /repo/a.ts has been updated.' }),
    assistantText('synthetic edit done'),
    ...tail(),
  ]
}

const textOf = (m: SessionMessage | undefined, i = 0): string => m?.toolResults?.[i]?.text ?? ''

test('test_evicts_read_superseded_by_later_edit', async ($, on) => {
  mock.env(on, ARMED)
  const engine = engineBelow(on)
  const input = readThenEdit()
  await $.session.compact({ trigger: 'auto', messages: input })
  const out = engine.seen.messages ?? []
  expect(out).toHaveLength(input.length)
  expect(textOf(out[2])).toContain('[touring-compact-evict] Read result of /repo/a.ts evicted at compaction')
  expect(textOf(out[2])).toContain('edited later')
  // the edit's own result and every other message are untouched, handle included
  expect(out[4]).toEqual(input[4])
  expect(out[1]).toEqual(input[1])
  // the rewritten message lost its handle (the engine would otherwise keep its own copy), the others keep theirs
  expect(out[2]?.handle).toBeUndefined()
  expect(out[3]?.handle).toBe(input[3]?.handle)
})

test('test_never_evicts_failure_results', async ($, on) => {
  mock.env(on, ARMED)
  const engine = engineBelow(on)
  const redTest = big('suite', 80) + '\nFAILED tests/test_x.py::test_y - AssertionError\n3 failed, 10 passed'
  const input: SessionMessage[] = [
    userText('synthetic task'),
    // a Read that failed, then the file is edited: the error stays
    calls({ ...READ('f1', '/repo/missing.ts'), isError: true, text: 'File does not exist' }),
    results({ id: 'f1', text: big('missing'), isError: true }),
    // a command that exited non-zero, repeated later
    calls({ ...BASH('b1', 'synthetic-build --all'), isError: true }),
    results({ id: 'b1', text: big('build-log'), isError: true }),
    calls(BASH('b2', 'synthetic-build --all')),
    results({ id: 'b2', text: 'build ok' }),
    // a red test run with exit 0 (piped through `|| true`), repeated later
    calls(BASH('t1', 'synthetic-runner suite || true')),
    results({ id: 't1', text: redTest }),
    calls(BASH('t2', 'synthetic-runner suite || true')),
    results({ id: 't2', text: 'all green' }),
    // a hook denial: an error result, repeated later
    calls({ ...BASH('d1', 'synthetic-denied-cmd'), isError: true }),
    results({ id: 'd1', text: '[hook] blocked: use the batch route. '.repeat(100), isError: true }),
    calls(BASH('d2', 'synthetic-denied-cmd')),
    results({ id: 'd2', text: 'fine' }),
    ...tail(),
  ]
  await $.session.compact({ trigger: 'auto', messages: input })
  expect(engine.seen.messages).toEqual(input)
  expect(engine.ran).toHaveLength(0)
  expect(engine.files.size).toBe(0)
})

/** A long first message that names a file, a Read of another file, and an edit of it close to the end. */
function pinnedScenario(): SessionMessage[] {
  return [
    userText(`synthetic task: ${big('prompt', 80)}`),
    assistantText('synthetic plan'),
    userText('synthetic filler'),
    assistantText('synthetic filler'),
    calls(READ('n1', '/repo/recent.ts')),
    results({ id: 'n1', text: big('recent') }),
    calls(EDIT('n2', '/repo/recent.ts')),
    results({ id: 'n2', text: 'updated' }),
    assistantText('closing'),
    userText('closing'),
  ]
}

test('test_keeps_pinned_first_and_recent', async ($, on) => {
  mock.env(on, ARMED)
  const engine = engineBelow(on)
  const input = pinnedScenario()
  await $.session.compact({ trigger: 'manual', messages: input })
  // the superseded Read result is index 5 of 10, inside the last 6 messages: kept; the first message is kept too
  expect(engine.seen.messages).toEqual(input)
  expect(engine.ran).toHaveLength(0)
})

test('test_closing_the_recent_window_evicts_but_never_the_first_message', async ($, on) => {
  mock.env(on, { ...ARMED, TOURING_COMPACT_EVICT_KEEP_LAST: '0' })
  const engine = engineBelow(on)
  const input = pinnedScenario()
  await $.session.compact({ trigger: 'manual', messages: input })
  const out = engine.seen.messages ?? []
  expect(textOf(out[5])).toContain('evicted at compaction')
  expect(out[0]).toEqual(input[0])
  expect(out[8]).toEqual(input[8])
})

test('test_stub_carries_pointer_to_raw', async ($, on) => {
  mock.env(on, ARMED)
  const engine = engineBelow(on)
  const raw = big('alpha')
  const input = readThenEdit(raw)
  await $.session.compact({ trigger: 'auto', messages: input })
  const stub = textOf(engine.seen.messages?.[2])
  const path = /Raw output: (\S+) - read it only/.exec(stub)?.[1] ?? ''
  expect(path.startsWith(RAW_DIR)).toBe(true)
  expect(path).toMatch(/\/[0-9a-f]{32}\.txt$/)
  // the raw is the original text, byte for byte, and it was written before the compaction went on
  expect(engine.files.get(path)).toBe(raw)
  expect(engine.timeline.indexOf(`write:${path}`)).toBeGreaterThanOrEqual(0)
  expect(engine.timeline.indexOf(`write:${path}`)).toBeLessThan(engine.timeline.indexOf('compact'))
  // private by construction: a 0700 directory first, 0600 on the file after
  expect(engine.ran[0]).toEqual(['mkdir', '-p', '-m', '700', RAW_DIR.slice(0, -1)])
  expect(engine.ran[1]).toEqual(['chmod', '600', path])
  // the stub says what it replaced, so it is short next to it
  expect(stub).toContain('Read result of /repo/a.ts')
  expect(stub.length).toBeLessThan(raw.length / 10)
})

test('test_off_by_default_passthrough', async ($, on) => {
  mock.env(on, { HOME: '/home/synthetic' })
  const engine = engineBelow(on)
  const input = readThenEdit()
  await $.session.compact({ trigger: 'auto', messages: input })
  expect(engine.seen.messages).toEqual(input)
  expect(engine.ran).toHaveLength(0)
  expect(engine.files.size).toBe(0)
})

for (const value of ['off', '0', 'false', '', 'dry']) {
  test(`test_flag_value_${JSON.stringify(value)}_is_passthrough`, async ($, on) => {
    mock.env(on, { ...ARMED, TOURING_COMPACT_EVICT: value })
    const engine = engineBelow(on)
    const input = readThenEdit()
    await $.session.compact({ trigger: 'auto', messages: input })
    expect(engine.seen.messages).toEqual(input)
    expect(engine.ran).toHaveLength(0)
  })
}

test('test_precompute_trigger_is_left_alone', async ($, on) => {
  mock.env(on, ARMED)
  const engine = engineBelow(on)
  const input = readThenEdit()
  await $.session.compact({ trigger: 'precompute', messages: input })
  expect(engine.seen.messages).toEqual(input)
  expect(engine.ran).toHaveLength(0)
})

test('test_evicts_repeated_command_and_keeps_the_last_run', async ($, on) => {
  mock.env(on, ARMED)
  const engine = engineBelow(on)
  const input: SessionMessage[] = [
    userText('synthetic task'),
    calls(BASH('c1', 'synthetic-status --all')),
    results({ id: 'c1', text: big('status-old') }),
    calls(BASH('c2', 'synthetic-status --all')),
    results({ id: 'c2', text: big('status-new') }),
    assistantText('synthetic note'),
    ...tail(),
  ]
  await $.session.compact({ trigger: 'auto', messages: input })
  const out = engine.seen.messages ?? []
  expect(textOf(out[2])).toContain('repeated later')
  expect(textOf(out[2])).toContain('`synthetic-status --all`')
  expect(out[4]).toEqual(input[4])
})

test('test_persisted_output_points_at_the_file_claude_code_wrote', async ($, on) => {
  mock.env(on, ARMED)
  const persisted = '/home/synthetic/.claude/projects/synthetic/tool-results/abc123.txt'
  const preview = `<persisted-output>\nOutput too large (48.2KB). Full output saved to: ${persisted}\n\nPreview (first 2KB):\n${big('head', 60)}\n</persisted-output>`
  const input: SessionMessage[] = [
    userText('synthetic task'),
    calls(BASH('l1', 'synthetic-listing --long')),
    results({ id: 'l1', text: preview }),
    assistantText('synthetic note'),
    ...tail(),
  ]
  // the file is on disk: the stub points at it and nothing is copied
  const present = engineBelow(on, new Map([[persisted, 'x'.repeat(48_000)]]))
  await $.session.compact({ trigger: 'auto', messages: input })
  expect(textOf(present.seen.messages?.[2])).toContain(`Raw output: ${persisted} - read it only`)
  expect(present.ran).toHaveLength(0)
  expect(present.timeline).toEqual(['compact'])
})

test('test_persisted_output_gone_from_disk_stays_in_the_transcript', async ($, on) => {
  mock.env(on, ARMED)
  const persisted = '/home/synthetic/.claude/projects/synthetic/tool-results/gone.txt'
  const preview = `<persisted-output>\nOutput too large (48.2KB). Full output saved to: ${persisted}\n\nPreview (first 2KB):\n${big('head', 60)}\n</persisted-output>`
  const input: SessionMessage[] = [
    userText('synthetic task'),
    calls(BASH('l1', 'synthetic-listing --long')),
    results({ id: 'l1', text: preview }),
    assistantText('synthetic note'),
    ...tail(),
  ]
  const absent = engineBelow(on)
  await $.session.compact({ trigger: 'auto', messages: input })
  expect(absent.seen.messages).toEqual(input)
})

test('test_raw_that_cannot_be_written_stays_in_the_transcript', async ($, on) => {
  mock.env(on, ARMED)
  const engine = engineBelow(on, new Map(), true)
  const input = readThenEdit()
  await $.session.compact({ trigger: 'auto', messages: input })
  expect(engine.seen.messages).toEqual(input)
})
