//! The four built-in capability profiles (phase **P2.3** of CEG Pln2,
//! `docs/2026-05-17-ceg-pln2-plan.md`).
//!
//! Each profile is a ready-made [`CapabilityProfile`] for a documented use
//! case. [`BuiltinProfile`] is the serializable identifier a per-project
//! profile config (CEG Pln2 P2.5) stores to select one.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{Capability, CapabilityProfile, CmdScope, Decision, HostScope, KeyScope, PathScope};

/// Environment variables every profile may read. Deliberately free of
/// credentials — secret-bearing keys (`AWS_*`, `GITHUB_TOKEN`, ...) are never
/// in the allowlist, so a sandboxed run cannot exfiltrate them.
pub const ENV_ALLOWLIST: &[&str] = &["PATH", "HOME", "USER", "LANG", "LC_ALL", "TERM", "TZ"];

/// Identifies one of the four built-in capability profiles.
///
/// This is the stable, serializable handle a per-project profile config
/// (CEG Pln2 P2.5) records; [`BuiltinProfile::build`] turns it into a concrete
/// [`CapabilityProfile`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BuiltinProfile {
    /// See [`read_only`].
    ReadOnly,
    /// See [`staged_write`].
    StagedWrite,
    /// See [`trusted`].
    Trusted,
    /// See [`sandboxed`].
    Sandboxed,
}

impl BuiltinProfile {
    /// A one-line description of the profile's intended use case.
    pub fn use_case(self) -> &'static str {
        match self {
            BuiltinProfile::ReadOnly => {
                "Static analysis, classification, dry-run of provably pure code."
            }
            BuiltinProfile::StagedWrite => "Generated scripts that legitimately produce artifacts.",
            BuiltinProfile::Trusted => "First-party tooling — touring, cargo.",
            BuiltinProfile::Sandboxed => "Default profile for any generic or unverified script.",
        }
    }

    /// Build the concrete [`CapabilityProfile`] for this identifier.
    ///
    /// `workspace` roots the read grant; `staging_dir` roots the write grant of
    /// [`BuiltinProfile::StagedWrite`] (ignored by the other three).
    pub fn build(self, workspace: &Path, staging_dir: &Path) -> CapabilityProfile {
        match self {
            BuiltinProfile::ReadOnly => read_only(workspace),
            BuiltinProfile::StagedWrite => staged_write(workspace, staging_dir),
            BuiltinProfile::Trusted => trusted(),
            BuiltinProfile::Sandboxed => sandboxed(workspace),
        }
    }
}

/// Grant every [`ENV_ALLOWLIST`] key to `profile`.
fn grant_env_allowlist(mut profile: CapabilityProfile) -> CapabilityProfile {
    for key in ENV_ALLOWLIST {
        profile = profile.allowing(Capability::Env(KeyScope::new(*key)));
    }
    profile
}

/// `ReadOnly` — static analysis, classification, dry-run of pure code.
///
/// Default `Deny`; grants read of the `workspace` subtree plus the
/// [`ENV_ALLOWLIST`]. No write, no net, no run.
pub fn read_only(workspace: &Path) -> CapabilityProfile {
    grant_env_allowlist(
        CapabilityProfile::new("ReadOnly", Decision::Deny)
            .allowing(Capability::FsRead(PathScope::new(workspace))),
    )
}

/// `StagedWrite` — generated scripts that legitimately produce artifacts.
///
/// Default `Deny`; grants read of `workspace`, write **only** under
/// `staging_dir`, plus the [`ENV_ALLOWLIST`].
pub fn staged_write(workspace: &Path, staging_dir: &Path) -> CapabilityProfile {
    grant_env_allowlist(
        CapabilityProfile::new("StagedWrite", Decision::Deny)
            .allowing(Capability::FsRead(PathScope::new(workspace)))
            .allowing(Capability::FsWrite(PathScope::new(staging_dir))),
    )
}

/// `Trusted` — first-party tooling (touring / cargo).
///
/// Default `Allow`; `rm` and `sudo` stay denied and all outbound network stays
/// denied — even a trusted profile cannot delete with `rm` or reach the net.
pub fn trusted() -> CapabilityProfile {
    CapabilityProfile::new("Trusted", Decision::Allow)
        .denying(Capability::Run(CmdScope::new("rm")))
        .denying(Capability::Run(CmdScope::new("sudo")))
        .denying(Capability::Net(HostScope::any()))
}

