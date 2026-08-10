//! Zero-copy serialization templates for touring crates.
//!
//! Centralizes common `#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]`
//! patterns used across touring-index, touring-hooks, touring-learning, and touring-cognitive.
//!
//! All 13 template types get a `CheckBytes` impl for byte validation on
//! deserialization, ensuring data integrity for cross-crate IPC and persistence.
//! Since rkyv 0.8 that is automatic with the `bytecheck` feature (this crate's
//! `validation` feature forwards to it); the 0.7 opt-in `#[archive(check_bytes)]`
//! no longer exists.
//!
//! # Usage
//!
//! Consumer crates should add `touring-rkyv` as a dependency and use:
//! ```ignore
//! use touring_rkyv::{Archive, Serialize, Deserialize};
//! use touring_rkyv::templates::{ArchivedHookEvent, ArchivedSymbol};
//! ```
//!
//! # Crates Using These Templates
//!
//! - `touring-index` — symbol index snapshots
//! - `touring-hooks` — dependency graph snapshots (ArchivedIndexSnapshot)
//! - `touring-learning` — RL state snapshots (QTable, LinUCB, ESAA event records, CRDT graphs)
//!
//! # Note on touring-cognitive
//!
//! touring-cognitive defines its own local GoTSnapshot/GotNodeSnapshot types
//! in `snapshot.rs` because they capture engine-specific state (max_depth,
//! beam_width, pheromone_trails) semantically different from the IPC templates.
//! See `touring-cognitive/src/snapshot.rs` for details.

#![deny(missing_docs)]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free leaf — lock against
// future bare unwrap in non-test code (`.expect("…")` stays the sanctioned escape).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod ipc;
pub mod saga_ipc;
pub mod templates;

// Re-export IPC types so downstream crates can write:
//   use touring_rkyv::{IpcRequest, IpcResponse, frame_request, unframe};
pub use ipc::{
    ArchivedIpcRequest, ArchivedIpcResponse, FrameError, IPC_FRAME_HEADER_LEN, IPC_MAGIC,
    IpcRequest, IpcResponse, frame_request, frame_response, unframe,
};

// Re-export saga coordination types for distributed 2PC
pub use saga_ipc::{SAGA_FRAME_LEN, SAGA_MAGIC, SagaError, SagaMessage, frame_saga, unframe_saga};

// Re-export rkyv derive macros so consumer crates can use:
//   use touring_rkyv::{Archive, Serialize, Deserialize};
// instead of:
//   use rkyv::{Archive, Serialize, Deserialize};
pub use rkyv::{Archive, Deserialize, Serialize};

// Re-export commonly used rkyv functions and modules
pub use rkyv::ser;
pub use rkyv::util::AlignedVec;

/// Teto conservador, em bytes, para o que pode ser serializado com segurança.
///
/// O rkyv 0.8 endereça o conteúdo de um arquivo por **ponteiros relativos de
/// `i32`**, então nada além de `i32::MAX` de deslocamento é representável. O
/// problema não é o limite — é o comportamento no limite: o caminho
/// `RelPtr::emplace` que `Vec`/`String` percorrem fixa a estratégia de erro em
/// `rancor::Panic`, então o estouro vira **panic do processo** em vez de `Err`,
/// mesmo quando o chamador pediu `rancor::Error`. Verificado por backtrace no
/// rkyv 0.8.18 (`rel_ptr.rs:411` → `rancor/lib.rs:648`); no 0.7 o mesmo caso
/// retornava erro.
///
/// Como o daemon serializa dados que chegam pelo socket, um payload gigante não
/// pode derrubar o processo. Os pontos de framing checam este teto **antes** de
/// chamar o rkyv e devolvem o erro de payload grande já documentado.
///
/// O teto é aplicado sobre os bytes **crus** dos campos. Um arquivo é sempre
/// ≥ os dados que carrega, logo qualquer valor acima daqui certamente não cabe;
/// a recíproca não vale — passar no teste não prova que cabe, apenas afasta a
/// faixa em que o panic é certo. É um piso de robustez, não uma garantia exata.
pub const MAX_ARCHIVE_BYTES: usize = i32::MAX as usize;

// ── Adaptadores 0.7 → 0.8 (P2 da migração, RUSTSEC-2026-0235) ───────────────
//
// Estes NÃO são `pub use`, e a diferença é o ponto inteiro da fachada: no 0.8 as
// três funções abaixo mudaram de SEMÂNTICA, não só de nome. `access` recebe o
// tipo ARCHIVED (`T::Archived`), enquanto o `check_archived_root` do 0.7 recebia
// o tipo FONTE (`T`); reexportar uma sob o nome da outra faria todo consumidor
// chamar com o tipo errado. Envolver preserva a assinatura vista de fora, que é
// o contrato que permitiu ao P1 canalizar 40 call sites sem tocá-los.

/// Valida e acessa a raiz arquivada — forma do 0.7 preservada.
///
/// Chamada como `check_archived_root::<T>(&bytes)`, exatamente como antes; por
/// dentro traduz para `rkyv::access::<T::Archived, _>`.
pub fn check_archived_root<T>(bytes: &[u8]) -> Result<&T::Archived, rkyv::rancor::Error>
where
    T: rkyv::Archive,
    T::Archived: for<'a> rkyv::bytecheck::CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>,
{
    rkyv::access::<T::Archived, rkyv::rancor::Error>(bytes)
}

