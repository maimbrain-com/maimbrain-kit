//! Always-available host services (SPEC §5.1).

use crate::ffi;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Level {
    Debug = 0,
    Info = 1,
    Warn = 2,
    Error = 3,
}

pub fn log(level: Level, msg: &str) {
    unsafe { ffi::mb_log(level as u32, msg.as_ptr(), msg.len() as u32) }
}

/// Marks this frame as one where motion changes on purpose: a jump, a dash,
/// a hit's knockback, a camera cut. It does nothing in the game and never
/// reaches the player; tools see it (SPEC §5.1): `mb shot --smoothness`
/// reports a spike in the camera's motion within two frames of a mark as
/// "marked: <label>" and doesn't count it as a problem, `mb shot --at
/// mark:boss` shoots the first frame marked "boss" (a moment worth looking
/// at: the boss arrives, a part breaks), and `mb.step()` and `mb.stats()`
/// list the frame's marks. Call it from `update`, in the frame that applies
/// the impulse or shows the moment; a short label (at most 64 bytes are kept).
/// ```
/// use maimbrain::sys;
/// sys::mark("jump");
/// ```
pub fn mark(label: &str) {
    unsafe { ffi::mb_mark(label.as_ptr(), label.len() as u32) }
}

/// A debug parameter the preview tools passed in: `mb shot --param zone=3`,
/// `mb measure --param zone=3`, `mb.restart({ params: { zone: "3" } })` or a
/// preview page's `?param=zone=3` (SPEC §5.1). Use them to reach a late
/// moment without editing the game: start in zone 3, with 39 pearls, the
/// first strike due now, the autopilot on.
///
/// Always None on a device and in the feed, so a shipped game behaves as it
/// does without them; read them only to choose where to start, never for
/// anything a player would see. They're fixed for the session (set before
/// `init`) and recorded in its replay log, so replays and `mb.verify()` match.
/// ```
/// use maimbrain::sys;
/// let zone: usize = sys::param_as("zone").unwrap_or(0);
/// let autopilot = sys::param("autopilot").is_some();
/// # let _ = (zone, autopilot);
/// ```
pub fn param(name: &str) -> Option<String> {
    let mut buf = vec![0u8; 64];
    loop {
        let n = unsafe { ffi::mb_param(name.as_ptr(), name.len() as u32, buf.as_mut_ptr(), buf.len() as u32) };
        let n = usize::try_from(n).ok()?;
        if n <= buf.len() {
            buf.truncate(n);
            return String::from_utf8(buf).ok();
        }
        buf.resize(n, 0);
    }
}

/// [`param`] parsed as a number or anything else `FromStr`: `--param zone=3`
/// gives `param_as::<usize>("zone") == Some(3)`. None when it's absent or
/// doesn't parse.
pub fn param_as<T: std::str::FromStr>(name: &str) -> Option<T> {
    param(name)?.trim().parse().ok()
}

/// Game time in seconds: the sum of every `dt` passed to `update` so far,
/// this frame's included. Not wall-clock time (SPEC §3). Natively (`cargo
/// test`) it's a clock the test drives: 0 until [`test::advance`] (or
/// [`test::update`], which advances it and calls `update`) moves it.
pub fn time() -> f64 {
    unsafe { ffi::mb_time() }
}

/// Wall-clock seconds this frame that game time did *not* get: what the
/// `dt ≤ 0.1` clamp dropped after a hitch or stall (SPEC §5.1). Usually 0.
/// Music scheduled earlier is now that far ahead of game time, so a rhythm
/// game re-anchors when this is > 0. Recorded, so replays see the same
/// values. 0 across a suspend/resume (the platform pauses audio with it).
pub fn time_lost() -> f32 {
    unsafe { ffi::mb_time_lost() }
}

/// Per-session seed. Seed your own PRNG from it, e.g. [`crate::Rng`].
pub fn rand_seed() -> u64 {
    unsafe { ffi::mb_rand_seed() }
}

/// The same for every player of this game on a given UTC day: use it for
/// daily challenges everyone plays identically.
pub fn daily_seed() -> u64 {
    unsafe { ffi::mb_daily_seed() }
}

/// Where the game is in its round (SPEC §5.1). The platform lets the player
/// swipe between games unless a round is being played, and turns a finished
/// round's screen into a feed card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Round {
    /// Title or menu: nothing at stake.
    Idle = 0,
    /// A round is in progress.
    Playing = 1,
    /// The round just ended (game-over screen).
    Over = 2,
}

pub fn round(state: Round) {
    unsafe { ffi::mb_round(state as u32) }
}

