// The planner on its own: which results are candidates, and what applying a plan does to a message list.
import { expect, test } from 'claude-code/testing'
import type { SessionMessage } from 'claude-code'
import { applyEvictions, DEFAULT_KEEP_LAST, DEFAULT_MIN_BYTES, eligibleResults, hasFailureText, intOr, isOn, planEvictions, stubFor } from '../hooks/evict.ts'
import { assistantText, BASH, big, calls, EDIT, READ, results, tail, userText } from './helpers.ts'

const CFG = { keepLast: DEFAULT_KEEP_LAST, minBytes: DEFAULT_MIN_BYTES }
const reasons = (ms: SessionMessage[], cfg = CFG): string[] => planEvictions(ms, cfg).map((c) => `${c.key}:${c.category}:${c.reason}`)

test('plan_read_then_edit_or_write_is_superseded', () => {
  const afterEdit = [userText('t'), calls(READ('r', '/repo/a.ts')), results({ id: 'r', text: big('a') }), calls(EDIT('e', '/repo/a.ts')), results({ id: 'e', text: 'ok' }), ...tail()]
  expect(reasons(afterEdit)).toEqual(['2:0:superseded-read:edited later'])
  const afterWrite = [userText('t'), calls(READ('r', '/repo/a.ts')), results({ id: 'r', text: big('a') }), calls({ id: 'w', tool: 'Write', input: { file_path: '/repo/a.ts', content: 'x' } }), results({ id: 'w', text: 'ok' }), ...tail()]
  expect(reasons(afterWrite)).toEqual(['2:0:superseded-read:written later'])
})

test('plan_a_failed_or_unrelated_edit_does_not_supersede', () => {
  const failedEdit = [userText('t'), calls(READ('r', '/repo/a.ts')), results({ id: 'r', text: big('a') }), calls({ ...EDIT('e', '/repo/a.ts'), isError: true }), results({ id: 'e', text: 'no match', isError: true }), ...tail()]
  expect(reasons(failedEdit)).toEqual([])
  const otherFile = [userText('t'), calls(READ('r', '/repo/a.ts')), results({ id: 'r', text: big('a') }), calls(EDIT('e', '/repo/b.ts')), results({ id: 'e', text: 'ok' }), ...tail()]
  expect(reasons(otherFile)).toEqual([])
})

test('plan_reread_supersedes_only_the_same_range', () => {
  const same = [userText('t'), calls(READ('r1', '/repo/a.ts')), results({ id: 'r1', text: big('a') }), calls(READ('r2', '/repo/a.ts')), results({ id: 'r2', text: big('a') }), ...tail()]
  expect(reasons(same)).toEqual(['2:0:superseded-read:re-read later'])
  const other = [userText('t'), calls(READ('r1', '/repo/a.ts', { offset: 1, limit: 50 })), results({ id: 'r1', text: big('a') }), calls(READ('r2', '/repo/a.ts', { offset: 900, limit: 50 })), results({ id: 'r2', text: big('b') }), ...tail()]
  expect(reasons(other)).toEqual([])
})

test('plan_the_first_and_the_last_n_messages_are_never_eligible', () => {
  const msg = results({ id: 'x', text: big('x') })
  expect(eligibleResults(msg, 0, 20, 6)).toEqual([])
  expect(eligibleResults(msg, 1, 20, 6)).toHaveLength(1)
  expect(eligibleResults(msg, 13, 20, 6)).toHaveLength(1)
  expect(eligibleResults(msg, 14, 20, 6)).toEqual([])
  expect(eligibleResults(msg, 19, 20, 6)).toEqual([])
  expect(eligibleResults(msg, 14, 20, 0)).toHaveLength(1)
  // an assistant message has no results to look at, whatever its position
  expect(eligibleResults(assistantText('t'), 3, 20, 6)).toEqual([])
})

test('plan_an_error_on_either_side_keeps_the_result', () => {
  const callErrors = [userText('t'), calls({ ...BASH('c1', 'synthetic-cmd'), isError: true }), results({ id: 'c1', text: big('out') }), calls(BASH('c2', 'synthetic-cmd')), results({ id: 'c2', text: 'x' }), ...tail()]
  expect(reasons(callErrors)).toEqual([])
  const resultErrors = [userText('t'), calls(BASH('c1', 'synthetic-cmd')), results({ id: 'c1', text: big('out'), isError: true }), calls(BASH('c2', 'synthetic-cmd')), results({ id: 'c2', text: 'x' }), ...tail()]
  expect(reasons(resultErrors)).toEqual([])
})

test('plan_a_read_nobody_touched_again_stays', () => {
  expect(reasons([userText('t'), calls(READ('r', '/repo/a.ts')), results({ id: 'r', text: big('a') }), ...tail()])).toEqual([])
})

test('plan_small_results_are_not_worth_a_stub', () => {
  const small = [userText('t'), calls(READ('r', '/repo/a.ts')), results({ id: 'r', text: 'tiny' }), calls(EDIT('e', '/repo/a.ts')), results({ id: 'e', text: 'ok' }), ...tail()]
  expect(reasons(small)).toEqual([])
  // even a configured minimum of 0 cannot go under the floor
  expect(reasons(small, { keepLast: 6, minBytes: 0 })).toEqual([])
})

