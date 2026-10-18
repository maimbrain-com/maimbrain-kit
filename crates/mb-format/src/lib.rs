//! The Maimbrain `.mbx` bundle format (docs/SPEC.md §1–§5): manifest schema,
//! host import table, deterministic packing and validation. Shared by the
//! `mb` CLI and the iOS client so both enforce exactly the same rules.

pub mod abi;
pub mod bundle;
pub mod font;
pub mod manifest;
pub mod meter;
pub mod studio;
pub mod wasm;

pub use bundle::{Bundle, Report, pack, validate};

/// The shared daily seed (SPEC §5.1): the same for every player of a game on
/// a UTC day. `date` is `YYYY-MM-DD`.
pub fn daily_seed(game_id: &str, date: &str) -> u64 {
    use sha2::Digest;
    let h = sha2::Sha256::digest(format!("maimbrain-daily|{game_id}|{date}"));
    u64::from_le_bytes(h[..8].try_into().unwrap())
}
pub use manifest::Manifest;
