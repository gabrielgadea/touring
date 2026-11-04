#!/usr/bin/env node
/**
 * Crates Size Via CLI Wrapper — returns crate size distribution.
 *
 * Input:  {"workspace": "/path/to/workspace"}   (default: $TOURING_WORKSPACE_ROOT)
 * Output: {"total_crates": 24, "median_loc": 1847, "largest": {"name": "touring-core", "loc": 48200}}
 * Return: 1 always (output always available); 2 when no workspace is derivable
 */

// No subprocess at all: the line count is computed in-process, so nothing derived
// from external input (`inputObj.workspace`, the members list) reaches a shell or an
// argv (OWASP A03 / CWE-78). It also removes the batch bug of `find -exec wc {} +`,
// whose LAST line is only the total of the last batch on a large crate.
const path = require("path");
const fs = require("fs");

/**
 * Workspace member paths (relative, as written in the root Cargo.toml).
 * Comment lines inside `members = [...]` are dropped first: a quoted word in a
 * comment is not a member.
 */
function getCrateList(workspace) {
  const tomlPath = path.join(workspace, "Cargo.toml");
  if (!fs.existsSync(tomlPath)) {
    return [];
  }
  const content = fs.readFileSync(tomlPath, "utf8");
  const match = content.match(/^members\s*=\s*\[([\s\S]*?)\]/m);
  if (!match) return [];
  const body = match[1]
    .split("\n")
    .map((line) => line.replace(/#.*$/, ""))
    .join("\n");
  return [...body.matchAll(/"([^"]+)"/g)].map((m) => m[1]);
}

/** Lines of every `*.rs` file under `dir`, walked in-process (symlinks not followed). */
function countRustLines(dir) {
  let total = 0;
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      total += countRustLines(full);
    } else if (entry.isFile() && entry.name.endsWith(".rs")) {
      const text = fs.readFileSync(full, "utf8");
      if (text.length > 0) {
        total += text.split("\n").length - (text.endsWith("\n") ? 1 : 0);
      }
    }
  }
  return total;
}

/** LOC of the member at `memberPath` (relative to `workspace`); 0 when absent or outside it. */
function measureCrate(workspace, memberPath) {
  const root = path.resolve(workspace);
  const srcPath = path.resolve(root, memberPath, "src");
  if (!srcPath.startsWith(root + path.sep) || !fs.existsSync(srcPath)) {
    return 0;
  }
  try {
    return countRustLines(srcPath);
  } catch (_) {
    return 0;
  }
}

function computeSizeDistribution(workspace) {
  const crateNames = getCrateList(workspace);
  const crates = [];
  for (const member of crateNames) {
    const loc = measureCrate(workspace, member);
    if (loc > 0) {
      crates.push({ name: path.basename(member), loc });
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

  // The workspace is derived, never compiled in: the input field, else the
  // environment. Nothing derivable → fail loud, never a guessed path.
  const workspace = inputObj.workspace || process.env.TOURING_WORKSPACE_ROOT;
  if (!workspace) {
    console.error(
      'crates_size_via_cli_wrapper: no workspace — pass {"workspace": "<path>"} or set TOURING_WORKSPACE_ROOT'
    );
    process.exit(2);
  }
  const dist = computeSizeDistribution(workspace);
  console.log(JSON.stringify(dist));
  process.exit(1);
}

if (require.main === module) {
  main();
}

module.exports = { getCrateList, measureCrate, computeSizeDistribution };
