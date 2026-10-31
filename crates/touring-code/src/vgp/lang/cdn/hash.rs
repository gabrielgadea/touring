// touring-quality:allow-secrets — the tests below embed PUBLISHED reference digests (Python
// `hashlib` outputs of public strings), high-entropy by nature and not credentials.
//! Digests for the CDN cache: the SRI tokens of the bytes the network resolver downloaded.
//!
//! The primitives are the workspace's `sha2` and `base64` crates — the layer-1 `cdn` check
//! compares the `integrity` of a tag against these tokens, so nothing here is hand-rolled; the
//! tests pin the wiring (algorithm, order, padding) against `hashlib` outputs.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use sha2::{Digest, Sha256, Sha384, Sha512};

/// The three SRI digests of `bytes`, as `algorithm-base64` tokens in the `integrity` syntax
/// (weakest first): `sha256-…`, `sha384-…`, `sha512-…`.
pub fn sri(bytes: &[u8]) -> Vec<String> {
    vec![
        format!("sha256-{}", STANDARD.encode(Sha256::digest(bytes))),
        format!("sha384-{}", STANDARD.encode(Sha384::digest(bytes))),
        format!("sha512-{}", STANDARD.encode(Sha512::digest(bytes))),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reference digests computed with Python `hashlib` + `base64.b64encode`.
    #[test]
    fn sri_bate_com_o_hashlib_na_ordem_e_com_preenchimento() {
        let casos: &[(&[u8], [&str; 3])] = &[
            (
                b"",
                [
                    "sha256-47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=",
                    "sha384-OLBgp1GsljhM2TJ+sbHjaiH9txEUvgdDTAzHv2P24donTt6/529l+9Ua0vFImLlb",
                    "sha512-z4PhNX7vuL3xVChQ1m2AB9Yg5AULVxXcg/SpIdNs6c5H0NE8XYXysP+DGNKHfuwvY7kxvUdBeoGlODJ6+SfaPg==",
                ],
            ),
            (
                b"abc",
                [
                    "sha256-ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0=",
                    "sha384-ywB1P0WjXou1oD1pmsZQBycsMqsO3tFjGotgWkP/W+2AhgcroefMI1i67KE0yCWn",
                    "sha512-3a81oZNherrMQXNJriBBMRLm+k6JqX6iCp7u5ktV05ohkpkqJ0/BqDa6PCOj/uu9RU1EI2Q86A4qmslPpUyknw==",
                ],
            ),
            (
                b"console.log('vgp');\n",
                [
                    "sha256-jJeNPzyBu7gnnlj9LK0KVdAOTaOJExcIQEy3ACbmvQo=",
                    "sha384-OcXUqEjFYy6VsN4S7pR544agBokSr1UOfFPWsq3YHKuFRIZmN07hNDLa0v+gTmsg",
                    "sha512-SL+vzu/Xz7QEpNABHjCoZwllqNaGYfOWbiwBtY8cjsqLeLBVwlWZR38Ul784ZGQn8/VrBGqgK4J5w0Ub8qkvuw==",
                ],
            ),
        ];
        for (entrada, esperado) in casos {
            assert_eq!(sri(entrada), esperado.to_vec(), "{entrada:?}");
        }
    }

    #[test]
    fn corpo_longo_cruza_varios_blocos() {
        let mil = sri(&[b'a'; 1000]);
        assert_eq!(
            mil[0],
            "sha256-Qe3s5C1j6Nm/UVqbppMuHCDLyfWl0TRkWttdsblzfqM="
        );
        assert_eq!(
            mil[1],
            "sha384-9USAaJxrCxHQMDKF2agbIak7ymulobRHJ2XcpNpF7jKAgtRpxlDNO2GxbTJmq4zt"
        );
        assert_eq!(
            mil[2],
            "sha512-Z7pVNaRuP4bb++2Mu68BJcdu1Un/iwueA+DIjPkPpjT6exK0fXe2lN5Iis6NmmWWfclt9ZlyfTKSqNnUR3Cclw=="
        );
    }
}
