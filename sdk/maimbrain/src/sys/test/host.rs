//! A fake host for native tests: it runs a [`Game`](crate::Game) frame by
//! frame as the platform does, hands it the input a test queues, and records
//! what the game asks of it (the round state, scores, saved data, logs) so a
//! test can check the platform's rules (SKILL.md "Platform facts that bite").
//! [`Conformance`](super::Conformance) is the ready-made check.
//!
//! It's per thread and only there while a [`Host`] is: without one, the
//! native stubs behave as they always have (no input, nothing saved).

use std::cell::RefCell;
use std::collections::BTreeMap;

use crate::input::{Event, synth};
use crate::sys::{Level, Round};

/// What the game asked the host for, in order. `frame` is 0 for `init` and
/// counts frames after; `time` is game time then.
#[derive(Clone, Debug, PartialEq)]
pub enum Call {
    /// `sys::round`.
    Round { frame: u64, time: f64, state: Round },
    /// `store::submit_score`.
    Score { frame: u64, time: f64, board: u32, value: i64 },
    /// `store::show_scores`.
    ShowScores { frame: u64, board: u32 },
    /// `store::set` (an empty value deletes the key).
    Save { frame: u64, key: String, value: Vec<u8> },
    /// `store::get`.
    Load { frame: u64, key: String },
    /// `sys::log`.
    Log { frame: u64, level: Level, message: String },
}

impl Call {
    /// The frame it happened in (0 = `init`).
    pub fn frame(&self) -> u64 {
        match self {
            Call::Round { frame, .. } | Call::Score { frame, .. } | Call::ShowScores { frame, .. } | Call::Save { frame, .. } | Call::Load { frame, .. } | Call::Log { frame, .. } => *frame,
        }
    }
}

struct State {
    seed: u64,
    daily: u64,
    frame: u64,
    queue: synth::Queue,
    /// This frame's events, handed out by `input::poll`.
    due: Vec<Event>,
    calls: Vec<Call>,
    store: BTreeMap<String, Vec<u8>>,
    /// The board `Rounds::daily` was given.
    daily_board: Option<u32>,
    /// This frame's draw fingerprint so far, and each frame's since `init`.
    drawing: u64,
    drawn: Vec<u64>,
}

