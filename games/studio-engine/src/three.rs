//! The engine's 3D games (`kind = "3d"`, the `3d` build):
//! what differs from the Arcade's loop. The round flow, input, debug menu,
//! scenarios and `mb play`'s hooks are the engine's own (lib.rs); here are
//! the cards' look, the project's models and sounds for the bindings
//! (scene.rs), the worlds that run off-stage, and drawing the scene.
//!
//! Off-stage, unlike the Arcade: the title card plays the game behind it
//! (its VM has `title` true, an attract mode), and after a round ends its
//! world keeps stepping with no input, so things settle under the card.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use maimbrain::audio::Sound;
use maimbrain::gfx2d::{self, Font};
use maimbrain::gfx3d;
use maimbrain::rounds::{Rounds, Stage};
use maimbrain::sys::Asset;
use maimbrain::ui::{Theme, TitleCard};

use crate::project::Project;
use crate::scene::{Audio, Hud};
use crate::vm::{FrameInput, Media};
use crate::{Studio, error_box, input, logical_size};

impl Studio {
    /// The cards, input units, and the models and sounds the bindings play.
    pub(crate) fn setup_3d(&mut self, p: &Project) {
        let mut title = TitleCard::new(&p.title.to_uppercase(), Theme::night());
        if let Some(t) = &p.tagline {
            title = title.tagline(t);
        }
        let mut rounds = Rounds::new(title).heading(&p.over.to_uppercase()).label(&p.label.to_uppercase());
        if p.lower_is_better {
            rounds = rounds.lower_is_better();
        }
        self.rounds = rounds;
        self.input.swipe_min = input::SWIPE_MIN_3D;
        let asset = |path: &str| self.asset_of(path);
        let models: BTreeMap<String, Asset> = p.models.iter().filter_map(|m| Some((m.clone(), asset(&format!("models/{m}.glb"))?))).collect();
        let mut audio = Audio::default();
        for (dir, names) in [("sounds", &p.sounds), ("music", &p.music)] {
            for n in names {
                if let Some(snd) = asset(&format!("{dir}/{n}.ogg")).and_then(sound_of) {
                    audio.sounds.insert(n.clone(), snd);
                }
            }
        }
        self.media = Media { models: Rc::new(models), audio: Rc::new(RefCell::new(audio)) };
    }

    /// A loaded file's asset. Natively nothing loads (SPEC §9.8), and every
    /// host call answers 1, so a project's files are handle 1.
    fn asset_of(&self, path: &str) -> Option<Asset> {
        #[cfg(target_arch = "wasm32")]
        return self.assets.get(path).copied();
        #[cfg(not(target_arch = "wasm32"))]
        return Some(self.assets.get(path).copied().unwrap_or(Asset(1)));
    }

    /// Between rounds: the title's attract mode, or the ended round's world
    /// settling under the card, with no input.
    pub(crate) fn step_3d_offstage(&mut self, dt: f32) {
        if self.error.is_some() {
            return;
        }
        let v = match self.rounds.stage() {
            Stage::Title => self.title.as_mut(),
            _ => self.round.as_mut(),
        };
        if let Some(v) = v
            && let Err(msg) = v.step(dt, &FrameInput::default())
        {
            self.fail(msg);
        }
    }

    /// The scene, the 2D the game drew over it this frame, then the cards.
    pub(crate) fn render_3d(&self, p: &Project) {
        let live = match self.rounds.stage() {
            Stage::Title => self.title.as_ref(),
            _ => self.round.as_ref(),
        };
        match live.and_then(|v| v.scene.as_ref()) {
            Some(sc) => {
                let sc = sc.borrow();
                if let Some(c) = &sc.camera {
                    gfx3d::camera(c);
                }
                gfx3d::render();
                for h in &sc.hud {
                    draw_hud(h);
                }
            }
            None => gfx2d::clear(0x0b0d1fff),
        }
        if let Some(e) = &self.error {
            error_box(e, logical_size(p.orientation).0 as f32);
        }
        self.rounds.draw();
    }
}

fn sound_of(a: Asset) -> Option<Sound> {
    #[cfg(target_arch = "wasm32")]
    return Sound::new(a);
    #[cfg(not(target_arch = "wasm32"))]
    return Some(Sound(a.0));
}

/// Text (Inter Bold, with a soft shadow unless asked not to) and boxes, in logical units.
fn draw_hud(h: &Hud) {
    match h {
        Hud::Text { s, x, y, size, color, align, shadow } => {
            let draw = |dx: f32, c: u32| match align {
                0 => gfx2d::text_centered(Font::SansBold, *size, x + dx, y + dx, c, s),
                -1 => gfx2d::text(Font::SansBold, *size, x + dx, y + dx, c, s),
                _ => gfx2d::text(Font::SansBold, *size, x + dx - gfx2d::measure(Font::SansBold, *size, s), y + dx, c, s),
            };
            if *shadow {
                draw(1.5, 0x00000090);
            }
            draw(0.0, *color);
        }
        Hud::Rect { x, y, w, h, color } => gfx2d::rect(*x, *y, *w, *h, *color),
    }
}
