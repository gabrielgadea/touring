#!/usr/bin/env node
/**
 * Boundary Violation Summary — parses S1 boundary events and summarizes violations.
 *
 * Input:  {"activity_log": "/path/to/activity.jsonl"}
 * Output: {"violations": [{"task_id": "t-1", "violation_type": "single_writer", "event_seq": 42}], "total": 1}
 * Return: 1 if violations found, 0 if clean
 */

const fs = require("fs");
const path = require("path");

function parseActivityLog(logPath) {
  const events = [];
  if (!fs.existsSync(logPath)) {
    return events;
  }
  const content = fs.readFileSync(logPath, "utf8");
  const lines = content.split("\n");
  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    try {
      events.push(JSON.parse(trimmed));
    } catch (_) {
      // skip malformed lines
    }
  }
  return events;
}

function detectViolations(events) {
  const violations = [];
  const writerTasks = new Set();

  for (const event of events) {
    const eventSeq = event.event_seq;
    const taskId = event.task_id;
    const eventType = event.event_type || event.type || "";

    // Detect single-writer violations: same task appears in conflicting phase contexts
    if (eventType.includes("write") || eventType.includes("edit")) {
      if (writerTasks.has(taskId) && !eventType.includes("complete")) {
        violations.push({
          task_id: taskId,
          violation_type: "single_writer",
          event_seq: eventSeq || 0,
        });
      }
      writerTasks.add(taskId);
    }

    // Detect boundary_cross: event_seq jumps non-sequentially
    if (eventSeq !== undefined) {
      const lastSeq = detectViolations._lastSeq || 0;
      if (lastSeq !== 0 && eventSeq < lastSeq) {
        violations.push({
          task_id: taskId || "unknown",
          violation_type: "sequence_regression",
          event_seq: eventSeq,
        });
      }
      detectViolations._lastSeq = eventSeq;
    }
  }

  return violations;
}

function main() {
  let inputObj;
  const args = process.argv.slice(2);
  if (args.length > 0) {
    inputObj = JSON.parse(args[0]);
  } else {
    const stdin = fs.readFileSync("/dev/stdin", "utf8");
    inputObj = JSON.parse(stdin);
  }

  const logPath = inputObj.activity_log || "";
  const events = parseActivityLog(logPath);
  const violations = detectViolations(events);

  const result = {
    violations,
    total: violations.length,
  };
  console.log(JSON.stringify(result));
  process.exit(violations.length > 0 ? 1 : 0);
}

if (require.main === module) {
  main();
}

module.exports = { parseActivityLog, detectViolations };
