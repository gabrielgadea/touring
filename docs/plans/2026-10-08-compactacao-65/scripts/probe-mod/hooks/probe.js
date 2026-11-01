// Probe mod (08/10/2026, compaction plan item 9). A hooks module may import only its own files and
// "claude-code" (validator: `cannot import "node:fs"`), so it writes through $.fs and reads its mode
// from probe-mode.json in the session directory: {"mode": "cut"|"observe", "force": true|false}.
const ADVICE = /hook additional context|TOURING SUGGEST|system-reminder|post-edit verification|CODE MODE/i
let forced = false

function text(raw) {
  return typeof raw === 'string' ? raw : (raw && (raw.text ?? raw.content)) || ''
}

async function write($, obj) {
  // read-modify-write: every run is a new process, and a per-process array overwrote the earlier run
  let prior = []
  try { prior = JSON.parse(text(await $.fs.read('compact-probe.json'))) } catch (_) {}
  prior.push({ ts: Date.now(), ...obj })
  try { await $.fs.write('compact-probe.json', JSON.stringify(prior, null, 1)) } catch (_) {}
}

async function config($) {
  try { return JSON.parse(text(await $.fs.read('probe-mode.json'))) } catch (_) { return { mode: 'observe', force: false } }
}

function describe(msgs) {
  const texts = msgs.map(m => String(m.text || ''))
  const withResults = msgs.find(m => (m.toolResults || []).length)
  return {
    n: msgs.length,
    messageKeys: msgs[0] ? Object.keys(msgs[0]) : [],
    resultKeys: withResults ? Object.keys(withResults.toolResults[0]) : [],
    advice_hits: texts.filter(t => ADVICE.test(t)).length,
    toolUses: msgs.reduce((a, m) => a + (m.toolUses || []).length, 0),
    toolResults: msgs.reduce((a, m) => a + (m.toolResults || []).length, 0),
    resultSample: withResults ? JSON.stringify(withResults.toolResults[0]).slice(0, 600) : '',
    sample: texts.map((t, i) => `${i}:${msgs[i].role}:${t.slice(0, 200)}`).join(' || ').slice(0, 3000),
  }
}

export function register(on) {
  on('session.compact', async ($, e, next) => {
    const cfg = await config($)
    const msgs = e.messages || []
    await write($, { event: 'compact', trigger: e.trigger, eventKeys: Object.keys(e), ...describe(msgs) })
    if (cfg.mode === 'cut' && msgs.length >= 3) {
      const kept = [{ ...msgs[0], text: 'PROBE-MARKER-7731 ' + (msgs[0].text || '') }, ...msgs.slice(-2)]
      await write($, { event: 'cut', kept: kept.length, from: msgs.length })
      return { messages: kept }
    }
    return next(e)
  })
  on('turn.complete', async ($, e, next) => {
    const cfg = await config($)
    await write($, { event: 'turn', force: cfg.force, mode: cfg.mode })
    if (cfg.force && !forced) {
      forced = true
      try { await write($, { event: 'usage', usage: await $.session.usage() }) } catch (err) { await write($, { event: 'usage_err', err: String(err) }) }
      try { await $.session.compact(); await write($, { event: 'forced_compact_done' }) } catch (err) { await write($, { event: 'forced_compact_err', err: String(err) }) }
    }
    return next(e)
  })
}