test('plan_repeated_command_needs_a_successful_repeat', () => {
  const failedRepeat = [userText('t'), calls(BASH('c1', 'synthetic-cmd')), results({ id: 'c1', text: big('out') }), calls({ ...BASH('c2', 'synthetic-cmd'), isError: true }), results({ id: 'c2', text: 'boom', isError: true }), ...tail()]
  expect(reasons(failedRepeat)).toEqual([])
  const okRepeat = [userText('t'), calls(BASH('c1', 'synthetic-cmd')), results({ id: 'c1', text: big('out') }), calls(BASH('c2', ' synthetic-cmd ')), results({ id: 'c2', text: 'x' }), ...tail()]
  expect(reasons(okRepeat)).toEqual(['2:0:repeated-command:repeated later'])
})

test('plan_a_persisted_test_run_is_never_a_candidate', () => {
  const path = '/home/synthetic/tool-results/t.txt'
  const preview = `<persisted-output>\nOutput too large (90KB). Full output saved to: ${path}\n\nPreview:\n${big('head', 60)}\n</persisted-output>`
  const build = (cmd: string) => [userText('t'), calls(BASH('c', cmd)), results({ id: 'c', text: preview }), ...tail()]
  expect(reasons(build('synthetic-dump --all'))).toEqual(['2:0:persisted-output:already persisted on disk'])
  expect(reasons(build('cargo test --workspace'))).toEqual([])
  expect(planEvictions(build('synthetic-dump --all'), CFG)[0]?.persistedPath).toBe(path)
})

test('plan_a_message_with_an_image_result_is_not_rebuilt', () => {
  const ms = [
    userText('t'),
    calls(READ('r1', '/repo/a.ts'), READ('r2', '/repo/shot.png')),
    results({ id: 'r1', text: big('a') }, { id: 'r2', text: '', result: { type: 'image', file: { base64: 'AAAA' } } }),
    calls(EDIT('e', '/repo/a.ts')),
    results({ id: 'e', text: 'ok' }),
    ...tail(),
  ]
  expect(reasons(ms)).toEqual([])
})

test('plan_only_tool_results_are_read_never_assistant_or_user_text', () => {
  const longText = big('prose', 200)
  const ms = [userText(longText), assistantText(longText), calls(READ('r', '/repo/a.ts')), results({ id: 'r', text: big('a') }), calls(EDIT('e', '/repo/a.ts')), results({ id: 'e', text: 'ok' }), ...tail()]
  const plan = planEvictions(ms, CFG)
  expect(plan).toHaveLength(1)
  const out = applyEvictions(ms, new Map(plan.map((c) => [c.key, stubFor(c, '/raw/x.txt')])))
  expect(out[0]).toEqual(ms[0])
  expect(out[1]).toEqual(ms[1])
  expect(out[2]).toEqual(ms[2])
})

test('apply_drops_the_handle_and_the_stored_record_and_keeps_the_rest', () => {
  const ms = [
    userText('t'),
    calls(READ('r', '/repo/a.ts')),
    { ...results({ id: 'r', text: big('a'), result: { type: 'text', file: { content: big('a') } } }), text: 'side text' },
    calls(EDIT('e', '/repo/a.ts')),
    results({ id: 'e', text: 'ok' }),
    ...tail(),
  ]
  const out = applyEvictions(ms, new Map([['2:0', 'STUB']]))
  const r = out[2]?.toolResults?.[0]
  expect(r?.text).toBe('STUB')
  expect(r?.tool_use_id).toBe('r')
  expect(r?.isError).toBe(false)
  expect(r?.result).toBeUndefined()
  expect('result' in (r ?? {})).toBe(false)
  expect(out[2]?.handle).toBeUndefined()
  expect(out[2]?.text).toBe('side text')
  expect(out[2]?.role).toBe('user')
  // the input list is not mutated
  expect(ms[2]?.toolResults?.[0]?.text).not.toBe('STUB')
  expect(ms[2]?.handle).toBeDefined()
})

test('plan_failure_scan_is_linear_on_a_huge_whitespace_result', () => {
  // 1.6 MB of blank lines and spaces: a scan that backtracks over whitespace would not finish inside a hook's 10 s
  const blank = ' \t\n'.repeat(530_000)
  const ms = [userText('t'), calls(BASH('c1', 'synthetic-cmd')), results({ id: 'c1', text: blank }), calls(BASH('c2', 'synthetic-cmd')), results({ id: 'c2', text: 'x' }), ...tail()]
  const started = performance.now()
  expect(reasons(ms)).toEqual(['2:0:repeated-command:repeated later'])
  expect(performance.now() - started).toBeLessThan(2000)
  // the verdict of a long log sits in its tail
  expect(hasFailureText(`${big('log', 8000)}\n3 failed, 10 passed`)).toBe(true)
  expect(hasFailureText(`${big('log', 8000)}\n0 failed, 13 passed`)).toBe(false)
})

test('flag_and_numbers_parse_strictly', () => {
  expect(['on', 'ON', ' 1 ', 'true', 'Yes'].map(isOn)).toEqual([true, true, true, true, true])
  expect([undefined, '', 'off', '0', 'dry', 'onn'].map(isOn)).toEqual([false, false, false, false, false, false])
  expect(intOr('0', 6)).toBe(0)
  expect(intOr('12', 6)).toBe(12)
  expect(intOr(' 3 ', 6)).toBe(3)
  expect([undefined, '', '-1', '2.5', 'x'].map((v) => intOr(v, 6))).toEqual([6, 6, 6, 6, 6])
})
