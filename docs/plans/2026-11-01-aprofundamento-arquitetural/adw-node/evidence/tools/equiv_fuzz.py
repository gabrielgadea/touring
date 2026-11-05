"""Old vs new, same inputs: the pieces that were REWRITTEN must not change behaviour
except where the change is the point.

  render_template          -> must be identical on every input (the runner executes it)
  _namespace_node_refs     -> must be identical on every input
  node_data_reads          -> new may only ADD node names, and only for the forms the old
                              regex mis-parsed (full_ref / omitted_bytes / facts)

    equiv_fuzz.py <old adw.py> <new adw.py> [samples]
"""
import importlib.util, random, sys, collections

OLD, NEW = sys.argv[1], sys.argv[2]
N = int(sys.argv[3]) if len(sys.argv) > 3 else 40000


def load(tag, path):
    spec = importlib.util.spec_from_file_location(tag, path)
    mod = importlib.util.module_from_spec(spec)
    sys.modules[tag] = mod
    spec.loader.exec_module(mod)
    return mod


old, new = load("adw_o", OLD), load("adw_n", NEW)

random.seed(20261102)
NAMES = ["a", "rev", "review", "a.b", "x-y", "A1", "n_1", "summary", "full_ref", "facts", "p.q.r"]
SUFFIX = ["", ".summary", ".full_ref", ".omitted_bytes", ".facts.K", ".facts.", ".facts.a.b",
          ".summary.summary", ".full_ref.summary", ".bogus", ".facts.summary", "."]
OPEN = ["{{nodes.", "{{vars.", "{{ nodes.", "{{nodes .", "{nodes.", "{{inputs.", "{{node.", "{{nodes.."]
CLOSE = ["}}", "}", "} }", "", "}}}", "}}}}"]
GLUE = ["", " ", "x", "\n", "{{", "}}", "{", "}", " and ", "'", '"']


def sample():
    parts = []
    for _ in range(random.randint(1, 4)):
        parts.append(random.choice(GLUE))
        parts.append(random.choice(OPEN) + random.choice(NAMES) + random.choice(SUFFIX) + random.choice(CLOSE))
    parts.append(random.choice(GLUE))
    return "".join(parts)


class Facts(dict):
    def __init__(self, n):
        self.n = n

    def get(self, k, d=None):
        return f"<{self.n}:facts:{k}>"


class Entry(dict):
    def __init__(self, n):
        self.n = n

    def get(self, field, d=None):
        return Facts(self.n) if field == "facts" else f"<{self.n}:{field}>"


class Results(dict):
    def get(self, n, d=None):
        return Entry(n)


VARS = collections.defaultdict(str)


class V(dict):
    def get(self, k, d=None):
        return f"[var:{k}]"


import re
MARK = re.compile(r"<([^:<>]*):(?:summary|full_ref|omitted_bytes|facts)")
EXT = re.compile(r"\.(?:full_ref|omitted_bytes|facts\.)")

mism_render = mism_ns = 0
new_wrong = old_wrong_plain = old_wrong_ext = ext_samples = 0
first_bad = []
for _ in range(N):
    t = sample()
    r_old, r_new = old.render_template(t, Results(), V()), new.render_template(t, Results(), V())
    if r_old != r_new:
        mism_render += 1
        first_bad.append(("render", t, r_old, r_new))
    for local in ({"a"}, {"rev", "a"}, {"p"}):
        n_old, n_new = old._namespace_node_refs(t, "NS", local), new._namespace_node_refs(t, "NS", local)
        if n_old != n_new:
            mism_ns += 1
            first_bad.append(("namespace", t, n_old, n_new))
    # data reads vs the RUNTIME's own answer (render_template run on a recording Results)
    truth = set(MARK.findall(r_old))
    nd = old.Node(name="x", type="code", raw={"command": ["bash", "-c", t], "type": "code"})
    nn = new.Node(name="x", type="code", raw={"command": ["bash", "-c", t], "type": "code"})
    ro, rn = old.node_data_reads(nd), new.node_data_reads(nn)
    has_ext = bool(EXT.search(t))
    ext_samples += has_ext
    if rn != truth:
        new_wrong += 1
        first_bad.append(("new_reads_vs_runtime", t, sorted(truth), sorted(rn)))
    if ro != truth:
        if has_ext:
            old_wrong_ext += 1
        else:
            old_wrong_plain += 1
            first_bad.append(("old_reads_vs_runtime(no ext form)", t, sorted(truth), sorted(ro)))

print(f"samples: {N}  (of which with a full_ref/omitted_bytes/facts form: {ext_samples})")
print(f"render_template mismatches old-vs-new:      {mism_render}")
print(f"_namespace_node_refs mismatches old-vs-new: {mism_ns}  (3 local-name sets per sample)")
print(f"node_data_reads == what the runtime resolves:")
print(f"   NEW wrong: {new_wrong}")
print(f"   OLD wrong on samples WITHOUT an extended form: {old_wrong_plain}")
print(f"   OLD wrong on samples WITH an extended form:    {old_wrong_ext} / {ext_samples}")
for kind, t, a, b in first_bad[:6]:
    print(f"  !! {kind}: {t!r}\n       a={a!r}\n       b={b!r}")
