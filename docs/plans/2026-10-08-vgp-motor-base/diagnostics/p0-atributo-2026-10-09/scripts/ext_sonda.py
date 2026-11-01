import json, subprocess, sys, tempfile
from pathlib import Path
ROOT = "/home/gabrielgadea/projects/analise"
CASOS = {
  "from lxml import etree": "from lxml import etree\nprint(etree.fromstring)\n",
  "import lxml.etree as et": "import lxml.etree as et\nprint(et.fromstring)\n",
  "import lxml; lxml.etree": "import lxml\nprint(lxml.etree)\n",
  "from PIL import _imaging": "from PIL import _imaging\n",
  "from charset_normalizer import md__mypyc": "from charset_normalizer import md__mypyc\n",
  "from numpy import linalg": "from numpy import linalg\nprint(linalg.norm)\n",
}
for b in sys.argv[1:]:
    print("==", b.split("/")[-2])
    for k, v in CASOS.items():
        with tempfile.TemporaryDirectory() as d:
            a = Path(d) / "c.py"; a.write_text(v)
            r = subprocess.run([b, "vgp", "check", "--path", "/tmp/x_ext.py", "--content-file", str(a), "--project-root", ROOT, "--lang", "python", "-j"], capture_output=True, text=True)
            j = json.loads(r.stdout)
            print(f"  {k:42s} {j['verdict']:6s}", [(f['severidade'], f['classe'], f['nome']) for f in j['findings']])
