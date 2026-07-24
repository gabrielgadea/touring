//! PT-BR (Portuguese-Brazilian) Pattern Markers for Erickson NLP
//!
//! Extends the default English/Chinese markers with Portuguese-Brazilian
//! discourse markers following the Toulmin argument model.
//!
//! # PT-BR Markers
//!
//! - **Claim**: deve, precisa, recomendamo, argumentamos, conclúimos, afirmamos, propomos, sugerimos, devemos, e necessario, importante
//! - **Evidence**: porque, ja que, visto que, segundo, conforme, estudos, pesquisas, dados, numeros, relatorios, evidencias, comprovadamente
//! - **Warrant**: portanto, assim, consequentemente, o que significa, isso demonstra, implica, indica, segue que, dado que, por conseguinte
//! - **Rebuttal**: entretanto, contudo, porem, apesar de, nao obstante, objecao, contra-argumento, embora, enquanto
//! - **Backing**: ademais, outrossim, alem disso, alem do mais, vale notar, inclusive, igualmente, tambem, junto com

use regex::Regex;
use std::sync::LazyLock;

/// PT-BR claim markers combined with EN originals.
/// Matches Portuguese-Brazilian assertion patterns.
pub static PTBR_CLAIM_MARKERS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(should|must|need to|recommend|argue|conclude|assert|claim|propose|suggest|deve|precisa|recomendamos|argumentamos|concluimos|afirmamos|propomos|sugerimos|devemos|e necessario|importante)\b",
    )
    .expect("invalid PTBR claim regex")
});

/// PT-BR evidence markers combined with EN+ZH originals.
/// Matches Portuguese-Brazilian evidence/support patterns.
pub static PTBR_EVIDENCE_MARKERS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(because|since|evidence|according to|studies|research|data|statistics|reports|evidencia|pesquisa|dados|numeros|relatorios|porque|ja que|visto que|segundo|conforme|estudos|pesquisas|evidencias|comprovadamente|segundo pesquisas)\b",
    )
    .expect("invalid PTBR evidence regex")
});

/// PT-BR warrant markers combined with EN+ZH originals.
/// Matches Portuguese-brazilian reasoning/conclusion patterns.
pub static PTBR_WARRANT_MARKERS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(therefore|thus|consequently|which means|this shows|implies|indicates|follows that|given that|assuming|portanto|assim|consequentemente|o que significa|isso demonstra|implica|indica|segue que|visto que|dado que|por conseguinte)\b",
    )
    .expect("invalid PTBR warrant regex")
});

/// PT-BR rebuttal markers combined with EN+ZH originals.
/// Matches Portuguese-brazilian counter-argument patterns.
pub static PTBR_REBUTTAL_MARKERS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(however|but|although|despite|nevertheless|objection|entretanto|contudo|porem|apesar de|nao obstante|objecao|contra-argumento|embora|enquanto)\b",
    )
    .expect("invalid PTBR rebuttal regex")
});

/// PT-BR backing markers combined with EN+ZH originals.
/// Matches Portuguese-brazilian additional support patterns.
pub static PTBR_BACKING_MARKERS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(furthermore|additionally|moreover|in addition|also|besides|another|ademais|outrossim|alem disso|alem do mais|vale notar|inclusive|igualmente|tambem|junto com)\b",
    )
    .expect("invalid PTBR backing regex")
});

/// Extracts PT-BR claim elements from text.
pub fn extract_ptbr_claims(text: &str) -> Vec<(String, usize, usize)> {
    PTBR_CLAIM_MARKERS
        .find_iter(text)
        .map(|m| (m.as_str().to_string(), m.start(), m.end()))
        .collect()
}

/// Extracts PT-BR evidence elements from text.
pub fn extract_ptbr_evidence(text: &str) -> Vec<(String, usize, usize)> {
    PTBR_EVIDENCE_MARKERS
        .find_iter(text)
        .map(|m| (m.as_str().to_string(), m.start(), m.end()))
        .collect()
}

/// Extracts PT-BR warrant elements from text.
pub fn extract_ptbr_warrants(text: &str) -> Vec<(String, usize, usize)> {
    PTBR_WARRANT_MARKERS
        .find_iter(text)
        .map(|m| (m.as_str().to_string(), m.start(), m.end()))
        .collect()
}

/// Extracts PT-BR rebuttal elements from text.
pub fn extract_ptbr_rebuttals(text: &str) -> Vec<(String, usize, usize)> {
    PTBR_REBUTTAL_MARKERS
        .find_iter(text)
        .map(|m| (m.as_str().to_string(), m.start(), m.end()))
        .collect()
}

