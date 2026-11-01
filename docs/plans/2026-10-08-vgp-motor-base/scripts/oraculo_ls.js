// Oraculo de DELTA por processo node persistente: ts.createLanguageService em vez de `tsc` por
// edicao. A coordenadora mediu ~9 s x 2 x 300 por linguagem com o `tsc`; aqui cada par e ms.
//
// Protocolo (JSON lines em stdin, JSON lines em stdout):
//   {"op":"abrir","raiz":"<dir>","tsconfig":"<path>"}
//   {"op":"delta","arquivo":"<rel>","antes":"<conteudo ou null>","depois":"<conteudo>"}
//   {"op":"fechar"}
//
// Resposta de `delta`: {"ok":bool,"novos":[{cod,lin,msg}],"perdidos":[...],"motivo":"..."}

const fs = require("fs");
const path = require("path");

// O `typescript` e o do PROPRIO repo rotulado, nao um fixo: o LanguageService tem que ser o mesmo
// que o `tsc` daquele projeto, senao a equivalencia provada (20/20) vale so para o `frontend`.
// `abrir` recebe `ts_node`; sem ele, cai no do `frontend` (o historico).
// MEDIDO em 10/10: este era o caminho de OUTRA SESSAO. Fica como ultimo recurso, e o
// `abrir` o substitui pelo `typescript` do repo rotulado — mas se o repo nao tiver um,
// o oraculo precisa de ALGUM typescript, e um caminho morto aqui e um oraculo morto.
let TS_NODE = "/home/gabrielgadea/projects/analise/frontend/node_modules/typescript";
let ts = require(TS_NODE);

let raiz = null;
let servico = null;
let versao = 0;
let tsconfig = null;

// Um "arquivo virtual" sobreposto: o host le do disco, exceto o que foi injetado.
const injetado = new Map(); // caminho absoluto -> texto

function abs(p) {
  return path.isAbsolute(p) ? p : path.join(raiz, p);
}

function versaoDe(absPath) {
  return injetado.has(absPath) ? versao : 0;
}

function scriptKindDe(f) {
  if (/\.tsx$/.test(f)) return ts.ScriptKind.TSX;
  if (/\.jsx$/.test(f)) return ts.ScriptKind.JSX;
  if (/\.js$|\.mjs$|\.cjs$/.test(f)) return ts.ScriptKind.JS;
  if (/\.json$/.test(f)) return ts.ScriptKind.JSON;
  return ts.ScriptKind.TS;
}