/// Screen geometry in logical units. A portrait letterboxed game's is the
/// same all session; an `orientation = "any"`, `"landscape"` or
/// `fit = "fill"` game's can change at a frame boundary, and an
/// [`input::Kind::Resize`](crate::input::Kind::Resize) event says when
/// (SPEC §5.2): read it again then rather than keeping a copy from `init`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct Screen {
    pub width: f32,
    pub height: f32,
    /// Safe-area insets. `inset_bottom` includes the platform's reserved
    /// feed strip; keep interactive elements out of it (SPEC §5.2).
    pub inset_top: f32,
    pub inset_right: f32,
    pub inset_bottom: f32,
    pub inset_left: f32,
    /// Device pixels per logical unit.
    pub dpr: f32,
    pub _reserved: f32,
}

impl Screen {
    /// Wider than it is tall: an `any` game turned sideways, or a landscape game.
    pub fn is_landscape(&self) -> bool {
        self.width > self.height
    }
}

/// Assets load asynchronously; readiness changes only at frame boundaries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Asset(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetState {
    Pending,
    Ready,
    Failed,
}

impl Asset {
    /// Starts loading `path` (relative to the bundle root, e.g. "assets/hit.ogg").
    pub fn load(path: &str) -> Asset {
        Asset(unsafe { ffi::mb_asset_load(path.as_ptr(), path.len() as u32) })
    }

    pub fn state(self) -> AssetState {
        match unsafe { ffi::mb_asset_state(self.0) } {
            0 => AssetState::Pending,
            1 => AssetState::Ready,
            _ => AssetState::Failed,
        }
    }

    /// The raw bytes, once ready (for .json/.bin/.txt).
    pub fn read(self) -> Option<Vec<u8>> {
        let n = unsafe { ffi::mb_asset_read(self.0, std::ptr::null_mut(), 0) };
        if n < 0 {
            return None;
        }
        let mut v = vec![0u8; n as usize];
        unsafe { ffi::mb_asset_read(self.0, v.as_mut_ptr(), n as u32) };
        Some(v)
    }
}

/// Asks the platform to open an http(s) URL. It always shows a "leaving
/// Maimbrain" interstitial first. Needs `capabilities = ["links"]`.
pub fn open_link(url: &str) -> bool {
    unsafe { ffi::mb_link_open(url.as_ptr(), url.len() as u32) == 0 }
}

pub fn screen() -> Screen {
    let mut s = Screen::default();
    unsafe { ffi::mb_screen(&mut s as *mut Screen as *mut u8) };
    s
}

/// The clock off wasm32 (`cargo test`, SPEC §9): on a device and in the
/// browser the host owns game time; natively these drive what
/// [`time`](super::time) and [`time_lost`](super::time_lost) return, so code
/// that schedules on game time (a beat grid, `play_at`, cooldowns read from
/// `sys::time()`) can be tested as it runs. Each test thread has its own
/// clock, starting at 0, so tests running in parallel don't disturb each
/// other. On wasm32 the functions are absent.
///
/// ```
/// use maimbrain::sys;
/// sys::test::reset();
/// sys::test::advance(1.0 / 60.0);
/// assert!((sys::time() - 1.0 / 60.0).abs() < 1e-6);   // dt is an f32, as update gets it
/// sys::test::set_time(12.5);
/// assert_eq!(sys::time(), 12.5);
/// ```
///
/// Drive a whole game as the host would, `time()` including each frame's `dt`
/// during `update`:
///
/// ```ignore
/// let mut game = MyGame::init();
/// for _ in 0..600 {
///     sys::test::update(&mut game, 1.0 / 60.0);   // advance(dt), then game.update(dt)
/// }
/// ```
///
/// Debug parameters work the same way: `set_param("zone", "3")` and
/// [`param`](super::param) returns it on this thread until `clear_params()`
/// or `reset()`.
///
/// To test the game as the platform runs it (input, the round state, scores,
/// saved data), run it under a [`Host`], or let [`Conformance`] check the
/// platform's rules with one:
///
/// ```ignore
/// #[test]
/// fn keeps_the_platform_rules() {
///     maimbrain::sys::test::Conformance::new().check::<MyGame>();
/// }
/// ```
#[cfg(not(target_arch = "wasm32"))]
pub mod test {
    use std::cell::{Cell, RefCell};
    use std::collections::BTreeMap;

    mod conformance;
    pub(crate) mod host;
    pub use conformance::{Conformance, Finding};
    pub use host::{Call, Host};

    thread_local! {
        static NOW: Cell<f64> = const { Cell::new(0.0) };
        static LOST: Cell<f32> = const { Cell::new(0.0) };
        static PARAMS: RefCell<BTreeMap<String, String>> = const { RefCell::new(BTreeMap::new()) };
    }

    /// Sets a debug parameter for this thread, as `mb shot --param name=value` does.
    pub fn set_param(name: &str, value: &str) {
        PARAMS.with(|p| p.borrow_mut().insert(name.to_string(), value.to_string()));
    }