/// Binários de INSPEÇÃO que um programa sandboxado pode invocar.
///
/// Todos são leitura pura: não escrevem, não removem, não alcançam a rede.
/// `git` entra porque suas subformas de leitura (`log`/`diff`/`show`/`status`)
/// são o vocabulário de investigação — a classe DESTRUTIVA da REGRA #11 é
/// barrada a montante pelo `git-safety-guard`, não por este perfil.
///
/// Origem (2026-08-25): o perfil negava TODA capability `Run`, então um
/// programa de code mode não podia chamar `rg` enquanto a chamada `Bash` ao
/// lado podia — o caminho preferido era estritamente mais fraco que o atômico,
/// e a adoção pagava por isso. O harness do DeepSeek escolheu deliberadamente a
/// postura oposta (nota de 15/06, §Trust posture): o runtime deles é
/// *bash-equivalent by design*, sem flag de unsafe, **porque** o bash ao lado
/// já carrega mais autoridade ambiente. A contenção real aqui é a mesma nos
/// dois caminhos — rlimits + landlock + env-clear + política de forbidden-call.
const READ_ONLY_BINARIES: &[&str] = &[
    "rg", "grep", "egrep", "ugrep", "find", "fd", "ls", "cat", "head", "tail", "wc", "sort",
    "uniq", "cut", "tr", "jq", "file", "stat", "readlink", "basename", "dirname", "git",
];