function criarHost() {
  return {
    // MEDIDO em 09/10: devolver SO os arquivos injetados dava `[]` do LS onde o `tsc` via
    // TS2339. O programa do LanguageService e construido a partir de `getScriptFileNames`;
    // com um arquivo so, o resto do projeto (e os tipos) fica de fora e nada e analisado.
    getScriptFileNames: () => {
      // MEDIDO em 10/10: este metodo NAO pode LANÇAR. O TypeScript chama ele de dentro do
      // `getSourceFileWithCache`, durante o `getSemanticDiagnostics` — e uma excecao aqui sai do
      // `try/catch` do handler de stdin (que so envolve a chamada SINCRONA), mata o listener sem
      // responder, e deixa o processo com o event loop ocioso e `utime` congelado. E o sintoma do
      // `v8` e do `v9`: `abrir` respondia em 0,3 s e `delta` travava em TODO repo, ate um arquivo
      // de 511 bytes.
      //
      // O programa e cacheado, e um `[]` sem programa e uma RESPOSTA — nunca uma morte. O cache
      // e invalidado no `abrir`, que e quando a raiz e o tsconfig podem mudar.
      if (programaCache) return programaCache;
      if (!tsconfig || !fs.existsSync(tsconfig)) {
        programaCache = [];
        return programaCache;
      }
      const bruto = ts.readConfigFile(tsconfig, ts.sys.readFile);
      if (bruto.error) {
        programaCache = [];
        return programaCache;
      }
      programaCache = ts.parseJsonConfigFileContent(bruto.config, ts.sys, raiz).fileNames.slice();
      return programaCache;
    },
    getScriptVersion: (f) => String(versaoDe(f)),
    getScriptSnapshot: (f) => {
      if (injetado.has(f)) return ts.ScriptSnapshot.fromString(injetado.get(f));
      if (!fs.existsSync(f)) return undefined;
      return ts.ScriptSnapshot.fromString(fs.readFileSync(f, "utf8"));
    },
    // MEDIDO em 10/10: o TypeScript 5.9 chama `acquireDocumentWithKey` no host durante o
    // `getSemanticDiagnostics`. Sem ele o TS recebe `undefined` e estoura
    // `TypeError: Cannot read properties of undefined (reading 'target')` em `computeValue`
    // (typescript.js:21908) — excecao LANCADA DENTRO do TypeService, que o `try/catch` do handler
    // nao alcanca. A assinatura e a do TS: devolve `{ sourceFile, version }`.
    acquireDocumentWithKey: (fileName, caminho, compilationSettings, scriptSnapshot, vers,
                             _formato) => {
      const texto = injetado.has(caminho)
        ? injetado.get(caminho)
        : (fs.existsSync(caminho) ? fs.readFileSync(caminho, "utf8") : undefined);
      if (texto === undefined) {
        // o TS espera uma entrada: sem `sourceFile`, `computeValue` le `undefined.target`.
        return { sourceFile: undefined, version: vers, languageVersionOrOptions: undefined };
      }
      return {
        sourceFile: ts.createSourceFile(
          caminho, texto, (compilationSettings && compilationSettings.languageVersion) || 99,
          true, scriptKindDe(caminho)
        ),
        version: vers,
        languageVersionOrOptions: compilationSettings,
      };
    },
    getScriptKind: (f) => {
      // MEDIDO em 09/10: sem isto, a prova de equivalencia deu 10/20 -- todas as divergencias
      // eram `.tsx`. O host padrao trata `.tsx` como `Unknown`, e o JSX nunca e analisado, o
      // que faz o LS devolver um conjunto de diagnosticos diferente do `tsc`.
      if (/\.tsx$/.test(f)) return ts.ScriptKind.TSX;
      if (/\.jsx$/.test(f)) return ts.ScriptKind.JSX;
      if (/\.js$|\.mjs$|\.cjs$/.test(f)) return ts.ScriptKind.JS;
      if (/\.json$/.test(f)) return ts.ScriptKind.JSON;
      return ts.ScriptKind.TS;
    },
    getCurrentDirectory: () => raiz,
    getCompilationSettings: () => settings,
    getDefaultLibFileName: (o) => ts.getDefaultLibFilePath(o),
    fileExists: (f) => injetado.has(f) || fs.existsSync(f),
    readFile: (f) => (injetado.has(f) ? injetado.get(f) : fs.existsSync(f) ? fs.readFileSync(f, "utf8") : undefined),
    readDirectory: ts.sys.readDirectory,
    directoryExists: ts.sys.directoryExists,
    getDirectories: ts.sys.getDirectories,
    useCaseSensitiveFileNames: () => ts.sys.useCaseSensitiveFileNames,
    getNewLine: () => ts.sys.newLine,
  };
}

let settings = {};
let programaCache = null;

// tsconfig do projeto, lido uma vez (as opcoes que importam ao oraculo).
function lerTsconfig(p) {
  const bruto = ts.readConfigFile(p, ts.sys.readFile);
  if (bruto.error) throw new Error("tsconfig invalido: " + p);
  const conv = ts.parseJsonConfigFileContent(
    bruto.config,
    ts.sys,
    path.dirname(p)
  );
  settings = Object.assign({}, conv.options, {
    noEmit: true,
    allowJs: true,
    checkJs: true,
    skipLibCheck: true,
  });
  return conv;
}

const LINHA_TSC = /^(.+?)\((\d+),(\d+)\): error (TS\d+): (.*)$/;

function diagnosticosDe(arquivoAbs) {
  const out = servico.getSemanticDiagnostics(arquivoAbs);
  return out.map((d) => ({
    cod: "TS" + d.code,
    lin: d.start != null ? d.file.getLineAndCharacterOfPosition(d.start).line + 1 : 0,
    msg: ts.flattenDiagnosticMessageText(d.messageText, " "),
  }));
}