    /// Removes every debug parameter on this thread.
    pub fn clear_params() {
        PARAMS.with(|p| p.borrow_mut().clear());
    }

    /// The native `mb_param`: the value's length (copying up to `cap` bytes), or −1.
    #[doc(hidden)]
    pub unsafe fn param_raw(name: *const u8, name_len: u32, out: *mut u8, cap: u32) -> i32 {
        let name = unsafe { std::slice::from_raw_parts(name, name_len as usize) };
        let Ok(name) = std::str::from_utf8(name) else { return -1 };
        PARAMS.with(|p| match p.borrow().get(name) {
            Some(v) => {
                let n = v.len().min(cap as usize);
                unsafe { std::ptr::copy_nonoverlapping(v.as_ptr(), out, n) };
                v.len() as i32
            }
            None => -1,
        })
    }

    /// What `sys::time()` returns on this thread.
    pub fn time() -> f64 {
        NOW.with(Cell::get)
    }

    /// What `sys::time_lost()` returns on this thread.
    pub fn time_lost() -> f32 {
        LOST.with(Cell::get)
    }

    /// Sets this thread's game time (seconds).
    pub fn set_time(t: f64) {
        NOW.with(|c| c.set(t));
    }

    /// Moves this thread's game time on by `dt` seconds, as the host does
    /// before each `update(dt)`. Clears `time_lost`, as a frame without a
    /// hitch would.
    pub fn advance(dt: f32) {
        NOW.with(|c| c.set(c.get() + dt as f64));
        LOST.with(|c| c.set(0.0));
    }

    /// Makes the next frame one after a hitch: `time_lost()` returns
    /// `seconds` until the next `advance`.
    pub fn set_time_lost(seconds: f32) {
        LOST.with(|c| c.set(seconds));
    }

    /// Back to 0, no time lost and no debug parameters: call at the start of
    /// a test that reads the clock or parameters.
    pub fn reset() {
        set_time(0.0);
        LOST.with(|c| c.set(0.0));
        clear_params();
        crate::input::reset_held();
        crate::debug::reset();
    }

    /// One frame as the host runs it: `advance(dt)`, then `game.update(dt)`.
    pub fn update<G: crate::Game>(game: &mut G, dt: f32) {
        advance(dt);
        game.update(dt);
    }

    #[cfg(test)]
    mod tests {
        use crate::sys;

        #[test]
        fn the_native_clock_follows_the_test() {
            sys::test::reset();
            assert_eq!(sys::time(), 0.0);
            for _ in 0..60 {
                sys::test::advance(1.0 / 60.0);
            }
            assert!((sys::time() - 1.0).abs() < 1e-6);
            sys::test::set_time_lost(0.25);
            assert_eq!(sys::time_lost(), 0.25);
            sys::test::advance(0.1);
            assert_eq!(sys::time_lost(), 0.0, "a frame without a hitch");
            // Each test thread has its own clock.
            let other = std::thread::spawn(|| {
                sys::test::advance(3.0);
                sys::time()
            });
            assert_eq!(other.join().unwrap(), 3.0);
            assert!((sys::time() - 1.1).abs() < 1e-6);
        }

        #[test]
        fn debug_params_are_absent_unless_a_test_sets_them() {
            sys::test::reset();
            assert_eq!(sys::param("zone"), None);
            sys::test::set_param("zone", "3");
            let long = "x".repeat(300);
            sys::test::set_param("note", &long);
            assert_eq!(sys::param("zone").as_deref(), Some("3"));
            assert_eq!(sys::param_as::<usize>("zone"), Some(3));
            assert_eq!(sys::param_as::<f32>("note"), None, "doesn't parse");
            assert_eq!(sys::param("note"), Some(long), "longer than the first buffer");
            sys::test::reset();
            assert_eq!(sys::param("zone"), None);
        }

        #[test]
        fn games_see_time_including_this_frames_dt() {
            struct Beats {
                next: f64,
                heard: Vec<f64>,
            }
            impl crate::Game for Beats {
                fn init() -> Self {
                    Beats { next: 0.5, heard: Vec::new() }
                }
                fn update(&mut self, _dt: f32) {
                    // Schedule on a half-second grid from the game clock, as a rhythm game does.
                    while sys::time() >= self.next {
                        self.heard.push(self.next);
                        self.next += 0.5;
                    }
                }
                fn render(&self) {}
            }
            sys::test::reset();
            let mut g = <Beats as crate::Game>::init();
            for _ in 0..240 {
                sys::test::update(&mut g, 1.0 / 120.0);
            }
            assert_eq!(g.heard, [0.5, 1.0, 1.5, 2.0], "every beat, not just the first");
        }
    }
}