thread_local! {
    static HOST: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn with<T>(f: impl FnOnce(&mut State) -> T) -> Option<T> {
    HOST.with(|h| h.borrow_mut().as_mut().map(f))
}

fn record(c: impl FnOnce(u64, f64) -> Call) {
    let time = super::time();
    with(|s| {
        let call = c(s.frame, time);
        s.calls.push(call)
    });
}

/// A fake host on this thread, until it's dropped. Starting one resets the
/// clock and debug parameters ([`reset`](super::reset)).
///
/// ```
/// use maimbrain::sys::{self, Round, test::Host};
/// # struct MyGame { playing: bool }
/// # impl maimbrain::Game for MyGame {
/// #     fn init() -> Self { sys::round(Round::Idle); MyGame { playing: false } }
/// #     fn update(&mut self, _dt: f32) {
/// #         for e in maimbrain::input::poll() {
/// #             if e.is_press() && !self.playing { self.playing = true; sys::round(Round::Playing); }
/// #         }
/// #     }
/// #     fn render(&self) {}
/// # }
/// let host = Host::start(7);
/// let mut game: MyGame = host.init();
/// host.tap(180.0, 320.0);
/// host.frame(&mut game, 1.0 / 60.0);
/// assert_eq!(host.rounds(), [Round::Idle, Round::Playing]);
/// ```
pub struct Host(());

impl Host {
    /// A host whose sessions are seeded with `seed` (`sys::rand_seed`); the
    /// daily seed is `seed ^ 0xda11`, unless [`set_daily_seed`](Host::set_daily_seed).
    pub fn start(seed: u64) -> Host {
        super::reset();
        HOST.with(|h| {
            *h.borrow_mut() = Some(State {
                seed,
                daily: seed ^ 0xda11,
                frame: 0,
                queue: synth::Queue::new(),
                due: Vec::new(),
                calls: Vec::new(),
                store: BTreeMap::new(),
                daily_board: None,
                drawing: FNV,
                drawn: Vec::new(),
            })
        });
        Host(())
    }

    pub fn set_daily_seed(&self, seed: u64) {
        with(|s| s.daily = seed);
    }

    /// A new session of the game, as the platform starts one: the clock back
    /// to 0, then `init`. Saved data and recorded calls carry on (a player
    /// coming back to the game).
    pub fn init<G: crate::Game>(&self) -> G {
        super::set_time(0.0);
        with(|s| {
            s.frame = 0;
            s.queue = synth::Queue::new();
            s.due.clear();
            s.drawn.clear();
        });
        G::init()
    }

    /// One frame: game time moves on by `dt`, the input queued up to then is
    /// handed to `input::poll`, then `update(dt)` and `render()`.
    pub fn frame<G: crate::Game>(&self, game: &mut G, dt: f32) {
        super::advance(dt);
        let now = super::time();
        with(|s| {
            s.frame += 1;
            s.due = s.queue.due(now);
            s.drawing = FNV;
        });
        game.update(dt);
        with(|s| s.due.clear());
        game.render();
        with(|s| s.drawn.push(s.drawing));
    }

    /// Frames of `dt` until `seconds` more of game time have passed.
    pub fn run<G: crate::Game>(&self, game: &mut G, seconds: f32, dt: f32) {
        let end = super::time() + seconds as f64 - 1e-6;
        while super::time() < end {
            self.frame(game, dt);
        }
    }

    /// Queues touches (with their own times, as `input::synth` makes them):
    /// each is handed to the frame whose time first reaches it.
    pub fn push(&self, events: impl IntoIterator<Item = Event>) {
        let events: Vec<Event> = events.into_iter().collect();
        with(|s| s.queue.push(events));
    }

    /// A quick tap at (x, y), starting in the next frame.
    pub fn tap(&self, x: f32, y: f32) {
        self.push(synth::tap(x, y, super::time() + 1e-4));
    }

    /// Game time now (s).
    pub fn now(&self) -> f64 {
        super::time()
    }

    /// Frames run since the last `init`.
    pub fn frames(&self) -> u64 {
        with(|s| s.frame).unwrap_or(0)
    }

    /// Everything the game asked for so far, in order.
    pub fn calls(&self) -> Vec<Call> {
        with(|s| s.calls.clone()).unwrap_or_default()
    }

    /// Forgets the recorded calls (saved data stays).
    pub fn clear_calls(&self) {
        with(|s| s.calls.clear());
    }

    /// The round states the game reported, in order.
    pub fn rounds(&self) -> Vec<Round> {
        self.calls().into_iter().filter_map(|c| if let Call::Round { state, .. } = c { Some(state) } else { None }).collect()
    }

    /// The scores the game submitted: (board, value), in order.
    pub fn scores(&self) -> Vec<(u32, i64)> {
        self.calls().into_iter().filter_map(|c| if let Call::Score { board, value, .. } = c { Some((board, value)) } else { None }).collect()
    }

    /// What's saved under `key`.
    pub fn saved(&self, key: &str) -> Option<Vec<u8>> {
        with(|s| s.store.get(key).cloned()).flatten()
    }

    /// Saves `value` under `key`, as an earlier session would have.
    pub fn save(&self, key: &str, value: &[u8]) {
        with(|s| s.store.insert(key.to_string(), value.to_vec()));
    }

    /// The board the game's `Rounds` plays its daily mode on (`Rounds::daily`).
    pub fn rounds_daily_board(&self) -> Option<u32> {
        with(|s| s.daily_board).flatten()
    }

    /// A fingerprint of what each frame since `init` drew (frame 1 first):
    /// a hash of its mb2d draw calls and the mb3d camera and node changes
    /// made in `update` and `render`, with their arguments. Two frames that
    /// drew the same things the same way match. (Natively no asset loads,
    /// so it's shapes, text, transforms and sprite rectangles.) It tells
    /// whether input changed anything on screen without knowing the game.
    pub fn drawn(&self) -> Vec<u64> {
        with(|s| s.drawn.clone()).unwrap_or_default()
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        HOST.with(|h| *h.borrow_mut() = None);
    }
}

// The native ffi stubs (ffi.rs): the host's when there is one.

const FNV: u64 = 0xcbf2_9ce4_8422_2325;

/// A draw call: its tag and arguments go into the frame's fingerprint.
pub(crate) fn draw(words: &[u32]) {
    with(|s| {
        for w in words {
            for b in w.to_le_bytes() {
                s.drawing = (s.drawing ^ b as u64).wrapping_mul(0x100_0000_01b3);
            }
        }
    });
}

/// A draw call with a block of floats or bytes behind a pointer.
pub(crate) unsafe fn draw_bytes(tag: u32, ptr: *const u8, len: usize) {
    draw(&[tag, len as u32]);
    if ptr.is_null() || len == 0 {
        return;
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    with(|s| {
        for b in bytes {
            s.drawing = (s.drawing ^ *b as u64).wrapping_mul(0x100_0000_01b3);
        }
    });
}

pub(crate) fn round(state: u32) {
    let state = match state {
        0 => Round::Idle,
        1 => Round::Playing,
        _ => Round::Over,
    };
    record(|frame, time| Call::Round { frame, time, state });
}

pub(crate) fn score_submit(board: u32, value: i64) -> i32 {
    record(|frame, time| Call::Score { frame, time, board, value });
    0
}

pub(crate) fn score_show(board: u32) {
    record(|frame, _| Call::ShowScores { frame, board });
}

pub(crate) fn rand_seed() -> u64 {
    with(|s| s.seed).unwrap_or(0)
}

pub(crate) fn daily_seed() -> u64 {
    with(|s| s.daily).unwrap_or(0)
}

pub(crate) fn note_daily_board(board: u32) {
    with(|s| s.daily_board = Some(board));
}

unsafe fn text(ptr: *const u8, len: u32) -> String {
    String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(ptr, len as usize) }).into_owned()
}

