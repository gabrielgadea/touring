//! The edit distance behind every "você quis dizer" — ONE implementation for all adapters.
//!
//! Until 02/11/2026 five adapters carried their own copy (`python.rs`, `rust.rs`,
//! `classe_css.rs`, `html/texto.rs`, `typescript/util.rs`): the same algorithm written five
//! times, four of them line for line. A suggestion that depends on which file computed the
//! distance is a bug nobody can see — a fix to one copy would have left four teaching a
//! different "closest name".

/// Levenshtein distance between `a` and `b`, by `char` (a code point is one edit, not its bytes):
/// the fewest single-character insertions, deletions and substitutions that turn one into the
/// other. Symmetric, `0` only for equal strings.
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut anterior: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut atual = Vec::with_capacity(b.len() + 1);
        atual.push(i + 1);
        for (j, cb) in b.iter().enumerate() {
            let troca = anterior[j] + usize::from(ca != cb);
            atual.push(troca.min(atual[j] + 1).min(anterior[j + 1] + 1));
        }
        anterior = atual;
    }
    anterior[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reference: the textbook full matrix, no row reuse.
    fn matriz(a: &str, b: &str) -> usize {
        let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
        let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
        for (i, linha) in d.iter_mut().enumerate() {
            linha[0] = i;
        }
        for (j, celula) in d[0].iter_mut().enumerate() {
            *celula = j;
        }
        for i in 1..=a.len() {
            for j in 1..=b.len() {
                let custo = usize::from(a[i - 1] != b[j - 1]);
                d[i][j] = (d[i - 1][j - 1] + custo)
                    .min(d[i - 1][j] + 1)
                    .min(d[i][j - 1] + 1);
            }
        }
        d[a.len()][b.len()]
    }

    #[test]
    fn a_tabela_de_distancias() {
        for (a, b, d) in [
            ("", "", 0),
            ("a", "", 1),
            ("", "ab", 2),
            ("abc", "", 3),
            ("meta", "metaq", 1),
            ("meta", "mta", 1),
            ("meta", "mota", 1),
            ("kitten", "sitting", 3),
            ("flaw", "lawn", 2),
            ("igual", "igual", 0),
            ("abc", "cba", 2),
            // by character, not by byte
            ("áé", "ae", 2),
            ("é", "e", 1),
            ("αβγ", "αβ", 1),
        ] {
            assert_eq!(levenshtein(a, b), d, "{a:?} {b:?}");
            assert_eq!(levenshtein(b, a), d, "{b:?} {a:?} (symmetry)");
        }
    }

    /// Every string up to length 4 over a 3-letter alphabet (with a 2-byte letter): 14 641 pairs,
    /// each against the full matrix.
    #[test]
    fn concorda_com_a_matriz_completa_em_todo_o_espaco_pequeno() {
        let letras = ['a', 'b', 'é'];
        let mut todas: Vec<String> = vec![String::new()];
        let mut fronteira: Vec<String> = vec![String::new()];
        for _ in 0..4 {
            let mut proxima = Vec::new();
            for s in &fronteira {
                for l in letras {
                    proxima.push(format!("{s}{l}"));
                }
            }
            todas.extend(proxima.iter().cloned());
            fronteira = proxima;
        }
        assert_eq!(todas.len(), 1 + 3 + 9 + 27 + 81);
        for a in &todas {
            for b in &todas {
                assert_eq!(levenshtein(a, b), matriz(a, b), "{a:?} {b:?}");
            }
        }
    }

    /// Identity and the triangle inequality, on a small sample.
    #[test]
    fn e_uma_metrica() {
        let amostra = ["", "a", "ab", "abc", "bca", "é", "aé", "cab"];
        for a in amostra {
            assert_eq!(levenshtein(a, a), 0);
            for b in amostra {
                for c in amostra {
                    assert!(levenshtein(a, c) <= levenshtein(a, b) + levenshtein(b, c));
                }
            }
        }
    }
}
