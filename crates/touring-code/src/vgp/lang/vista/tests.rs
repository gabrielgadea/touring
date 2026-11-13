//! Tests of the project view (`vista.rs`): the precedence, the key and the directory rule are
//! asserted ONCE here; the adapters are tested through their verdicts
//! (`tests/vgp_vista_overlays.rs`).

use super::*;
use std::fs;
use std::path::PathBuf;

/// `tmp/proj/` with `a.rs` ("disco"), `d/` (a directory) and `grande.txt` (64 bytes); `tmp/fora.rs`
/// is outside the root.
struct Arvore {
    tmp: PathBuf,
    raiz: PathBuf,
}

impl Arvore {
    fn nova(tag: &str) -> Self {
        let tmp = std::env::temp_dir().join(format!("vgp-vista-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let raiz = tmp.join("proj");
        fs::create_dir_all(raiz.join("d")).expect("mkdir");
        fs::write(raiz.join("a.rs"), "disco").expect("write");
        fs::write(raiz.join("grande.txt"), "x".repeat(64)).expect("write");
        fs::write(tmp.join("fora.rs"), "fora").expect("write");
        Self { tmp, raiz }
    }

    fn p(&self, rel: &str) -> PathBuf {
        self.raiz.join(rel)
    }
}

impl Drop for Arvore {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.tmp);
    }
}

fn mapa(itens: &[(&Path, &str)]) -> HashMap<String, String> {
    itens
        .iter()
        .map(|(k, v)| (k.to_string_lossy().into_owned(), (*v).to_string()))
        .collect()
}

#[test]
fn o_overlay_vence_o_disco_e_o_disco_vale_onde_nao_ha_overlay() {
    let t = Arvore::nova("precedencia");
    let sobre = mapa(&[(&t.p("a.rs"), "overlay"), (&t.p("novo.rs"), "so overlay")]);
    let vista = Vista::nova(&sobre);
    let raizes = [t.raiz.as_path()];
    // the overlay over the disk
    assert_eq!(vista.ler(&raizes, &t.p("a.rs"), None).unwrap(), "overlay");
    // only the overlay: the disk has no such file
    assert_eq!(
        vista.ler(&raizes, &t.p("novo.rs"), None).unwrap(),
        "so overlay"
    );
    // only the disk: no overlay covers it
    let vazio = HashMap::new();
    let so_disco = Vista::nova(&vazio);
    assert_eq!(so_disco.ler(&raizes, &t.p("a.rs"), None).unwrap(), "disco");
    // absent everywhere: the filesystem's answer, never an overlay's
    assert!(matches!(
        so_disco.ler(&raizes, &t.p("nada.rs"), None),
        Err(Recusa::Io(_))
    ));
}

#[test]
fn o_disco_so_vale_dentro_das_raizes_e_o_overlay_nao_depende_delas() {
    let t = Arvore::nova("raizes");
    let vazio = HashMap::new();
    let raizes = [t.raiz.as_path()];
    // outside the roots the disk is refused (the containment is `caminho`'s)
    assert!(
        Vista::nova(&vazio)
            .ler(&raizes, &t.tmp.join("fora.rs"), None)
            .unwrap_err()
            .fora_das_raizes()
    );
    // …but an overlay the caller declared is read wherever it is
    let sobre = mapa(&[(&t.tmp.join("fora.rs"), "declarado")]);
    assert_eq!(
        Vista::nova(&sobre)
            .ler(&raizes, &t.tmp.join("fora.rs"), None)
            .unwrap(),
        "declarado"
    );
}

#[test]
fn o_teto_de_leitura_e_do_disco_e_nunca_do_overlay() {
    let t = Arvore::nova("teto");
    let raizes = [t.raiz.as_path()];
    let vazio = HashMap::new();
    assert!(matches!(
        Vista::nova(&vazio).ler(&raizes, &t.p("grande.txt"), Some(16)),
        Err(Recusa::Grande {
            tamanho: 64,
            limite: 16
        })
    ));
    assert_eq!(
        Vista::nova(&vazio)
            .ler(&raizes, &t.p("grande.txt"), Some(64))
            .unwrap()
            .len(),
        64
    );
    // an overlay over the cap is read whole: the cap guards the disk
    let sobre = mapa(&[(&t.p("grande.txt"), "x".repeat(200).as_str())]);
    assert_eq!(
        Vista::nova(&sobre)
            .ler(&raizes, &t.p("grande.txt"), Some(16))
            .unwrap()
            .len(),
        200
    );
}

#[test]
fn a_chave_e_o_caminho_normal_e_so_a_chave_e_normalizada() {
    let t = Arvore::nova("chave");
    let sobre = mapa(&[(&t.p("a.rs"), "overlay")]);
    let vista = Vista::nova(&sobre);
    let esquisito = [
        t.raiz.join(".").join("a.rs"),
        t.raiz.join("d").join("..").join("a.rs"),
        PathBuf::from(format!("{}//a.rs", t.raiz.display())),
    ];
    for p in &esquisito {
        assert_eq!(vista.sobreposto(p), Some("overlay"), "{}", p.display());
    }
    // a different file is a different key
    assert_eq!(vista.sobreposto(&t.p("b.rs")), None);
    // a RELATIVE path is not the absolute one: nothing resolves it against the process's directory
    assert_eq!(vista.sobreposto(Path::new("a.rs")), None);
}

