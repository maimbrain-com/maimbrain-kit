//! Lighting a textured hero: a generated model (the dragon from `mb model`,
//! 512² textures) lit with `Lighting::hero()` (a clear day sky as the fill,
//! a warm key light over the camera's shoulder, shadows) and
//! `MaterialStyle::hero()` (smooth shading, a rim, a thin outline), at full
//! resolution. `--param scene=hero`.
//!
//! `--param look=muddy` is the same model the way a game shouldn't light it,
//! and how Bolt Brawl first looked: a dark backlit sky, the model darkened,
//! three toon bands with a thick outline, and the retro look at 640×448 with
//! 6-bit color and fog bands. `mb shot --readability` passes the first and
//! warns about the second (the dragon is its `mark_subject`).
//!
//! `--param view=grazing` lowers the camera to look along a steel deck
//! (`assets/deck_plate.jpg` on a lit floor) at the dragon side-on, where
//! textures seen at a slant go soft without anisotropic filtering; add
//! `sharpen=0.4` (any amount 0–1) for `Post::sharpen` over the hero look.

use maimbrain::gfx2d::{self, Font};
use maimbrain::gfx3d::{
    self, Atmosphere, Camera, Fog, FogMode, Lighting, Material, MaterialDesc, MaterialStyle, MeshData, Model, Node, Post, Quat,
    Retro, Sky, Sun, Texture, Transform, vec3,
};
use maimbrain::sys::{self, Asset, AssetState};

pub struct Hero {
    asset: Asset,
    dragon: Option<Node>,
    muddy: bool,
    /// The deck texture while it loads (`view=grazing` only).
    deck: Option<Asset>,
    t: f32,
}

impl Hero {
    pub fn new() -> Hero {
        let muddy = sys::param("look").as_deref() == Some("muddy");
        let grazing = sys::param("view").as_deref() == Some("grazing");
        if !grazing {
            let ground = if muddy { [0.06, 0.05, 0.055, 1.0] } else { [0.42, 0.45, 0.4, 1.0] };
            let mat = Material::new(&MaterialDesc { base_color: ground, roughness: 0.9, ..Default::default() }).expect("material");
            let floor = Node::with_mesh(gfx3d::Mesh::new(&MeshData::plane(80.0, 80.0)).expect("mesh"), mat).expect("node");
            floor.set_transform(&Transform::default());
        }
        if muddy {
            gfx3d::sky(&Sky { colors: [[0.03, 0.02, 0.035], [0.09, 0.04, 0.05], [0.14, 0.05, 0.03]], nebula_density: 0.6, star_density: 0.0, ..Default::default() });
            gfx3d::atmosphere(&Atmosphere::default());
            // The sun low in front of the camera: the dragon's lit side faces away.
            gfx3d::sun(&Sun { direction: vec3(-0.25, -0.35, 0.9), color: [1.0, 0.6, 0.35], intensity: 2.5, shadows: 1, shadow_distance: 25.0 });
            gfx3d::post(&Post { exposure: 1.0, bloom_strength: 0.4, vignette: 0.4, ..Default::default() });
            gfx3d::fog(&Fog { color: [0.02, 0.012, 0.015], mode: FogMode::Linear, start: 6.0, end: 40.0, sky: 0.0, ..Default::default() });
            gfx3d::retro(&Retro { resolution: (640.0, 448.0), color_bits: 6.0, dither: 0.6, vertex_snap: 0.0, fog_bands: 8.0, ..Retro::default() });
        } else {
            gfx3d::sky(&Sky { nebula_density: 0.0, star_density: 0.0, ..Default::default() });
            let lighting = Lighting::hero().key_from(Hero::camera_at(0.0).rot.forward());
            gfx3d::lighting(&lighting);
            if let Some(amount) = sys::param("sharpen").and_then(|s| s.parse::<f32>().ok()) {
                gfx3d::post(&Post { sharpen: amount, ..lighting.post });
            }
        }
        let deck = grazing.then(|| Asset::load("assets/deck_plate.jpg"));
        Hero { asset: Asset::load("assets/dragon.glb"), dragon: None, muddy, deck, t: 0.0 }
    }

    pub fn update(&mut self, dt: f32) {
        self.t += dt;
        if let Some(a) = self.deck
            && a.state() == AssetState::Ready
        {
            self.deck = None;
            // Deck plates 2 m square to 40 m out: the far ones are seen at a few degrees.
            let tex = Texture::new(a);
            let mat = Material::new(&MaterialDesc { base_color: [0.9, 0.9, 0.9, 1.0], base_tex: tex, roughness: 0.7, metallic: 0.3, ..Default::default() }).expect("material");
            let floor = Node::with_mesh(gfx3d::Mesh::new(&MeshData::mode7_floor(40.0, 2.0, 16)).expect("mesh"), mat).expect("node");
            floor.set_transform(&Transform::default());
        }
        if self.dragon.is_none() && self.asset.state() == AssetState::Ready {
            let Some(model) = Model::load(self.asset) else { return };
            let style = if self.muddy {
                model.tint([0.45, 0.42, 0.48]);
                MaterialStyle { outline_width: 2.5, toon_bands: 3, ..MaterialStyle::default() }
            } else {
                MaterialStyle::hero()
            };
            for m in model.materials() {
                m.set_style(&style);
            }
            self.dragon = model.spawn(None);
            if let Some(d) = self.dragon {
                d.mark_subject();
            }
        }
        if let Some(d) = self.dragon {
            let t = self.t;
            d.set_transform(&Transform { pos: vec3(0.0, 1.3 + 0.12 * (t * 1.4).sin(), 0.0), rot: Quat::from_rotation_y(3.0 + 0.25 * (t * 0.3).sin()), ..Default::default() });
        }
    }

    fn camera_at(t: f32) -> Camera {
        if sys::param("view").as_deref() == Some("grazing") {
            // Low and to the dragon's side, looking along the deck past it.
            let mut c = Camera::looking_at(vec3(7.0, 0.9, 3.5), vec3(-1.0, 1.0, -1.0));
            c.fov_y = 0.8;
            c.far = 200.0;
            return c;
        }
        let a = 0.5 + 0.15 * (t * 0.2).sin();
        let pos = vec3(a.sin() * 9.5, 2.6, a.cos() * 9.5);
        let mut c = Camera::looking_at(pos, vec3(0.0, 1.3, 0.0));
        c.fov_y = 0.8;
        c.far = 200.0;
        c
    }

    pub fn render(&self) {
        gfx3d::camera(&Hero::camera_at(self.t));
        gfx3d::render();
        let (title, sub) = if self.muddy {
            ("muddy", "backlit · dark · toon 3 · retro 6-bit")
        } else {
            ("hero", "Lighting::hero() · MaterialStyle::hero()")
        };
        gfx2d::text_centered(Font::SansBold, 22.0, 180.0, 40.0, 0xffffffff, title);
        gfx2d::text_centered(Font::Sans, 11.0, 180.0, 66.0, 0xffffffcc, sub);
    }
}
