//! Guest SDK for Maimbrain games (docs/SPEC.md).
//!
//! Implement [`Game`] and export it with [`export_game!`]:
//!
//! ```ignore
//! use maimbrain::{Game, gfx2d, input};
//!
//! struct Hello { color: u32 }
//!
//! impl Game for Hello {
//!     fn init() -> Self { Hello { color: 0x1a1a2eff } }
//!     fn update(&mut self, _dt: f32) {
//!         for e in input::poll() {
//!             if e.kind() == input::Kind::TouchDown { self.color ^= 0xffffff00; }
//!         }
//!     }
//!     fn render(&self) { gfx2d::clear(self.color); }
//! }
//!
//! maimbrain::export_game!(Hello);
//! ```
//!
//! Everything nondeterministic (time, input, the random seed) comes from the
//! host and is recorded, so runs replay exactly (SPEC §3). Use [`Rng`] seeded
//! from [`sys::rand_seed`] rather than any other entropy source.
//!
//! Optional Cargo features: `physics2d` and `physics3d` add deterministic
//! rigid-body physics (Rapier) as [`physics2d`] / [`physics3d`]; see
//! docs/PHYSICS.md.

pub mod audio;
#[cfg(not(target_arch = "wasm32"))]
pub mod bot;
pub mod debug;
pub mod foley;
pub mod gfx2d;
pub mod gfx3d;
pub mod ground;
pub mod input;
pub mod juice;
pub mod motion;
pub mod music;
#[cfg(feature = "physics2d")]
pub mod physics2d;
#[cfg(feature = "physics3d")]
pub mod physics3d;
pub mod progress;
pub mod rounds;
pub mod sensors;
pub mod store;
pub mod sys;
pub mod ui;
pub mod walk;

#[doc(hidden)]
pub mod ffi;

#[cfg(any(feature = "physics2d", feature = "physics3d"))]
mod physics_clock;
mod rng;

pub use rng::Rng;

/// A game's lifecycle. The host owns the loop (SPEC §3).
pub trait Game: Sized + 'static {
    /// Called once. Must return within 500 ms; the first frame follows.
    fn init() -> Self;
    /// Advance the simulation. `dt` is seconds since the last update, ≤ 0.1.
    fn update(&mut self, dt: f32);
    /// Issue draw commands only; don't change game state here.
    fn render(&self);
    /// The game is going off screen. Persist anything important.
    fn suspend(&mut self) {}
    /// The game is back. Don't assume time continuity.
    fn resume(&mut self) {}
}

/// This SDK's version: the creator kit's (crates/mb-cli), which a test keeps
/// it equal to. Every game records it (see [`export_game!`]).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Exports `mb_init`, `mb_update`, `mb_render`, `mb_suspend`, `mb_resume` and
/// `mb_alloc` for a [`Game`] type, and records the SDK's [`VERSION`] in the
/// wasm's `maimbrain-sdk` custom section (SPEC §3), which the About sheet shows.
#[macro_export]
macro_rules! export_game {
    ($game:ty) => {
        static __MB_GAME: $crate::__private::Slot<$game> = $crate::__private::Slot::new();

        // In the game's own crate, so the linker always keeps it. Only in the
        // wasm: a native build (tests) has no such section, and Mach-O rejects the name.
        #[cfg(target_arch = "wasm32")]
        #[used]
        #[unsafe(link_section = "maimbrain-sdk")]
        static __MB_SDK: [u8; $crate::VERSION.len()] = $crate::__private::version_bytes();

        #[unsafe(no_mangle)]
        pub extern "C" fn mb_init() {
            $crate::__private::install_panic_hook();
            __MB_GAME.set(<$game as $crate::Game>::init());
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn mb_update(dt: f32) {
            $crate::Game::update(__MB_GAME.get(), dt);
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn mb_render() {
            $crate::ground::__begin_frame();
            $crate::Game::render(__MB_GAME.get());
            $crate::ground::__end_frame();
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn mb_suspend() {
            $crate::Game::suspend(__MB_GAME.get());
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn mb_resume() {
            $crate::Game::resume(__MB_GAME.get());
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn mb_alloc(len: u32) -> u32 {
            $crate::__private::alloc(len)
        }
    };
}

#[doc(hidden)]
pub mod __private {
    use std::cell::UnsafeCell;

    /// Holds the game instance. Guests are single-threaded (SPEC §3).
    pub struct Slot<T>(UnsafeCell<Option<T>>);
    unsafe impl<T> Sync for Slot<T> {}

    impl<T> Default for Slot<T> {
        fn default() -> Self {
            Self::new()
        }
    }

    impl<T> Slot<T> {
        pub const fn new() -> Self {
            Slot(UnsafeCell::new(None))
        }
        pub fn set(&self, v: T) {
            unsafe { *self.0.get() = Some(v) }
        }
        #[allow(clippy::mut_from_ref)]
        pub fn get(&self) -> &mut T {
            unsafe { (*self.0.get()).as_mut().expect("mb_init has not run") }
        }
    }

    pub fn install_panic_hook() {
        std::panic::set_hook(Box::new(|info| {
            crate::sys::log(crate::sys::Level::Error, &format!("panic: {info}"));
        }));
    }

    /// [`crate::VERSION`] as an array, for the `maimbrain-sdk` custom section.
    pub const fn version_bytes<const N: usize>() -> [u8; N] {
        let v = crate::VERSION.as_bytes();
        let mut out = [0; N];
        let mut i = 0;
        while i < N {
            out[i] = v[i];
            i += 1;
        }
        out
    }

    /// Hands the host a buffer it can fill; the guest takes ownership.
    pub fn alloc(len: u32) -> u32 {
        let mut v = Vec::<u8>::with_capacity(len as usize);
        let p = v.as_mut_ptr();
        std::mem::forget(v);
        p as u32
    }
}