/// Acesso sem validação — forma do 0.7 preservada.
///
/// # Safety
///
/// Os bytes devem ser um arquivo `rkyv` válido e alinhado para `T::Archived`,
/// produzido por [`to_bytes`]. Sem validação, bytes de outra versão do formato
/// são lidos como lixo em silêncio — foi o que o SPIKE do P0 mediu.
pub unsafe fn archived_root<T: rkyv::Archive>(bytes: &[u8]) -> &T::Archived {
    // SAFETY: contrato repassado ao chamador, idêntico ao que `archived_root`
    // do 0.7 exigia.
    unsafe { rkyv::access_unchecked::<T::Archived>(bytes) }
}

/// Bound de serialização da fachada — o único lugar que soletra a forma 0.8.
///
/// No 0.7 bastava `T: Serialize<AllocSerializer<N>>`; o 0.8 trocou isso por uma
/// `HighSerializer` com writer, alocador e estratégia de erro parametrizados —
/// seis linhas de ruído que apareceriam em cada assinatura genérica. Concentrar
/// aqui é a mesma tese da fachada aplicada aos *bounds*: quando o 0.9 mudar a
/// forma outra vez, muda-se ESTA linha, não cada `where` espalhado.
///
/// É um alias por blanket impl (Rust estável não tem `trait alias`): todo tipo
/// que satisfaz o bound real já é `Serializable`, sem `impl` manual.
pub trait Serializable:
    for<'a> rkyv::Serialize<
        rkyv::api::high::HighSerializer<
            AlignedVec,
            rkyv::ser::allocator::ArenaHandle<'a>,
            rkyv::rancor::Error,
        >,
    >
{
}

impl<T> Serializable for T where
    T: for<'a> rkyv::Serialize<
            rkyv::api::high::HighSerializer<
                AlignedVec,
                rkyv::ser::allocator::ArenaHandle<'a>,
                rkyv::rancor::Error,
            >,
        >
{
}

/// Serializa — forma do 0.7 preservada, inclusive o parâmetro de scratch.
///
/// O 0.7 pedia `to_bytes::<_, N>(&v)` com `N` dimensionando o scratch buffer
/// (os call sites usam 256, 1024, 4096, 32768 e 65536). O 0.8 gerencia o
/// scratch sozinho e removeu esse const genérico, mas ele é MANTIDO aqui — e
/// ignorado — para que nenhuma chamada precise mudar.
pub fn to_bytes<T: Serializable, const N: usize>(
    value: &T,
) -> Result<AlignedVec, rkyv::rancor::Error> {
    rkyv::to_bytes::<rkyv::rancor::Error>(value)
}

/// Desserializa a partir de bytes validados — forma do 0.7 preservada.
///
/// O 0.8 acrescentou um segundo genérico de estratégia de erro
/// (`from_bytes::<T, E>`). Enquanto isto foi um `pub use` cru, o segundo
/// parâmetro ficava sem inferência e TODO consumidor era obrigado a soletrar
/// `rkyv::rancor::Error` — ou seja, a fachada vazava justamente o conhecimento
/// de versão que ela existe para absorver. O sintoma foi real e mediu-se
/// sozinho: `type annotations needed` em
/// `touring-storage/src/embedding/client.rs:268`, visível apenas sob
/// `--all-targets` (a unificação de features mascarava).
///
/// Fixando `E = rancor::Error` aqui, a chamada volta a ser `from_bytes::<T>(b)`
/// — exatamente o que os call sites já escrevem — e nenhum deles muda.
pub fn from_bytes<T>(bytes: &[u8]) -> Result<T, rkyv::rancor::Error>
where
    T: rkyv::Archive,
    T::Archived: for<'a> rkyv::bytecheck::CheckBytes<rkyv::api::high::HighValidator<'a, rkyv::rancor::Error>>
        + rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>,
{
    rkyv::from_bytes::<T, rkyv::rancor::Error>(bytes)
}

/// Desserializa um valor arquivado de volta ao tipo dono.
///
/// Substitui o par `Deserialize::deserialize(archived, &mut Infallible)` do 0.7.
/// `Infallible` era um TIPO-MARCADOR de desserializador; o 0.8 trocou isso por
/// estratégias de erro (`rancor`), e não existe alias honesto — por isso aqui é
/// uma função, e não um `pub use`. É a única mudança que alcança o consumidor,
/// porque a forma antiga não tem contraparte; são **5 call sites**
/// (`touring-hooks-core/src/dependency_cache.rs` mais quatro em
/// `touring-intelligence`: `reasoning/snapshot.rs`, `rl/bandit/linucb.rs`,
/// `rl/rl/qtable.rs`, `rl/memory/crdt_graph.rs`).
pub fn deserialize<T>(archived: &T::Archived) -> Result<T, rkyv::rancor::Error>
where
    T: rkyv::Archive,
    T::Archived: rkyv::Deserialize<T, rkyv::api::high::HighDeserializer<rkyv::rancor::Error>>,
{
    rkyv::deserialize::<T, rkyv::rancor::Error>(archived)
}
