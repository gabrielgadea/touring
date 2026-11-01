// Valida um `tsconfig` com o PARSER DO PROPRIO COMPILADOR, e nao com um parser proprio.
//
// MEDIDO em 10/10 (duas vezes): `json.loads` reprovou um tsconfig com comentario, que o `tsc`
// aceita (JSONC); e o `jsonc_para_json` que escrevi para consertar isso reprovaria num tsconfig
// com VIRGULA FINAL, que o TypeScript tambem aceita. Cada conserto meu de parser e uma lista nova
// do que ele nao sabe.
//
// Aqui a regua e `ts.parseConfigFileTextToJson`: o MESMO parser que o `tsc` usa. Com isso o
// validador e o compilador concordam POR CONSTRUCAO -- nao existe um tsconfig que o `tsc` aceite
// e o validador recuse, porque e o mesmo codigo rodando.
//
// Protocolo: uma linha JSON por requisicao em `stdin`, uma resposta `#<n bytes>\n<corpo>\n`. O
// framing por comprimento declarado e o mesmo do `oraculo_ls.js`, pelo mesmo motivo medido em
// 10/10: um `readline()` trunca em 65.536 bytes e o `json.loads` estourava "Unterminated string".
const ts = require(process.argv[2]);
let buf = "";

process.stdin.on("data", (d) => {
  buf += d.toString();
  let i;
  while ((i = buf.indexOf("\n")) >= 0) {
    const linha = buf.slice(0, i);
    buf = buf.slice(i + 1);
    if (!linha.trim()) continue;
    const msg = JSON.parse(linha);
    if (msg.op !== "parse-tsconfig") {
      responde({ ok: false, erro: "op desconhecida: " + msg.op });
      continue;
    }
    const r = ts.parseConfigFileTextToJson(msg.nome || "tsconfig.json", msg.texto);
    responde({
      ok: !r.error,
      json: r.config === undefined ? null : r.config,
      erro: r.error ? String(r.error.messageText) : "",
    });
  }
});

function responde(obj) {
  const corpo = JSON.stringify(obj);
  process.stdout.write(`#${Buffer.byteLength(corpo, "utf8")}\n${corpo}\n`);
}