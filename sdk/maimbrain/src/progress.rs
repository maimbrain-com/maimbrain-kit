//! A player's progress across runs, verified by the platform (SPEC §5.8):
//! unlocks, story flags, a run history, a checkpoint to resume a long run.
//! Declare `capabilities = ["store", "progress"]`.
//!
//! Progress is saved data under `progress/`, so it's written and read like
//! any saved data, recorded in replays and the same every time a run is
//! replayed. What's different is what the platform does with it: it replays
//! each finished run on its servers, from the progress the run started with,
//! and keeps the progress the game ended with on the player's account
//! (their other devices get it too). The phone uses new progress right away;
//! if a run doesn't replay the same, the player's progress goes back to the
//! last run that did. So progress only ever comes from play, and a
//! leaderboard can trust it. At most 64 KB; names up to 55 bytes.
//!
//! ```ignore
//! use maimbrain::progress;
//! let runs = progress::get_u64("runs").unwrap_or(0);
//! progress::set_u64("runs", runs + 1);          // at the end of a run
//! ```
//!
//! On the web, where players have no account, progress stays in the browser.

use crate::store;

/// The saved-data prefix progress lives under.
pub const PREFIX: &str = "progress/";
/// The checkpoint's name (`progress/run`).
const CHECKPOINT: &str = "run";

fn key(name: &str) -> String {
    format!("{PREFIX}{name}")
}

/// The progress saved as `name`, if any.
pub fn get(name: &str) -> Option<Vec<u8>> {
    store::get(&key(name))
}

/// Saves progress as `name` (empty removes it). False if the name is too long
/// (over 55 bytes) or progress would go over 64 KB.
pub fn set(name: &str, value: &[u8]) -> bool {
    store::set(&key(name), value)
}

pub fn remove(name: &str) -> bool {
    set(name, &[])
}

pub fn get_u64(name: &str) -> Option<u64> {
    store::get_u64(&key(name))
}

pub fn set_u64(name: &str, value: u64) -> bool {
    store::set_u64(&key(name), value)
}

/// Saves where a run is, to pick it up in a later session (between floors
/// of a long run, say). The platform also resumes a run the player left by
/// replaying it to where they were, with nothing for the game to do; a
/// checkpoint is for when it can't (a run left a day ago, the game updated
/// since, a run longer than 15 minutes). Read it back in `init` with
/// [`checkpoint`], and [`clear_checkpoint`] when the run ends.
pub fn save_checkpoint(state: &[u8]) -> bool {
    set(CHECKPOINT, state)
}

/// The checkpoint a run left, if one is waiting.
pub fn checkpoint() -> Option<Vec<u8>> {
    get(CHECKPOINT).filter(|v| !v.is_empty())
}

pub fn clear_checkpoint() -> bool {
    remove(CHECKPOINT)
}
