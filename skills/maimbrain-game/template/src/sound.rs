//! Sound from the sim's cues: which file each moment plays, how loud and at
//! what pitch, all in one place. Output only (nothing reads it back), so it
//! can't change a replay.
//!
//! In tests it also writes down every sound it plays, in the skill's
//! `scripts/mix_check.py` format, so the mix can be measured without ears:
//! bot.rs's `mix_log` plays a bot round through it.
//!
//!     MB_MIX_LOG=/tmp/mix.log cargo test -p NAME mix_log
//!     python3 <skill dir>/scripts/mix_check.py games/NAME/assets /tmp/mix.log
//!
//! The files are placeholders from `tools/sounds.py`: replace them with the
//! game's own (that script, or `mb sfx`) and add a cue and a file for each new
//! moment. Keep the recorder as you go.

use maimbrain::audio::Sound as Snd;
use maimbrain::sys::{Asset, AssetState};

use crate::sim::Cue;

/// assets/<name>.ogg, by index.
const FILES: [&str; 3] = ["pop", "miss", "over"];
const POP: usize = 0;
const MISS: usize = 1;
const OVER: usize = 2;

/// Levels. Files come out of sfx.py and `mb sfx` at the same loudness, so
/// these set the balance: the frequent pop sits lowest, the end of the round
/// highest. With music, set them against it with mix_check.py.
const VOL_POP: f32 = 0.28;
const VOL_MISS: f32 = 0.4;
const VOL_OVER: f32 = 0.8;
/// Each pop climbs the major scale, starting over every five, so a run of
/// pops sounds like progress instead of one sound repeated (within ±7
/// semitones: further, a sample sounds chipmunked or sludgy).
const LADDER: [f32; 5] = [0.0, 2.0, 4.0, 5.0, 7.0];

pub struct Sound {
    assets: Vec<Asset>,
    sounds: Vec<Option<Snd>>,
    /// Seconds since this was made (the test log's clock).
    clock: f32,
    /// Tests: every sound played, a line each: `<seconds> <file> <vol> <pitch>`.
    #[cfg(test)]
    pub log: Vec<String>,
}

impl Sound {
    pub fn new() -> Sound {
        Sound {
            assets: FILES.iter().map(|f| Asset::load(&format!("assets/{f}.ogg"))).collect(),
            sounds: vec![None; FILES.len()],
            clock: 0.0,
            #[cfg(test)]
            log: Vec::new(),
        }
    }

    /// Every frame: picks up sounds as they finish loading.
    pub fn update(&mut self, dt: f32) {
        self.clock += dt;
        for (s, a) in self.sounds.iter_mut().zip(&self.assets) {
            if s.is_none() && a.state() == AssetState::Ready {
                *s = Snd::new(*a);
            }
        }
    }

    pub fn cue(&mut self, cue: Cue) {
        match cue {
            Cue::Pop { score } => {
                let step = LADDER[(score as usize + LADDER.len() - 1) % LADDER.len()];
                self.play(POP, VOL_POP, (step / 12.0).exp2());
            }
            Cue::Miss => self.play(MISS, VOL_MISS, 1.0),
            Cue::Over => self.play(OVER, VOL_OVER, 1.0),
        }
    }

    fn play(&mut self, i: usize, vol: f32, pitch: f32) {
        #[cfg(test)]
        self.log.push(format!("{:.4} {} {vol:.4} {pitch:.4}", self.clock, FILES[i]));
        if let Some(s) = self.sounds[i] {
            s.play(vol, 0.0, pitch);
        }
    }
}
