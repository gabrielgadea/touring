#!/usr/bin/env node
/**
 * Crates Size Via CLI Wrapper — returns crate size distribution.
 *
 * Input:  {"workspace": "/home/gabrielgadea/.claude/rust"}
 * Output: {"total_crates": 24, "median_loc": 1847, "largest": {"name": "touring-core", "loc": 48200}}
 * Return: 1 always (output always available)
 */

// Security (OWASP A03 / CWE-78): use execFileSync with an argv array, never a
// shell string. `cratePath` derives from external input (`inputObj.workspace`),
// so interpolating it into a shell command was a command-injection vector.
const { execFileSync } = require("child_process");
const path = require("path");
const fs = require("fs");

function getCrateList(workspace) {
  const tomlPath = path.join(workspace, "Cargo.toml");
  if (!fs.existsSync(tomlPath)) {
    return [];
  }
  const content = fs.readFileSync(tomlPath, "utf8");
  const memberRegex = /^members\s*=\s*\[([\s\S]*?)\]/m;
  const match = content.match(memberRegex);
  if (!match) return [];
  const membersStr = match[1];
  const crateNames = [];
  const memberRegex2 = /"([^"]+)"/g;
  let m;
  while ((m = memberRegex2.exec(membersStr)) !== null) {
    crateNames.push(m[1]);
  }
  return crateNames;
}

function measureCrate(workspace, crateName) {
  try {
    const cratePath = path.join(workspace, "crates", crateName, "src");
    if (!fs.existsSync(cratePath)) {
      return 0;
    }
    // No shell: argv array is immune to metacharacter injection from cratePath.
    const output = execFileSync(
      "find",
      [cratePath, "-name", "*.rs", "-type", "f", "-exec", "wc", "-l", "{}", "+"],
      { encoding: "utf8", timeout: 10000 }
    );
    const lines = output.trim().split("\n");
    const lastLine = lines[lines.length - 1] || "";
    const loc = parseInt(lastLine.trim().split(/\s+/)[0], 10);
    return isNaN(loc) ? 0 : loc;
  } catch (_) {
    return 0;
  }
}

function computeSizeDistribution(workspace) {
  const crateNames = getCrateList(workspace);
  const crates = [];
  for (const name of crateNames) {
    const loc = measureCrate(workspace, name);
    if (loc > 0) {
      crates.push({ name, loc });
    }
  }

  if (crates.length === 0) {
    return { total_crates: 0, median_loc: 0, largest: null };
  }

  crates.sort((a, b) => a.loc - b.loc);
  const medianIdx = Math.floor(crates.length / 2);
  const medianLoc = crates.length % 2 === 0
    ? Math.round((crates[medianIdx - 1].loc + crates[medianIdx].loc) / 2)
    : crates[medianIdx].loc;
  const largest = crates[crates.length - 1];

  return {
    total_crates: crates.length,
    median_loc: medianLoc,
    largest: { name: largest.name, loc: largest.loc },
  };
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

  const workspace = inputObj.workspace || "/home/gabrielgadea/.claude/rust";
  const dist = computeSizeDistribution(workspace);
  console.log(JSON.stringify(dist));
  process.exit(1);
}

if (require.main === module) {
  main();
}

module.exports = { getCrateList, measureCrate, computeSizeDistribution };
