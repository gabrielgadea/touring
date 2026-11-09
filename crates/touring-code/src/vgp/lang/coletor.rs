//! How a language adapter turns "this name does not exist" into a [`Finding`] — ONE copy.
//!
//! Only the edit distance had been unified ([`super::distancia`]). Each adapter still kept
//! its own deduplication, its own "Você quis dizer", its own cut of `reais`, and the copies
//! had drifted: the Rust adapter picked the suggestion with an order-dependent `min_by_key`
//! and cut `reais` from a `HashSet` BEFORE sorting (a different set on every run — the same
//! defect the Python adapter fixed on 10/10/2026). The rules here are total: the answer never
//! depends on the order the candidates arrive in.
//!
//! The HTML and CSS adapters list up to five neighbours with their own thresholds (`len/4`);
//! unifying those thresholds changes which names are suggested and is a product decision
//! recorded in `docs/plans/2026-11-09-work-outer/bfce/DECISOES.md`, not made here.

use super::super::{Alvo, Finding};
use super::distancia::levenshtein;

/// The real name closest to `nome` within 1..=2 edits, if any. Ties go to the smaller
/// name: the suggestion does not depend on the order of `reais`.
pub fn mais_proximo<'a>(nome: &str, reais: impl IntoIterator<Item = &'a String>) -> Option<String> {
    reais
        .into_iter()
        .map(|real| (levenshtein(nome, real), real))
        .filter(|(distancia, _)| (1..=2).contains(distancia))
        .min_by(|(da, ra), (db, rb)| da.cmp(db).then_with(|| ra.cmp(rb)))
        .map(|(_, real)| real.clone())
}

/// `motivo`, followed by ". Você quis dizer `s`?" when a real name is close.
pub fn com_sugestao(motivo: &str, nome: &str, reais: &[String]) -> String {
    match mais_proximo(nome, reais) {
        Some(sugestao) => format!("{motivo}. Você quis dizer `{sugestao}`?"),
        None => motivo.to_string(),
    }
}

/// `nomes` for [`Finding::reais`]: a TOTAL order, duplicates out, THEN the cut to `max`.
/// Cutting first (`set.iter().take(n)`) keeps a different subset on every run.
pub fn reais_ordenados<'a>(nomes: impl IntoIterator<Item = &'a String>, max: usize) -> Vec<String> {
    let mut v: Vec<String> = nomes.into_iter().cloned().collect();
    v.sort();
    v.dedup();
    v.truncate(max);
    v
}

/// Push the finding "`nome` (of `classe`) does not exist" unless one for the same
/// `(classe, nome)` is already there — the judge counts presence, not occurrences — with the
/// closest real name in its suggestion.
pub fn empurrar(
    out: &mut Vec<Finding>,
    classe: &str,
    nome: &str,
    linha: usize,
    alvo: Alvo,
    motivo: &str,
    reais: Vec<String>,
) {
    if out.iter().any(|f| f.classe == classe && f.nome == nome) {
        return;
    }
    let sugestao = com_sugestao(motivo, nome, &reais);
    out.push(Finding::candidato(
        classe, nome, linha, alvo, sugestao, reais,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nomes(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_string()).collect()
    }

    /// Every permutation of the candidates gives the same suggestion and the same `reais`.
    #[test]
    fn the_answer_does_not_depend_on_candidate_order() {
        let base = nomes(&[
            "getTime", "getTimes", "getTine", "setTime", "a", "b", "c", "d", "e", "f",
        ]);
        let mut rodada = base.clone();
        let esperado_sugestao = mais_proximo("getTim", &base);
        let esperado_reais = reais_ordenados(&base, 8);
        for passo in 0..base.len() {
            rodada.rotate_left(1);
            if passo % 2 == 0 {
                rodada.reverse();
            }
            assert_eq!(
                mais_proximo("getTim", &rodada),
                esperado_sugestao,
                "{rodada:?}"
            );
            assert_eq!(reais_ordenados(&rodada, 8), esperado_reais, "{rodada:?}");
        }
        // Distance 1 from both `getTime` and `getTine`: the smaller name wins.
        assert_eq!(esperado_sugestao.as_deref(), Some("getTime"));
        assert_eq!(
            esperado_reais,
            nomes(&["a", "b", "c", "d", "e", "f", "getTime", "getTimes"])
        );
    }

    #[test]
    fn the_suggestion_is_one_or_two_edits_away_and_never_the_name_itself() {
        let reais = nomes(&["getTime"]);
        assert_eq!(mais_proximo("getTime", &reais), None);
        assert_eq!(mais_proximo("zzzzzz", &reais), None);
        assert_eq!(
            com_sugestao("`Date` não tem `getTim`", "getTim", &reais),
            "`Date` não tem `getTim`. Você quis dizer `getTime`?"
        );
        assert_eq!(com_sugestao("m", "zzz", &reais), "m");
    }

    #[test]
    fn one_finding_per_class_and_name() {
        let mut out = Vec::new();
        empurrar(
            &mut out,
            "atributo",
            "x",
            1,
            Alvo::Externo,
            "m",
            nomes(&["xy"]),
        );
        empurrar(&mut out, "atributo", "x", 9, Alvo::Externo, "m", Vec::new());
        empurrar(&mut out, "kwarg", "x", 2, Alvo::Externo, "m", Vec::new());
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].linha, 1);
        assert_eq!(out[0].sugestao, "m. Você quis dizer `xy`?");
    }
}