/// `Sandboxed` — the default profile for any generic or unverified script.
///
/// Default `Deny`; grants read of `workspace`, the [`ENV_ALLOWLIST`], e a
/// invocação dos [`READ_ONLY_BINARIES`]. Escrita, rede e qualquer outro
/// executável seguem negados.
pub fn sandboxed(workspace: &Path) -> CapabilityProfile {
    let mut profile = grant_env_allowlist(
        CapabilityProfile::new("Sandboxed", Decision::Deny)
            .allowing(Capability::FsRead(PathScope::new(workspace))),
    );
    for bin in READ_ONLY_BINARIES {
        profile = profile.allowing(Capability::Run(CmdScope::new(*bin)));
    }
    profile
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws() -> &'static Path {
        Path::new("/home/user/ws")
    }

    fn staging() -> &'static Path {
        Path::new("/home/user/.staging")
    }

    /// O caminho preferido não pode ser mais fraco que o atômico.
    ///
    /// Asserção sobre TODA a lista, não sobre um binário de amostra: uma
    /// invariante verificada numa instância reaparece na próxima não coberta.
    #[test]
    fn sandboxed_permite_todo_binario_de_inspecao() {
        let p = sandboxed(ws());
        for bin in READ_ONLY_BINARIES {
            assert!(
                p.allows(&Capability::Run(CmdScope::new(*bin))),
                "programa sandboxado precisa poder invocar `{bin}` — o Bash ao lado pode"
            );
        }
    }

    /// A abertura é cirúrgica: mutação e rede seguem negadas.
    #[test]
    fn sandboxed_segue_negando_mutacao_e_rede() {
        let p = sandboxed(ws());
        for perigoso in ["rm", "mv", "cp", "sudo", "kill", "pkill", "curl", "wget", "cargo", "sh"] {
            assert!(
                !p.allows(&Capability::Run(CmdScope::new(perigoso))),
                "`{perigoso}` jamais pode ser concedido pelo perfil Sandboxed"
            );
        }
        assert!(!p.allows(&Capability::Net(HostScope::any())));
        assert!(!p.allows(&Capability::FsWrite(PathScope::new("/home/user/ws"))));
    }

    /// `ReadOnly` é o perfil de análise ESTÁTICA — continua sem `Run`. A
    /// assimetria com `Sandboxed` é deliberada e por isso é afirmada aqui:
    /// um comentário alegando simetria é a evidência mais fraca que existe.
    #[test]
    fn read_only_permanece_sem_run_apesar_do_sandboxed_ter_ganho() {
        let p = read_only(ws());
        for bin in READ_ONLY_BINARIES {
            assert!(!p.allows(&Capability::Run(CmdScope::new(*bin))));
        }
    }

    #[test]
    fn read_only_grants_workspace_read() {
        let p = read_only(ws());
        assert!(p.allows(&Capability::FsRead(PathScope::new("/home/user/ws/src"))));
    }

    #[test]
    fn read_only_denies_write() {
        let p = read_only(ws());
        assert_eq!(
            p.resolve(&Capability::FsWrite(PathScope::new("/home/user/ws/out"))),
            Decision::Deny
        );
    }

    #[test]
    fn read_only_denies_net_and_run() {
        let p = read_only(ws());
        assert_eq!(
            p.resolve(&Capability::Net(HostScope::any())),
            Decision::Deny
        );
        assert_eq!(
            p.resolve(&Capability::Run(CmdScope::new("ls"))),
            Decision::Deny
        );
    }

    #[test]
    fn staged_write_allows_staging_write() {
        let p = staged_write(ws(), staging());
        assert!(p.allows(&Capability::FsWrite(PathScope::new(
            "/home/user/.staging/x.sh"
        ))));
    }

    #[test]
    fn staged_write_denies_non_staging_write() {
        let p = staged_write(ws(), staging());
        assert_eq!(
            p.resolve(&Capability::FsWrite(PathScope::new(
                "/home/user/ws/src/lib.rs"
            ))),
            Decision::Deny
        );
    }

    #[test]
    fn trusted_allows_arbitrary_by_default() {
        let p = trusted();
        assert!(p.allows(&Capability::FsWrite(PathScope::new("/anywhere"))));
    }

    #[test]
    fn trusted_denies_rm_and_sudo() {
        let p = trusted();
        assert_eq!(
            p.resolve(&Capability::Run(CmdScope::new("rm"))),
            Decision::Deny
        );
        assert_eq!(
            p.resolve(&Capability::Run(CmdScope::new("sudo"))),
            Decision::Deny
        );
    }

    #[test]
    fn trusted_denies_net() {
        let p = trusted();
        assert_eq!(
            p.resolve(&Capability::Net(HostScope::new("example.com", Some(443)))),
            Decision::Deny
        );
    }

    #[test]
    fn trusted_still_allows_safe_run() {
        let p = trusted();
        assert!(p.allows(&Capability::Run(CmdScope::new("cargo"))));
    }

    #[test]
    fn sandboxed_is_deny_by_default() {
        let p = sandboxed(ws());
        assert_eq!(p.default_decision(), Decision::Deny);
        assert_eq!(
            p.resolve(&Capability::Run(CmdScope::new("python3"))),
            Decision::Deny
        );
    }

    #[test]
    fn sandboxed_grants_workspace_read() {
        let p = sandboxed(ws());
        assert!(p.allows(&Capability::FsRead(PathScope::new("/home/user/ws/a.py"))));
    }

    #[test]
    fn env_allowlist_grants_path_but_not_secrets() {
        let p = sandboxed(ws());
        assert!(p.allows(&Capability::Env(KeyScope::new("PATH"))));
        // A credential key is never in the allowlist — sandboxed cannot read it.
        assert_eq!(
            p.resolve(&Capability::Env(KeyScope::new("AWS_SECRET_ACCESS_KEY"))),
            Decision::Deny
        );
    }

    #[test]
    fn builtin_profile_use_case_is_documented() {
        for bp in [
            BuiltinProfile::ReadOnly,
            BuiltinProfile::StagedWrite,
            BuiltinProfile::Trusted,
            BuiltinProfile::Sandboxed,
        ] {
            assert!(!bp.use_case().is_empty());
        }
    }

    #[test]
    fn builtin_profile_build_dispatches() {
        assert_eq!(
            BuiltinProfile::ReadOnly.build(ws(), staging()).name(),
            "ReadOnly"
        );
        assert_eq!(
            BuiltinProfile::StagedWrite.build(ws(), staging()).name(),
            "StagedWrite"
        );
        assert_eq!(
            BuiltinProfile::Trusted.build(ws(), staging()).name(),
            "Trusted"
        );
        assert_eq!(
            BuiltinProfile::Sandboxed.build(ws(), staging()).name(),
            "Sandboxed"
        );
    }

    #[test]
    fn builtin_profile_serde_roundtrip() {
        let bp = BuiltinProfile::Trusted;
        let json = serde_json::to_string(&bp).expect("serialize");
        let back: BuiltinProfile = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(bp, back);
    }
}