pub(crate) unsafe fn log(level: u32, ptr: *const u8, len: u32) {
    let level = match level {
        0 => Level::Debug,
        1 => Level::Info,
        2 => Level::Warn,
        _ => Level::Error,
    };
    let message = unsafe { text(ptr, len) };
    record(|frame, _| Call::Log { frame, level, message });
}

pub(crate) unsafe fn input_poll(buf: *mut u8, cap: u32) -> i32 {
    with(|s| {
        let n = s.due.len().min(cap as usize);
        let out = buf as *mut Event;
        for (i, e) in s.due.drain(..n).enumerate() {
            unsafe { out.add(i).write_unaligned(e) };
        }
        n as i32
    })
    .unwrap_or(0)
}

pub(crate) unsafe fn store_get(key: *const u8, key_len: u32, buf: *mut u8, cap: u32) -> i32 {
    let key = unsafe { text(key, key_len) };
    with(|s| {
        s.calls.push(Call::Load { frame: s.frame, key: key.clone() });
        match s.store.get(&key) {
            Some(v) => {
                let n = v.len().min(cap as usize);
                if n > 0 {
                    unsafe { std::ptr::copy_nonoverlapping(v.as_ptr(), buf, n) };
                }
                v.len() as i32
            }
            None => -1,
        }
    })
    .unwrap_or(-1)
}

pub(crate) unsafe fn store_set(key: *const u8, key_len: u32, val: *const u8, val_len: u32) -> i32 {
    let key = unsafe { text(key, key_len) };
    let value = if val_len == 0 { Vec::new() } else { unsafe { std::slice::from_raw_parts(val, val_len as usize) }.to_vec() };
    with(|s| {
        s.calls.push(Call::Save { frame: s.frame, key: key.clone(), value: value.clone() });
        if value.is_empty() {
            s.store.remove(&key);
        } else {
            s.store.insert(key, value);
        }
    });
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Game, input, store, sys};

    /// Taps count; the round ends at 3 taps.
    struct Taps {
        n: u32,
        best: u64,
    }

    impl Game for Taps {
        fn init() -> Self {
            sys::round(Round::Idle);
            Taps { n: 0, best: store::get_u64("best").unwrap_or(0) }
        }
        fn update(&mut self, _dt: f32) {
            for e in input::poll() {
                if e.is_press() {
                    self.n += 1;
                    if self.n == 1 {
                        sys::round(Round::Playing);
                    } else if self.n == 3 {
                        store::submit_score(0, 3);
                        store::set_u64("best", 3);
                        sys::round(Round::Over);
                    }
                }
            }
        }
        fn render(&self) {}
    }

    #[test]
    fn the_host_delivers_input_and_records_what_the_game_asks() {
        let host = Host::start(5);
        assert_eq!(sys::rand_seed(), 5);
        let mut g: Taps = host.init();
        for i in 0..3 {
            host.push(synth::tap(10.0, 10.0, 0.1 + 0.2 * i as f64));
        }
        host.run(&mut g, 1.0, 1.0 / 60.0);
        assert_eq!(g.n, 3);
        assert_eq!(host.rounds(), [Round::Idle, Round::Playing, Round::Over]);
        assert_eq!(host.scores(), [(0, 3)]);
        // Each tap reached the frame whose time first passed it.
        let playing = host.calls().into_iter().find_map(|c| if let Call::Round { frame, state: Round::Playing, .. } = c { Some(frame) } else { None });
        assert_eq!(playing, Some(6), "the frame that reaches 0.1 s, at 60 frames a second");
        // A new session reads what the last one saved.
        let g2: Taps = host.init();
        assert_eq!(g2.best, 3);
    }

    #[test]
    fn without_a_host_the_stubs_are_as_before() {
        assert_eq!(store::get("best"), None);
        assert!(store::set("best", b"x"));
        assert_eq!(store::get("best"), None, "nothing is saved without a host");
        assert!(input::poll().is_empty());
        assert_eq!(sys::rand_seed(), 0);
    }
}