function assinatura(ds) {
  const m = new Map();
  for (const d of ds) {
    const k = JSON.stringify([d.cod, d.lin, d.msg]);
    m.set(k, (m.get(k) || 0) + 1);
  }
  return m;
}

function subtrai(a, b) {
  const out = new Map();
  for (const [k, n] of a) {
    const d = n - (b.get(k) || 0);
    if (d > 0) out.set(k, d);
  }
  return out;
}

function abrir(msg) {
  raiz = msg.raiz;
  injetado.clear();
  programaCache = null;
  tsconfig = msg.tsconfig || null;
  if (msg.ts_node && msg.ts_node !== TS_NODE) {
    // recarrega o `typescript` do repo rotulado (o cache do `require` e por caminho absoluto)
    TS_NODE = msg.ts_node;
    ts = require(TS_NODE);
  }
  if (tsconfig && fs.existsSync(tsconfig)) lerTsconfig(tsconfig);
  servico = ts.createLanguageService(criarHost(), ts.createDocumentRegistry());
  return { ok: true, raiz, versao_ts: ts.version, ts_node: TS_NODE };
}

function delta(msg) {
  const a = abs(msg.arquivo);
  // estado ANTES: o arquivo como esta no disco hoje
  const antes = fs.existsSync(a) ? diagnosticosDe(a) : [];
  // estado DEPOIS: o mesmo arquivo, com o conteudo da edicao
  injetado.set(a, msg.depois);
  versao += 1;
  const depois = diagnosticosDe(a);
  injetado.delete(a);
  versao += 1;

  // `JSON.parse(k)` devolve um ARRAY [cod, lin, msg] (a assinatura e serializada assim), e nao um
  // objeto -- o Python lia `n["cod"]` num array e quebrava. Aqui o vira objeto nomeado.
  const desempacota = (m) =>
    Array.from(m.entries()).map(([k, n]) => {
      const [cod, lin, msg] = JSON.parse(k);
      return { cod, lin, msg, n };
    });

  const novos = subtrai(assinatura(depois), assinatura(antes));
  const perdidos = subtrai(assinatura(antes), assinatura(depois));
  return {
    ok: true,
    arquivo: msg.arquivo,
    antes: antes.length,
    depois: depois.length,
    // os diagnosticos JA EXISTENTES na base deste arquivo: o rotulador usa esta lista para
    // descartar o par quando ela tem dependencia externa ausente (TS2307/TS2305/TS7016), pelo
    // motivo de um par que quebra simbolo de pacote ausente sair com erro igual antes e depois.
    base: antes,
    novos: desempacota(novos),
    perdidos: desempacota(perdidos),
  };
}

// MEDIDO em 10/10: a resposta pode passar de 65 KB (um `delta` com 145 diagnosticos serializados
// deu 65.536 caracteres). Com `text=True, bufsize=1` do lado do Python, o `readline()` lia so o
// que coube no buffer e devolvia uma linha SEM FIM -- e o `json.loads` estourava
// "Unterminated string starting at column 65455". A saida agora vai precedida de `#<n bytes>\n`
// e o leitor consome EXATAMENTE n bytes, sem depender de linha nenhuma. A leitura por linha
// fica so para o que vier sem o marcador (compatibilidade).
function responde(obj) {
  const corpo = JSON.stringify(obj);
  process.stdout.write(`#${Buffer.byteLength(corpo, "utf8")}\n${corpo}\n`);
}

let buffer = "";
process.stdin.on("data", (d) => {
  buffer += d.toString();
  let i;
  while ((i = buffer.indexOf("\n")) >= 0) {
    const linha = buffer.slice(0, i);
    buffer = buffer.slice(i + 1);
    if (!linha.trim()) continue;
    let msg;
    try {
      msg = JSON.parse(linha);
    } catch (e) {
      responde({ ok: false, erro: "json: " + e.message });
      continue;
    }
    try {
      let r;
      if (msg.op === "abrir") r = abrir(msg);
      else if (msg.op === "delta") r = delta(msg);
      else if (msg.op === "fechar") {
        r = { ok: true };
        responde(r);
        process.exit(0);
      } else r = { ok: false, erro: "op desconhecida: " + msg.op };
      responde(r);
    } catch (e) {
      responde({ ok: false, erro: String(e && e.message ? e.message : e) });
    }
  }
});