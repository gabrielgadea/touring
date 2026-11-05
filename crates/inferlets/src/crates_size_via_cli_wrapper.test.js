// Run: node --test crates/inferlets/src/crates_size_via_cli_wrapper.test.js
const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { getCrateList, measureCrate, computeSizeDistribution } = require("./crates_size_via_cli_wrapper.js");

function workspace(files) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "crates-size-"));
  for (const [rel, text] of Object.entries(files)) {
    fs.mkdirSync(path.dirname(path.join(root, rel)), { recursive: true });
    fs.writeFileSync(path.join(root, rel), text);
  }
  return root;
}

const TOML = `[workspace]
members = [
    "crates/a",
    # "crates/ghost" was removed — a quoted word in a comment is not a member
    "crates/b",
]
`;

test("members are paths, comments are ignored", () => {
  const ws = workspace({ "Cargo.toml": TOML });
  assert.deepEqual(getCrateList(ws), ["crates/a", "crates/b"]);
});

test("members are measured at their own path and named by basename", () => {
  const ws = workspace({
    "Cargo.toml": TOML,
    "crates/a/src/lib.rs": "fn a() {}\n",
    "crates/b/src/lib.rs": "1\n2\n3\n",
    "crates/b/src/deep/mod.rs": "4\n5",
    "crates/b/src/notes.txt": "ignored\n",
  });
  assert.equal(measureCrate(ws, "crates/b"), 5);
  assert.deepEqual(computeSizeDistribution(ws), {
    total_crates: 2,
    median_loc: 3,
    largest: { name: "b", loc: 5 },
  });
});

test("a member path that escapes the workspace is not measured", () => {
  const ws = workspace({ "Cargo.toml": TOML });
  const outside = workspace({ "src/lib.rs": "x\n" });
  assert.equal(measureCrate(ws, path.relative(ws, outside)), 0);
});
