"""Which `{{nodes.*}}` reference forms do real corpora use? Usage: ref_forms.py dir [dir ...]"""
import sys, re, pathlib, collections
FORM = re.compile(r"\{\{nodes\.[^}]*\}\}")
cnt = collections.Counter()
examples = {}
for d in sys.argv[1:]:
    for f in sorted(pathlib.Path(d).rglob("*.toml")):
        txt = f.read_text(encoding="utf-8", errors="ignore")
        for m in FORM.finditer(txt):
            ref = m.group(0)
            body = ref[len("{{nodes."):-2]
            if body.endswith(".summary"):
                form = "X.summary"
            elif body.endswith(".full_ref"):
                form = "X.full_ref"
            elif body.endswith(".omitted_bytes"):
                form = "X.omitted_bytes"
            elif ".facts." in body:
                form = "X.facts.k"
            else:
                form = "X (bare)"
            cnt[form] += 1
            examples.setdefault(form, []).append(f"{f.name}: {ref}")
for form, n in cnt.most_common():
    print(f"{form:18s} {n:4d}   e.g. {examples[form][0]}")