#[test]
fn sondar_diz_arquivo_para_o_overlay_e_o_que_o_disco_diz_para_o_resto() {
    let t = Arvore::nova("sondar");
    let sobre = mapa(&[
        (&t.p("d"), "overlay no lugar de um diretorio"),
        (&t.p("so.rs"), "x"),
    ]);
    let vista = Vista::nova(&sobre);
    let raizes = [t.raiz.as_path()];
    // an overlay is a file, even where the disk has a directory
    assert_eq!(vista.sondar(&raizes, &t.p("d")), Sonda::Arquivo);
    assert_eq!(vista.sondar(&raizes, &t.p("so.rs")), Sonda::Arquivo);
    // what no overlay covers is the disk's answer
    assert_eq!(vista.sondar(&raizes, &t.p("a.rs")), Sonda::Arquivo);
    assert_eq!(vista.sondar(&raizes, &t.p("nada")), Sonda::Ausente);
    assert_eq!(vista.sondar(&raizes, &t.tmp.join("fora.rs")), Sonda::Fora);
    assert!(vista.e_arquivo(&raizes, &t.p("so.rs")));
    assert!(!vista.e_arquivo(&raizes, &t.p("nada")));
}

#[test]
fn o_diretorio_que_so_o_overlay_cria_existe() {
    let t = Arvore::nova("diretorio");
    let sobre = mapa(&[(&t.p("novo/fundo/x.ts"), "x")]);
    let vista = Vista::nova(&sobre);
    let raizes = [t.raiz.as_path()];
    // the disk has neither `novo` nor `novo/fundo`
    for dir in ["novo", "novo/fundo"] {
        assert!(vista.e_diretorio(&raizes, &t.p(dir)), "{dir}");
        assert!(vista.existe(&raizes, &t.p(dir)), "{dir}");
    }
    // the file itself is not a directory; a sibling name is not covered by the prefix
    assert!(!vista.e_diretorio(&raizes, &t.p("novo/fundo/x.ts")));
    assert!(!vista.e_diretorio(&raizes, &t.p("nov")));
    // a trailing slash and a `..` spell the same directory
    assert!(vista.e_diretorio(
        &raizes,
        &PathBuf::from(format!("{}/novo/", t.raiz.display()))
    ));
    assert!(vista.e_diretorio(&raizes, &t.p("d").join("..").join("novo")));
    // the directory of the DISK is still a directory
    assert!(vista.e_diretorio(&raizes, &t.p("d")));
    // `.` is nothing an absolute key lies under
    assert!(!vista.e_diretorio(&raizes, Path::new(".")));
}

#[test]
fn existe_cobre_o_arquivo_do_overlay_o_diretorio_implicito_e_qualquer_coisa_do_disco() {
    let t = Arvore::nova("existe");
    let sobre = mapa(&[(&t.p("so.rs"), "x")]);
    let vista = Vista::nova(&sobre);
    let raizes = [t.raiz.as_path()];
    assert!(vista.existe(&raizes, &t.p("so.rs")), "file of the overlays");
    assert!(vista.existe(&raizes, &t.p("a.rs")), "file of the disk");
    assert!(vista.existe(&raizes, &t.p("d")), "directory of the disk");
    assert!(!vista.existe(&raizes, &t.p("nada")));
    assert!(
        !vista.existe(&raizes, &t.tmp.join("fora.rs")),
        "outside the roots is not looked at"
    );
}

#[test]
fn o_mapa_vazio_nao_responde_nada_e_nao_normaliza() {
    let t = Arvore::nova("vazio");
    let vazio = HashMap::new();
    let vista = Vista::nova(&vazio);
    assert_eq!(vista.sobreposto(&t.p("a.rs")), None);
    assert!(!vista.cobre_diretorio(&t.p("d")));
    assert_eq!(vista.outros(&t.p("a.rs")).count(), 0);
    // the disk still answers
    assert!(vista.e_arquivo(&[t.raiz.as_path()], &t.p("a.rs")));
}

#[test]
fn outros_traz_os_irmaos_sem_o_proprio_arquivo() {
    let t = Arvore::nova("outros");
    let sobre = mapa(&[
        (&t.p("a.rs"), "eu"),
        (&t.p("b.rs"), "irmao b"),
        (&t.p("c.rs"), "irmao c"),
    ]);
    let vista = Vista::nova(&sobre);
    let mut textos: Vec<&str> = vista.outros(&t.p("a.rs")).map(|(_, v)| v).collect();
    textos.sort_unstable();
    assert_eq!(textos, vec!["irmao b", "irmao c"]);
    // the entry is recognised however its path is spelled
    let esquisito = t.raiz.join("d").join("..").join("a.rs");
    assert_eq!(vista.outros(&esquisito).count(), 2);
    // a file with no overlay: every overlay is a sibling
    assert_eq!(vista.outros(&t.p("z.rs")).count(), 3);
}