/// Extracts PT-BR backing elements from text.
pub fn extract_ptbr_backings(text: &str) -> Vec<(String, usize, usize)> {
    PTBR_BACKING_MARKERS
        .find_iter(text)
        .map(|m| (m.as_str().to_string(), m.start(), m.end()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_claim_ptbr() {
        let text = "Devemos atualizar o serde porque tem uma vulnerabilidade critica.";
        let claims = extract_ptbr_claims(text);
        assert!(
            !claims.is_empty(),
            "expected to find PT-BR claim in: {}",
            text
        );
        assert!(
            claims.iter().any(|(s, _, _)| s.to_lowercase() == "devemos"),
            "expected 'devemos' in claims"
        );
    }

    #[test]
    fn test_extract_claim_ptbr_argumentamos() {
        let text = "Argumentamos que a correcao e necessaria para a seguranca.";
        let claims = extract_ptbr_claims(text);
        assert!(
            !claims.is_empty(),
            "expected to find 'argumentamos' claim in: {}",
            text
        );
    }

    #[test]
    fn test_extract_claim_ptbr_recomendamos() {
        let text = "Recomendamos a atualizacao imediata do pacote.";
        let claims = extract_ptbr_claims(text);
        assert!(
            !claims.is_empty(),
            "expected to find 'recomendamos' claim in: {}",
            text
        );
    }

    #[test]
    fn test_extract_claim_ptbr_concluimos() {
        let text = "Concluimos que o problema deve ser corrigido urgentemente.";
        let claims = extract_ptbr_claims(text);
        assert!(
            !claims.is_empty(),
            "expected to find 'concluimos' claim in: {}",
            text
        );
    }

    #[test]
    fn test_extract_evidence_ptbr() {
        let text = "Porque o sistema esta vulneravel, recomendamos atualizacao.";
        let evidence = extract_ptbr_evidence(text);
        assert!(
            !evidence.is_empty(),
            "expected to find PT-BR evidence in: {}",
            text
        );
        assert!(
            evidence
                .iter()
                .any(|(s, _, _)| s.to_lowercase() == "porque"),
            "expected 'porque' in evidence"
        );
    }

    #[test]
    fn test_extract_evidence_ptbr_visto_que() {
        let text = "Visto que ha relatorios de falhas, devemos agir.";
        let evidence = extract_ptbr_evidence(text);
        assert!(
            !evidence.is_empty(),
            "expected to find 'visto que' evidence in: {}",
            text
        );
    }

    #[test]
    fn test_extract_evidence_ptbr_estudos() {
        let text = "Segundo estudos recentes, a vulnerabilidade e critica.";
        let evidence = extract_ptbr_evidence(text);
        assert!(
            !evidence.is_empty(),
            "expected to find 'estudos' evidence in: {}",
            text
        );
    }

    #[test]
    fn test_extract_warrant_ptbr() {
        let text = "O sistema esta vulneravel, portanto precisamos atualizar.";
        let warrants = extract_ptbr_warrants(text);
        assert!(
            !warrants.is_empty(),
            "expected to find PT-BR warrant in: {}",
            text
        );
        assert!(
            warrants
                .iter()
                .any(|(s, _, _)| s.to_lowercase() == "portanto"),
            "expected 'portanto' in warrants"
        );
    }

    #[test]
    fn test_extract_warrant_ptbr_consequentemente() {
        let text = "A vulnerabilidade e critica, consequentemente devemos atualizar.";
        let warrants = extract_ptbr_warrants(text);
        assert!(
            !warrants.is_empty(),
            "expected to find 'consequentemente' warrant in: {}",
            text
        );
    }

    #[test]
    fn test_extract_warrant_ptbr_dado_que() {
        let text = "Dado que ha evidencias de ataque, precisamos agir.";
        let warrants = extract_ptbr_warrants(text);
        assert!(
            !warrants.is_empty(),
            "expected to find 'dado que' warrant in: {}",
            text
        );
    }

    #[test]
    fn test_extract_rebuttal_ptbr() {
        let text = "Entretanto, isso pode quebrar compatibilidade retroativa.";
        let rebuttals = extract_ptbr_rebuttals(text);
        assert!(
            !rebuttals.is_empty(),
            "expected to find PT-BR rebuttal in: {}",
            text
        );
        assert!(
            rebuttals.iter().any(
                |(s, _, _)| s.to_lowercase() == "entretanto" || s.to_lowercase() == "entretant"
            ),
            "expected 'entretanto' in rebuttals"
        );
    }

    #[test]
    fn test_extract_rebuttal_ptbr_porem() {
        let text = "Porem, ha alternativas para manter a compatibilidade.";
        let rebuttals = extract_ptbr_rebuttals(text);
        assert!(
            !rebuttals.is_empty(),
            "expected to find 'porem' rebuttal in: {}",
            text
        );
    }

    #[test]
    fn test_extract_rebuttal_ptbr_apesar_de() {
        let text = "Apesar de o risco ser baixo, devemos atualizar.";
        let rebuttals = extract_ptbr_rebuttals(text);
        assert!(
            !rebuttals.is_empty(),
            "expected to find 'apesar de' rebuttal in: {}",
            text
        );
    }

    #[test]
    fn test_extract_backing_ptbr() {
        let text = "Ademais, a nova versao tem melhorias de performance.";
        let backings = extract_ptbr_backings(text);
        assert!(
            !backings.is_empty(),
            "expected to find PT-BR backing in: {}",
            text
        );
        assert!(
            backings
                .iter()
                .any(|(s, _, _)| s.to_lowercase() == "ademais"),
            "expected 'ademais' in backings"
        );
    }

    #[test]
    fn test_extract_backing_ptbr_alem_disso() {
        let text = "Alem disso, ha correcoes de seguranca adicionais.";
        let backings = extract_ptbr_backings(text);
        assert!(
            !backings.is_empty(),
            "expected to find 'alem disso' backing in: {}",
            text
        );
    }

    #[test]
    fn test_extract_backing_ptbr_outrossim() {
        let text = "Outrossim, os testes demonstram maior estabilidade.";
        let backings = extract_ptbr_backings(text);
        assert!(
            !backings.is_empty(),
            "expected to find 'outrossim' backing in: {}",
            text
        );
    }

    #[test]
    fn test_ptbr_markers_case_insensitive() {
        let text = "DEVEMOS atualizar o sistema.";
        let claims = extract_ptbr_claims(text);
        assert!(
            !claims.is_empty(),
            "PT-BR markers should be case-insensitive"
        );
    }

    #[test]
    fn test_ptbr_markers_no_overlap_en() {
        // Both EN and PT-BR markers should work on same text
        let text = "We should upgrade because of CVE.";
        let claims = extract_ptbr_claims(text);
        let evidence = extract_ptbr_evidence(text);
        // EN markers should match
        assert!(
            !claims.is_empty() || !evidence.is_empty(),
            "EN markers should work"
        );
    }

    #[test]
    fn test_extract_claim_ptbr_sugerimos() {
        let text = "Sugerimos que a equipe revise o codigo imediatamente.";
        let claims = extract_ptbr_claims(text);
        assert!(
            !claims.is_empty(),
            "expected to find 'sugerimos' claim in: {}",
            text
        );
        assert!(
            claims
                .iter()
                .any(|(s, _, _)| s.to_lowercase() == "sugerimos"),
            "expected 'sugerimos' in claims"
        );
    }

    #[test]
    fn test_extract_claim_ptbr_importante() {
        let text = "E importante atualizar o sistema para evitar vulnerabilidades.";
        let claims = extract_ptbr_claims(text);
        assert!(
            !claims.is_empty(),
            "expected to find 'importante' claim in: {}",
            text
        );
    }

    #[test]
    fn test_extract_evidence_ptbr_comprovadamente() {
        let text = "Comprovadamente, o modulo tem falhas de seguranca.";
        let evidence = extract_ptbr_evidence(text);
        assert!(
            !evidence.is_empty(),
            "expected to find 'comprovadamente' evidence in: {}",
            text
        );
    }

    #[test]
    fn test_extract_warrant_ptbr_segue_que() {
        let text = "Os dados mostram risco alto, segue que devemos atualizar.";
        let warrants = extract_ptbr_warrants(text);
        assert!(
            !warrants.is_empty(),
            "expected to find 'segue que' warrant in: {}",
            text
        );
    }

    #[test]
    fn test_extract_backing_ptbr_inclusive() {
        let text = "Inclusive, ja houve tentativa de exploatacao.";
        let backings = extract_ptbr_backings(text);
        assert!(
            !backings.is_empty(),
            "expected to find 'inclusive' backing in: {}",
            text
        );
    }

    #[test]
    fn test_extract_backing_ptbr_junto_com() {
        let text = "Junto com a atualizacao, devem ser aplicados os patches.";
        let backings = extract_ptbr_backings(text);
        assert!(
            !backings.is_empty(),
            "expected to find 'junto com' backing in: {}",
            text
        );
    }
}
