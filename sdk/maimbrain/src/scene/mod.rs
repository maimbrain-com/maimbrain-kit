//! A retained 2D scene (docs/STUDIO-2.md §4.1–4.3): nodes that the game
//! makes once and changes by property, which Rust keeps, moves and draws
//! through [`gfx2d`] each frame. Studio engine 2.0's nodes (`Sprite`,
//! `Shape`, `Text`, `Layer`, `Group`) are this module seen from Lua, and a
//! Rust game can use it directly (Cargo feature `scene`).
//!
//! ```ignore
//! use maimbrain::scene::{Scene, Kind, View};
//! let mut scene = Scene::new(360.0, 640.0);
//! let hero = scene.add(Kind::Sprite, None).unwrap();
//! scene.node_mut(hero).unwrap().x = 180.0;
//! // update: scene.step(dt); render: scene.draw(&view, None);
//! ```
//!
//! - **Space.** Logical units, `x` right and `y` down. A node's `x`, `y`,
//!   `rotation` and scale are in its parent's space; a node without a parent
//!   is in the world.
//! - **The camera** ([`Camera`]) looks at the world: its `x`, `y` is the
//!   view's centre, `scale` zooms, `rot` turns. A `fixed` node (and
//!   everything under it) ignores it: the HUD. A [`Kind::Layer`] scrolls by
//!   its `parallax` share of the camera's travel.
//! - **Order.** Nodes are drawn by their effective order (their own `order`
//!   plus their parent's, as Godot's relative z), then in tree order (a
//!   parent before its children, siblings as made). [`Scene::draw`] draws
//!   nodes at order 0 or less, then the immediate layer it's given (Studio's
//!   `love.draw`, a [`DrawList`]), then the nodes above 0.
//! - **Culling.** A node whose bounds are off the view isn't drawn.
//! - **Motion.** [`Scene::step`] moves every node by its velocity and
//!   gravity (`vx`, `vy`, `gravity`) and turns it by its spin (`vr`), in
//!   Rust, each update. A spun `rotation` stays in −π..π.
//!
//! Ids carry a generation, so an id kept after its node is destroyed never
//! reaches the node that reuses its slot.
//!
//! [`three`] is the scene's 3D side: nodes that stand for mb3d's, sent to
//! the host once an update.

pub mod draw;
pub mod emitter;
pub mod three;
pub mod tilemap;
pub mod walker;

pub use draw::{Cmd, DrawList};
pub use emitter::{Emitter, Preset};
pub use tilemap::{Tile, TileMap};
pub use walker::WalkerNode;

use crate::gfx2d::{self, Font, Image};

/// Nodes at once (mb3d's cap too; docs/STUDIO-2.md §4.6).
pub const MAX_NODES: usize = 4096;

/// A spun angle, kept in −π..π so it never grows past f32's precision
/// (an angle already in range is left exactly as it is).
pub fn wrap_angle(a: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    if (-PI..=PI).contains(&a) { a } else { (a + PI).rem_euclid(TAU) - PI }
}

/// A node: its slot and generation (`Scene::add`'s result). 0 is never an
/// id, and every id fits in an `i32` (a Lua array index, say).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(u32);

impl NodeId {
    fn new(slot: usize, gen_: u32) -> NodeId {
        NodeId((gen_ << 12) | slot as u32)
    }
    fn slot(self) -> usize {
        (self.0 & 0xfff) as usize
    }
    /// The id as a number (what a handle stores), and back.
    pub fn to_bits(self) -> u32 {
        self.0
    }
    pub fn from_bits(b: u32) -> NodeId {
        NodeId(b)
    }
}

/// What a node is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// An empty node, to move things together.
    Group,
    /// An image, or one frame of a sheet.
    Sprite,
    /// A rectangle, circle, polygon or line in the node's `color`.
    Shape,
    /// Text in a font.
    Text,
    /// A picture that scrolls by a parallax factor and can wrap.
    Layer,
    /// A grid of tiles from one tile set ([`TileMap`]), its top left at its place.
    Tilemap,
    /// A side-view figure whose feet plant as it walks ([`WalkerNode`]); its place is its feet.
    Walker,
    /// Particles ([`Emitter`]), emitted at its place and living in the world.
    Emitter,
    /// A button, panel, pill, meter or icon ([`crate::ui::Widget`]), drawn
    /// centred on its place in the scene's [`theme`](Scene::theme).
    Widget,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Group => "Group",
            Kind::Sprite => "Sprite",
            Kind::Shape => "Shape",
            Kind::Text => "Text",
            Kind::Layer => "Layer",
            Kind::Tilemap => "Tilemap",
            Kind::Walker => "Walker",
            Kind::Emitter => "Emitter",
            Kind::Widget => "Widget",
        }
    }
}

/// A [`Kind::Shape`]'s outline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ShapeKind {
    #[default]
    Rect,
    Circle,
    /// `points` around the node's position.
    Poly,
    /// `points` as a line through them.
    Line,
}

/// How [`Kind::Text`] sits on its `x`: its left edge, centre or right edge.
pub use crate::ui::Align;

/// An image the scene draws: registered once with [`Scene::add_image`]. Its
/// handle can arrive later (assets load asynchronously); until then sprites
/// of it draw as a faint box of their size.
#[derive(Clone, Debug, Default)]
pub struct SceneImage {
    pub image: Option<Image>,
    /// The picture's size in logical units.
    pub w: f32,
    pub h: f32,
    /// Texels per logical unit in the image file (1 for most; more for art
    /// the packer scaled up to stay crisp).
    pub texels: f32,
    /// One frame of a sheet (the whole image when there's no sheet).
    pub frame_w: f32,
    pub frame_h: f32,
    /// Where the picture starts in its image file, in texels: (0, 0) unless
    /// it's packed into an atlas with others.
    pub origin: [f32; 2],
}

impl SceneImage {
    pub fn new(w: f32, h: f32) -> SceneImage {
        SceneImage { image: None, w, h, texels: 1.0, frame_w: w, frame_h: h, origin: [0.0, 0.0] }
    }

    /// Frames in the sheet, left to right, then top to bottom.
    pub fn frames(&self) -> u32 {
        let cols = (self.w / self.frame_w.max(1.0)).floor().max(1.0) as u32;
        let rows = (self.h / self.frame_h.max(1.0)).floor().max(1.0) as u32;
        cols * rows
    }

    /// Frame `n` (from 1) as a source rectangle in texels.
    pub fn frame_src(&self, n: u32) -> [f32; 4] {
        let cols = (self.w / self.frame_w.max(1.0)).floor().max(1.0) as u32;
        let i = n.saturating_sub(1) % self.frames();
        let t = self.texels;
        [self.origin[0] + (i % cols) as f32 * self.frame_w * t, self.origin[1] + (i / cols) as f32 * self.frame_h * t, self.frame_w * t, self.frame_h * t]
    }

    /// The whole picture as a source rectangle in texels.
    pub fn src(&self) -> [f32; 4] {
        [self.origin[0], self.origin[1], self.w * self.texels, self.h * self.texels]
    }

    /// A rectangle of the picture (in its own pixels) as texels in its file.
    pub fn sub(&self, r: [f32; 4]) -> [f32; 4] {
        let t = self.texels;
        [self.origin[0] + r[0] * t, self.origin[1] + r[1] * t, r[2] * t, r[3] * t]
    }
}

/// An image's index in the scene ([`Scene::add_image`]).
pub type ImageId = u16;

/// One node: the shared properties, then each kind's own. Fields are public:
/// the binding that changes one checks the value first.
#[derive(Clone, Debug)]
pub struct Node {
    pub kind: Kind,
    pub x: f32,
    pub y: f32,
    /// Radians, clockwise on screen.
    pub rotation: f32,
    pub sx: f32,
    pub sy: f32,
    /// Units a second, and units a second² on `vy`.
    pub vx: f32,
    pub vy: f32,
    pub gravity: f32,
    /// Radians a second onto `rotation` (a spin).
    pub vr: f32,
    /// Something else moves it (a behaviour, with tile collision): [`Scene::step`]
    /// leaves its velocity and gravity alone (it still spins by `vr`).
    pub driven: bool,
    /// 0–1, multiplied down the tree.
    pub alpha: f32,
    pub visible: bool,
    /// 0xRRGGBBAA: a sprite's or text's tint, a shape's fill.
    pub color: u32,
    pub order: f32,
    /// Ignores the camera (the HUD).
    pub fixed: bool,
    pub name: String,
    pub tag: String,
    /// A Shape's size (a Sprite's comes from its frame and scale).
    pub w: f32,
    pub h: f32,
    pub radius: f32,

    // Sprite and Layer
    pub image: Option<ImageId>,
    /// From 1.
    pub frame: u32,
    pub anchor_x: f32,
    pub anchor_y: f32,
    pub flip_x: bool,
    pub flip_y: bool,

    // Shape
    pub shape: ShapeKind,
    /// Around the node's position: x1, y1, x2, y2, …
    pub points: Vec<f32>,
    pub corner: f32,
    /// A stroke this wide; 0 fills.
    pub line: f32,

    // Text
    pub text: String,
    pub font: Font,
    pub size: f32,
    pub align: Align,
    /// Wraps at this width (0: no wrapping).
    pub width: f32,

    // Layer
    pub parallax: f32,
    pub repeat_x: bool,
    pub repeat_y: bool,
    /// Painted below the layer's picture to the bottom of the view.
    pub fill: Option<u32>,

    pub tilemap: Option<Box<TileMap>>,
    pub walker: Option<Box<WalkerNode>>,
    pub emitter: Option<Box<Emitter>>,
    pub widget: Option<Box<crate::ui::Widget>>,

    parent: Option<NodeId>,
    children: Vec<NodeId>,
}

impl Node {
    fn new(kind: Kind) -> Node {
        Node {
            kind,
            x: 0.0,
            y: 0.0,
            rotation: 0.0,
            sx: 1.0,
            sy: 1.0,
            vx: 0.0,
            vy: 0.0,
            gravity: 0.0,
            vr: 0.0,
            driven: false,
            alpha: 1.0,
            visible: true,
            color: 0xffffffff,
            order: 0.0,
            fixed: false,
            name: String::new(),
            tag: String::new(),
            w: if kind == Kind::Shape { 32.0 } else { 0.0 },
            h: if kind == Kind::Shape { 32.0 } else { 0.0 },
            radius: if kind == Kind::Shape { 16.0 } else { 0.0 },
            image: None,
            frame: 1,
            anchor_x: 0.5,
            anchor_y: 0.5,
            flip_x: false,
            flip_y: false,
            shape: ShapeKind::Rect,
            points: Vec::new(),
            corner: 0.0,
            line: 0.0,
            text: String::new(),
            font: Font::Sans,
            size: 16.0,
            align: Align::Left,
            width: 0.0,
            parallax: 1.0,
            repeat_x: false,
            repeat_y: false,
            fill: None,
            tilemap: if kind == Kind::Tilemap { Some(Box::default()) } else { None },
            walker: if kind == Kind::Walker { Some(Box::new(WalkerNode::new(60.0))) } else { None },
            emitter: None,
            widget: None,
            parent: None,
            children: Vec::new(),
        }
    }

    pub fn parent(&self) -> Option<NodeId> {
        self.parent
    }

    pub fn children(&self) -> &[NodeId] {
        &self.children
    }
}

struct Slot {
    gen_: u32,
    node: Option<Node>,
    /// Made-order, for drawing siblings as made.
    seq: u64,
}

/// The 2D camera (hump.camera's model): it looks at `x`, `y` (the view's
/// centre), zoomed by `scale` and turned by `rot`; `shake` offsets it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub rot: f32,
    pub shake_x: f32,
    pub shake_y: f32,
    /// Positions snap to whole units as they're drawn (the Arcade preset's pixels).
    pub snap: bool,
}

impl Camera {
    /// Looking at the middle of a view `w` × `h`: world and screen agree.
    pub fn new(w: f32, h: f32) -> Camera {
        Camera { x: w / 2.0, y: h / 2.0, scale: 1.0, rot: 0.0, shake_x: 0.0, shake_y: 0.0, snap: false }
    }

    /// A world point on the view (logical units from its top left).
    pub fn to_screen(&self, w: f32, h: f32, x: f32, y: f32) -> (f32, f32) {
        let (dx, dy) = (x - self.x - self.shake_x, y - self.y - self.shake_y);
        let (s, c) = (-self.rot).sin_cos();
        let (rx, ry) = (dx * c - dy * s, dx * s + dy * c);
        (rx * self.scale + w / 2.0, ry * self.scale + h / 2.0)
    }

    /// A point on the view in the world.
    pub fn to_world(&self, w: f32, h: f32, sx: f32, sy: f32) -> (f32, f32) {
        let k = if self.scale.abs() > 1e-6 { self.scale } else { 1e-6 };
        let (dx, dy) = ((sx - w / 2.0) / k, (sy - h / 2.0) / k);
        let (s, c) = self.rot.sin_cos();
        (dx * c - dy * s + self.x + self.shake_x, dx * s + dy * c + self.y + self.shake_y)
    }

    /// gfx2d's transform for the world, inside a view `w` × `h`.
    pub fn apply(&self, w: f32, h: f32) {
        gfx2d::translate(w / 2.0, h / 2.0);
        gfx2d::scale(self.scale, self.scale);
        gfx2d::rotate(-self.rot);
        let (x, y) = (self.x + self.shake_x, self.y + self.shake_y);
        if self.snap {
            gfx2d::translate(-x.round(), -y.round());
        } else {
            gfx2d::translate(-x, -y);
        }
    }
}

/// Where the scene is drawn: the view's size in logical units (the game's
/// canvas), already placed on the screen by the caller's transform.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub w: f32,
    pub h: f32,
}

/// What the scene's frame did that the game may want to hear (a node's signals).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// A Walker's foot landed: which (0 the far one), and where.
    Stepped { node: NodeId, foot: usize, x: f32, y: f32 },
}

/// Every node, the images they draw and the camera.
pub struct Scene {
    slots: Vec<Slot>,
    free: Vec<usize>,
    live: usize,
    /// Widget nodes alive (input goes past them only when there are some).
    widgets: usize,
    seq: u64,
    images: Vec<SceneImage>,
    pub camera: Camera,
    /// The canvas: the view the camera starts on.
    pub width: f32,
    pub height: f32,
    /// The game's one-shot effects (`fx.burst`, `fx.popup`, `fx.flash`):
    /// drawn over the nodes, in the world (the flash on the whole view).
    pub fx: Emitter,
    pub popups: crate::juice::Popups,
    /// A flash: colour, its length, time left.
    pub flash: Option<(u32, f32, f32)>,
    /// Particle systems drawn from the immediate layer ([`Cmd::Particles`]):
    /// their owner updates them.
    pub systems: Vec<Emitter>,
    /// The identity widgets draw in (`Kind::Widget`).
    pub theme: crate::ui::Theme,
}

/// Why a node couldn't be made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Full;

impl Scene {
    pub fn new(width: f32, height: f32) -> Scene {
        Scene {
            slots: Vec::new(),
            free: Vec::new(),
            live: 0,
            widgets: 0,
            seq: 0,
            images: Vec::new(),
            camera: Camera::new(width, height),
            width,
            height,
            fx: Emitter::new(0x6678),
            popups: crate::juice::Popups::new(),
            flash: None,
            systems: Vec::new(),
            theme: crate::ui::Theme::preset("greybox"),
        }
    }

    /// Live nodes.
    pub fn len(&self) -> usize {
        self.live
    }

    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    /// Registers an image for sprites and layers.
    pub fn add_image(&mut self, img: SceneImage) -> ImageId {
        self.images.push(img);
        (self.images.len() - 1) as ImageId
    }

    pub fn image(&self, id: ImageId) -> Option<&SceneImage> {
        self.images.get(id as usize)
    }

    pub fn image_mut(&mut self, id: ImageId) -> Option<&mut SceneImage> {
        self.images.get_mut(id as usize)
    }

    /// A new node under `parent` (or the root), at its origin.
    pub fn add(&mut self, kind: Kind, parent: Option<NodeId>) -> Result<NodeId, Full> {
        let slot = match self.free.pop() {
            Some(s) => s,
            None if self.slots.len() < MAX_NODES => {
                self.slots.push(Slot { gen_: 0, node: None, seq: 0 });
                self.slots.len() - 1
            }
            None => return Err(Full),
        };
        let s = &mut self.slots[slot];
        s.gen_ = (s.gen_ + 1) & 0x7_ffff;
        if s.gen_ == 0 {
            s.gen_ = 1;
        }
        self.seq += 1;
        s.seq = self.seq;
        let mut node = Node::new(kind);
        if kind == Kind::Emitter {
            node.emitter = Some(Box::new(Emitter::new(self.seq ^ 0x454d_4954)));
        }
        s.node = Some(node);
        let id = NodeId::new(slot, s.gen_);
        self.live += 1;
        if kind == Kind::Widget {
            self.widgets += 1;
        }
        let parent = parent.filter(|p| self.get(*p).is_some());
        self.link(id, parent);
        Ok(id)
    }

    pub fn get(&self, id: NodeId) -> Option<&Node> {
        let s = self.slots.get(id.slot())?;
        if NodeId::new(id.slot(), s.gen_) != id {
            return None;
        }
        s.node.as_ref()
    }

    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        let s = self.slots.get_mut(id.slot())?;
        if NodeId::new(id.slot(), s.gen_) != id {
            return None;
        }
        s.node.as_mut()
    }

    /// When the node was made, against the others (larger: later).
    pub fn made_order(&self, id: NodeId) -> u64 {
        self.slots.get(id.slot()).filter(|s| s.gen_ == id.0 >> 12).map_or(u64::MAX, |s| s.seq)
    }

    pub fn alive(&self, id: NodeId) -> bool {
        self.get(id).is_some()
    }

    fn link(&mut self, id: NodeId, parent: Option<NodeId>) {
        if let Some(p) = parent
            && let Some(pn) = self.node_mut(p)
        {
            pn.children.push(id);
        }
        if let Some(n) = self.node_mut(id) {
            n.parent = parent;
        }
    }

    fn unlink(&mut self, id: NodeId) {
        let Some(p) = self.get(id).and_then(|n| n.parent) else { return };
        if let Some(pn) = self.node_mut(p) {
            pn.children.retain(|c| *c != id);
        }
        if let Some(n) = self.node_mut(id) {
            n.parent = None;
        }
    }

    /// Destroys a node and everything under it; returns them as they were
    /// (the node first).
    pub fn destroy(&mut self, id: NodeId) -> Vec<(NodeId, Node)> {
        let mut gone = Vec::new();
        if !self.alive(id) {
            return gone;
        }
        self.unlink(id);
        let mut stack = vec![id];
        while let Some(n) = stack.pop() {
            let slot = n.slot();
            if let Some(node) = self.slots[slot].node.take() {
                stack.extend(node.children.iter().rev());
                self.free.push(slot);
                self.live -= 1;
                if node.kind == Kind::Widget {
                    self.widgets -= 1;
                }
                gone.push((n, node));
            }
        }
        gone
    }

    /// Moves a node under `parent` (None: the root). With `keep`, it stays
    /// where it is on screen: its position, rotation and scale become the
    /// same world transform in the new parent's space. Refused (false) if
    /// `parent` is the node or under it.
    pub fn set_parent(&mut self, id: NodeId, parent: Option<NodeId>, keep: bool) -> bool {
        if !self.alive(id) || parent.is_some_and(|p| !self.alive(p)) {
            return false;
        }
        let mut up = parent;
        while let Some(p) = up {
            if p == id {
                return false;
            }
            up = self.get(p).and_then(|n| n.parent);
        }
        let world = self.world_transform(id);
        self.unlink(id);
        self.link(id, parent);
        if keep {
            let inv = parent.map_or(Affine::IDENTITY, |p| self.world_transform(p).inverse());
            let local = inv.mul(&world);
            let (x, y, r, sx, sy) = local.decompose();
            if let Some(n) = self.node_mut(id) {
                (n.x, n.y, n.rotation, n.sx, n.sy) = (x, y, r, sx, sy);
            }
        }
        true
    }

    /// The node's transform to the world (its parents' and its own).
    pub fn world_transform(&self, id: NodeId) -> Affine {
        let mut chain = Vec::new();
        let mut at = Some(id);
        while let Some(n) = at.and_then(|i| self.get(i)) {
            chain.push(n);
            at = n.parent;
        }
        chain.iter().rev().fold(Affine::IDENTITY, |m, n| m.mul(&Affine::trs(n.x, n.y, n.rotation, n.sx, n.sy)))
    }

    /// Where the node's origin is in the world.
    pub fn world_position(&self, id: NodeId) -> (f32, f32) {
        let m = self.world_transform(id);
        (m.e, m.f)
    }

    /// Whether the node or one above it is `fixed` (drawn without the camera).
    pub fn is_fixed(&self, id: NodeId) -> bool {
        let mut at = Some(id);
        while let Some(n) = at.and_then(|i| self.get(i)) {
            if n.fixed {
                return true;
            }
            at = n.parent;
        }
        false
    }

    /// Every live node's id, as made.
    pub fn ids(&self) -> Vec<NodeId> {
        let mut v: Vec<(u64, NodeId)> = self.slots.iter().enumerate().filter(|(_, s)| s.node.is_some()).map(|(i, s)| (s.seq, NodeId::new(i, s.gen_))).collect();
        v.sort();
        v.into_iter().map(|(_, id)| id).collect()
    }

    /// The first node made with this name.
    pub fn find(&self, name: &str) -> Option<NodeId> {
        self.ids().into_iter().find(|id| self.get(*id).is_some_and(|n| n.name == name))
    }

    /// A descendant of `id` with this name (depth first, children as made).
    pub fn find_under(&self, id: NodeId, name: &str) -> Option<NodeId> {
        for c in self.get(id)?.children.clone() {
            if self.get(c).is_some_and(|n| n.name == name) {
                return Some(c);
            }
            if let Some(f) = self.find_under(c, name) {
                return Some(f);
            }
        }
        None
    }

    /// Every node with this tag (`"*"`: every node), as made.
    pub fn all(&self, tag: &str) -> Vec<NodeId> {
        self.ids().into_iter().filter(|id| self.get(*id).is_some_and(|n| tag == "*" || n.tag == tag)).collect()
    }

    /// The node's drawn size in its own space: a sprite's frame (times
    /// nothing: scale is applied by the transform), a shape's box, a layer's
    /// image. Groups and text have none here.
    pub fn local_size(&self, id: NodeId) -> (f32, f32) {
        let Some(n) = self.get(id) else { return (0.0, 0.0) };
        match n.kind {
            Kind::Sprite | Kind::Layer => n.image.and_then(|i| self.image(i)).map_or((0.0, 0.0), |im| (im.frame_w, im.frame_h)),
            Kind::Shape => match n.shape {
                ShapeKind::Rect => (n.w, n.h),
                ShapeKind::Circle => (n.radius * 2.0, n.radius * 2.0),
                ShapeKind::Poly | ShapeKind::Line => {
                    let (x0, y0, x1, y1) = bounds(&n.points);
                    (2.0 * x0.abs().max(x1.abs()), 2.0 * y0.abs().max(y1.abs()))
                }
            },
            Kind::Text => {
                let (w, h) = crate::ui::text(&n.text).font(n.font).size(n.size).wrap(n.width).measure();
                (w, h)
            }
            Kind::Tilemap => n.tilemap.as_ref().map_or((0.0, 0.0), |m| m.size()),
            Kind::Walker => n.walker.as_ref().map_or((0.0, 0.0), |w| (w.body[2] * w.scale(), w.height)),
            Kind::Group | Kind::Emitter => (0.0, 0.0),
            Kind::Widget => n.widget.as_ref().map_or((0.0, 0.0), |w| w.size(&self.theme, n.w, n.h)),
        }
    }

    /// Whether any widget node is alive.
    pub fn has_widgets(&self) -> bool {
        self.widgets > 0
    }

    /// The widgets' own time (their presses, fills and focus rings): real
    /// time, so they move while the world is slowed or held.
    pub fn step_widgets(&mut self, dt: f32) {
        let th = self.theme;
        for s in self.slots.iter_mut() {
            if let Some(w) = s.node.as_mut().and_then(|n| n.widget.as_mut()) {
                w.update(dt, &th);
            }
        }
    }

    /// One update's motion: velocities, gravity and spins, in made order;
    /// then the walkers' feet and the emitters' particles follow where
    /// things are; then the one-shot effects. Returns what happened
    /// (footfalls).
    pub fn step(&mut self, dt: f32) -> Vec<Event> {
        for s in self.slots.iter_mut() {
            let Some(n) = s.node.as_mut() else { continue };
            if !n.driven && (n.vx != 0.0 || n.vy != 0.0 || n.gravity != 0.0) {
                n.vy += n.gravity * dt;
                n.x += n.vx * dt;
                n.y += n.vy * dt;
            }
            if n.vr != 0.0 {
                n.rotation = wrap_angle(n.rotation + n.vr * dt);
            }
        }
        let mut events = Vec::new();
        let moving: Vec<NodeId> = self.slots.iter().enumerate().filter(|(_, s)| s.node.as_ref().is_some_and(|n| matches!(n.kind, Kind::Walker | Kind::Emitter))).map(|(i, s)| NodeId::new(i, s.gen_)).collect();
        for id in moving {
            let (x, y) = self.world_position(id);
            let n = self.node_mut(id).unwrap();
            if let Some(w) = n.walker.as_mut() {
                for s in w.step(dt, x) {
                    events.push(Event::Stepped { node: id, foot: s.foot, x: s.x, y });
                }
            }
            if let Some(e) = n.emitter.as_mut() {
                e.step(dt, x, y);
            }
        }
        self.fx.step(dt, 0.0, 0.0);
        self.popups.update(dt);
        if let Some((c, len, left)) = self.flash {
            self.flash = (left > dt).then_some((c, len, left - dt));
        }
        events
    }

    /// What gets drawn and in what order: (node, effective order, effective
    /// alpha, fixed), visible ones only.
    pub fn draw_order(&self) -> Vec<(NodeId, f32, f32, bool)> {
        let mut out = Vec::with_capacity(self.live);
        let mut roots: Vec<NodeId> = self.ids().into_iter().filter(|id| self.get(*id).is_some_and(|n| n.parent.is_none())).collect();
        roots.reverse();
        // Depth first, children as made; each with its parent's sums.
        let mut stack: Vec<(NodeId, f32, f32, bool)> = roots.into_iter().map(|r| (r, 0.0, 1.0, false)).collect();
        while let Some((id, order, alpha, fixed)) = stack.pop() {
            let Some(n) = self.get(id) else { continue };
            if !n.visible {
                continue;
            }
            let (o, a, f) = (order + n.order, alpha * n.alpha.clamp(0.0, 1.0), fixed || n.fixed);
            out.push((id, o, a, f));
            for c in n.children.iter().rev() {
                stack.push((*c, o, a, f));
            }
        }
        // Stable: tree order within an order.
        out.sort_by(|a, b| a.1.total_cmp(&b.1));
        out
    }

    /// Draws the scene into a view of the canvas's size, with `immediate`
    /// between the nodes at order 0 or less and those above it. The caller
    /// has placed the view on the screen (gfx2d's transform).
    pub fn draw(&self, view: &View, immediate: Option<&DrawList>) {
        self.draw_filtered(view, immediate, true, &|_| true);
    }

    /// [`draw`](Scene::draw) with only the nodes `keep` takes (each as
    /// [`draw_order`](Scene::draw_order) lists it: node, effective order,
    /// alpha, fixed), and the one-shot effects and flash only if `effects`:
    /// a variant of the frame for a check (Studio engine 2.0's readability
    /// check draws it without the HUD, and its subject alone).
    pub fn draw_filtered(&self, view: &View, immediate: Option<&DrawList>, effects: bool, keep: &dyn Fn(&(NodeId, f32, f32, bool)) -> bool) {
        let list = self.draw_order();
        let split = list.partition_point(|e| e.1 <= 0.0);
        for e in list[..split].iter().filter(|e| keep(e)) {
            self.draw_node(view, *e);
        }
        if let Some(d) = immediate {
            d.replay(self, view);
        }
        for e in list[split..].iter().filter(|e| keep(e)) {
            self.draw_node(view, *e);
        }
        if !effects {
            return;
        }
        // The one-shot effects, over everything, in the world.
        if !self.fx.particles.is_empty() || !self.popups.list.is_empty() {
            gfx2d::push();
            self.camera.apply(view.w, view.h);
            self.fx.particles.draw();
            self.popups.draw();
            gfx2d::pop();
        }
        if let Some((c, len, left)) = self.flash {
            gfx2d::rect(0.0, 0.0, view.w, view.h, with_alpha(c, (left / len.max(1e-3)).clamp(0.0, 1.0)));
        }
    }

    fn draw_node(&self, view: &View, (id, _, alpha, fixed): (NodeId, f32, f32, bool)) {
        let Some(n) = self.get(id) else { return };
        if alpha <= 0.0 || n.kind == Kind::Group {
            return;
        }
        match n.kind {
            Kind::Layer => return self.draw_layer(view, n, alpha, fixed),
            Kind::Emitter => {
                if let Some(e) = &n.emitter {
                    gfx2d::push();
                    if !fixed {
                        self.camera.apply(view.w, view.h);
                    }
                    if e.additive {
                        gfx2d::blend(gfx2d::Blend::Add);
                    }
                    e.particles.draw();
                    if e.additive {
                        gfx2d::blend(gfx2d::Blend::Alpha);
                    }
                    gfx2d::pop();
                }
                return;
            }
            Kind::Tilemap => return self.draw_tilemap(view, id, n, alpha, fixed),
            Kind::Walker => {
                let (x0, y0, x1, y1) = self.screen_box(id, view, fixed);
                if x1 < 0.0 || y1 < 0.0 || x0 > view.w || y0 > view.h {
                    return;
                }
                return self.draw_walker(view, id, n, alpha, fixed);
            }
            _ => {}
        }
        let (lw, lh) = self.local_size(id);
        let (x0, y0, x1, y1) = self.screen_box(id, view, fixed);
        if x1 < 0.0 || y1 < 0.0 || x0 > view.w || y0 > view.h {
            return;
        }
        gfx2d::push();
        if !fixed {
            self.camera.apply(view.w, view.h);
        }
        // The chain of transforms from the root (any mix of scale and turn).
        let mut chain = Vec::new();
        let mut at = Some(id);
        while let Some(p) = at.and_then(|i| self.get(i)) {
            chain.push(p);
            at = p.parent;
        }
        for p in chain.iter().rev() {
            if self.camera.snap {
                gfx2d::translate(p.x.round(), p.y.round());
            } else {
                gfx2d::translate(p.x, p.y);
            }
            if p.rotation != 0.0 {
                gfx2d::rotate(p.rotation);
            }
            if p.sx != 1.0 || p.sy != 1.0 {
                gfx2d::scale(p.sx, p.sy);
            }
        }
        let color = with_alpha(n.color, alpha);
        match n.kind {
            Kind::Sprite => self.draw_sprite(n, color, lw, lh),
            Kind::Shape => draw_shape(n, color),
            Kind::Text => {
                crate::ui::text(&n.text).font(n.font).size(n.size).color(color).wrap(n.width).align(n.align).draw(0.0, 0.0);
            }
            Kind::Widget => {
                if let Some(w) = &n.widget {
                    w.draw_tinted(&self.theme, lw, lh, alpha * (n.color & 0xff) as f32 / 255.0, n.color | 0xff);
                }
            }
            Kind::Group | Kind::Layer | Kind::Tilemap | Kind::Walker | Kind::Emitter => {}
        }
        gfx2d::pop();
    }

    /// The node's box on the view (x0, y0, x1, y1), from its world
    /// transform and the camera (none for a `fixed` one): what culling
    /// tests, and where a description of the frame says it is.
    pub fn screen_box(&self, id: NodeId, view: &View, fixed: bool) -> (f32, f32, f32, f32) {
        let Some(n) = self.get(id) else { return (0.0, 0.0, 0.0, 0.0) };
        let m = self.world_transform(id);
        let (lw, lh) = self.local_size(id);
        let (ox, oy) = match n.kind {
            Kind::Sprite => (-n.anchor_x * lw, -n.anchor_y * lh),
            Kind::Text => (
                match n.align {
                    Align::Left => 0.0,
                    Align::Center => -lw / 2.0,
                    Align::Right => -lw,
                },
                0.0,
            ),
            Kind::Tilemap => (0.0, 0.0),
            Kind::Walker => (-lw / 2.0, -lh),
            _ => (-lw / 2.0, -lh / 2.0),
        };
        let pad = n.line.max(0.0);
        let corners = [(ox - pad, oy - pad), (ox + lw + pad, oy - pad), (ox - pad, oy + lh + pad), (ox + lw + pad, oy + lh + pad)];
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for (cx, cy) in corners {
            let (wx, wy) = m.apply(cx, cy);
            let (sx, sy) = if fixed { (wx, wy) } else { self.camera.to_screen(view.w, view.h, wx, wy) };
            (x0, y0, x1, y1) = (x0.min(sx), y0.min(sy), x1.max(sx), y1.max(sy));
        }
        (x0, y0, x1, y1)
    }

    /// The nodes drawn on the view this frame, in order, with their boxes:
    /// what the camera sees, after culling (layers and groups aren't listed).
    pub fn on_view(&self, view: &View) -> Vec<(NodeId, (f32, f32, f32, f32))> {
        self.draw_order()
            .into_iter()
            .filter(|(id, _, a, _)| *a > 0.0 && self.get(*id).is_some_and(|n| !matches!(n.kind, Kind::Group | Kind::Layer | Kind::Emitter | Kind::Tilemap)))
            .map(|(id, _, _, fixed)| (id, self.screen_box(id, view, fixed)))
            .filter(|(_, (x0, y0, x1, y1))| !(*x1 < 0.0 || *y1 < 0.0 || *x0 > view.w || *y0 > view.h))
            .collect()
    }

    /// A tile map's tiles on the view (in its parents' transform; culled by
    /// the view's box in the world, as if the map isn't turned or scaled).
    fn draw_tilemap(&self, view: &View, id: NodeId, n: &Node, alpha: f32, fixed: bool) {
        let Some(m) = &n.tilemap else { return };
        let (ox, oy) = self.world_position(id);
        let corners = [(0.0, 0.0), (view.w, 0.0), (0.0, view.h), (view.w, view.h)].map(|(x, y)| if fixed { (x, y) } else { self.camera.to_world(view.w, view.h, x, y) });
        let (wx0, wy0) = corners.iter().fold((f32::MAX, f32::MAX), |a, c| (a.0.min(c.0), a.1.min(c.1)));
        let (wx1, wy1) = corners.iter().fold((f32::MIN, f32::MIN), |a, c| (a.0.max(c.0), a.1.max(c.1)));
        let cx0 = (((wx0 - ox) / m.tile_w).floor() as i64).max(0);
        let cy0 = (((wy0 - oy) / m.tile_h).floor() as i64).max(0);
        let cx1 = (((wx1 - ox) / m.tile_w).ceil() as i64).min(m.cols as i64);
        let cy1 = (((wy1 - oy) / m.tile_h).ceil() as i64).min(m.rows as i64);
        let img = m.image.and_then(|i| self.image(i));
        let color = with_alpha(n.color, alpha);
        gfx2d::push();
        if !fixed {
            self.camera.apply(view.w, view.h);
        }
        let mut chain = Vec::new();
        let mut at = Some(id);
        while let Some(p) = at.and_then(|i| self.get(i)) {
            chain.push(p);
            at = p.parent;
        }
        for p in chain.iter().rev() {
            if self.camera.snap {
                gfx2d::translate(p.x.round(), p.y.round());
            } else {
                gfx2d::translate(p.x, p.y);
            }
            if p.rotation != 0.0 {
                gfx2d::rotate(p.rotation);
            }
            if p.sx != 1.0 || p.sy != 1.0 {
                gfx2d::scale(p.sx, p.sy);
            }
        }
        for cy in cy0..cy1 {
            for cx in cx0..cx1 {
                let Some(t) = m.cell(cx, cy) else { continue };
                let (x, y) = (cx as f32 * m.tile_w, cy as f32 * m.tile_h);
                match img.and_then(|im| im.image.map(|h| (im, h))) {
                    Some((im, handle)) => gfx2d::sprite(handle, im.frame_src(t.frame), [x, y, m.tile_w, m.tile_h], color),
                    None => gfx2d::rect(x, y, m.tile_w, m.tile_h, with_alpha(if t.solid { 0x808080ff } else { 0x80808060 }, alpha)),
                }
            }
        }
        gfx2d::pop();
    }

    /// A walker: the far foot, the body (bobbing, leaning about its hips),
    /// the near foot, facing the way it walks (or `flipX`'s way).
    fn draw_walker(&self, view: &View, id: NodeId, n: &Node, alpha: f32, fixed: bool) {
        let Some(wn) = &n.walker else { return };
        let w = &wn.walk;
        let (_, ground) = self.world_position(id);
        let k = wn.scale();
        let face = if n.flip_x { -1.0 } else { w.facing() };
        let hip = ground - wn.ankle() - w.bob();
        let color = with_alpha(n.color, alpha);
        let img = n.image.and_then(|i| self.image(i));
        let part = |src: [f32; 4], dst: [f32; 4], tint: u32| match img.and_then(|im| im.image.map(|h| (im, h))) {
            Some((im, handle)) => gfx2d::sprite(handle, im.sub(src), dst, tint),
            None => gfx2d::rect(dst[0], dst[1], dst[2], dst[3], with_alpha(tint & 0xffffff00 | 0x50, 1.0)),
        };
        gfx2d::push();
        if !fixed {
            self.camera.apply(view.w, view.h);
        }
        let foot = |i: usize, tint: u32| {
            let (x, h) = w.foot(i);
            gfx2d::push();
            gfx2d::translate(x, ground - h);
            gfx2d::rotate(w.foot_tilt(i));
            gfx2d::scale(face, 1.0);
            let (fw, fh) = (wn.foot[2] * k, wn.foot[3] * k);
            part(wn.foot, [-0.5 * fw, -fh, fw, fh], tint);
            gfx2d::pop();
        };
        foot(0, crate::scene::graphics_mul(0xb4b4bcff, color));
        gfx2d::push();
        gfx2d::translate(w.x(), hip);
        gfx2d::rotate(w.lean_angle() * face * w.facing());
        gfx2d::scale(face, 1.0);
        let (bw, bh) = (wn.body[2] * k, wn.body[3] * k);
        part(wn.body, [-0.5 * bw, -bh, bw, bh], color);
        gfx2d::pop();
        foot(1, color);
        gfx2d::pop();
    }

    fn draw_sprite(&self, n: &Node, color: u32, w: f32, h: f32) {
        let (x, y) = (-n.anchor_x * w, -n.anchor_y * h);
        let img = n.image.and_then(|i| self.image(i));
        match img.and_then(|im| im.image.map(|h| (im, h))) {
            Some((im, handle)) => {
                let src = im.frame_src(n.frame);
                if n.flip_x || n.flip_y {
                    gfx2d::scale(if n.flip_x { -1.0 } else { 1.0 }, if n.flip_y { -1.0 } else { 1.0 });
                }
                gfx2d::sprite(handle, src, [x, y, w, h], color);
            }
            // Not loaded yet (or natively): a faint box of its size.
            None => gfx2d::rect(x, y, w, h, with_alpha(color & 0xffffff00 | 0x40, 1.0)),
        }
    }

    /// The point of a layer's space at the view's middle: it moves by
    /// `parallax` of the camera's travel from its start, and shakes with the
    /// view (`fx.shake`) as the world does, whatever its parallax. A `fixed`
    /// layer does neither.
    pub fn layer_centre(&self, n: &Node, fixed: bool) -> (f32, f32) {
        if fixed {
            return (self.width / 2.0, self.height / 2.0);
        }
        let cam = &self.camera;
        let p = n.parallax;
        (self.width / 2.0 + (cam.x - self.width / 2.0) * p + cam.shake_x, self.height / 2.0 + (cam.y - self.height / 2.0) * p + cam.shake_y)
    }

    fn draw_layer(&self, view: &View, n: &Node, alpha: f32, fixed: bool) {
        let Some(im) = n.image.and_then(|i| self.image(i)) else { return };
        let (w, h) = (im.w * n.sx.abs(), im.h * n.sy.abs());
        if w < 1.0 || h < 1.0 {
            return;
        }
        let cam = &self.camera;
        let (cx, cy) = self.layer_centre(n, fixed);
        let k = if fixed { 1.0 } else { cam.scale };
        // The view's corners in the layer's space.
        let (vw, vh) = (view.w / k, view.h / k);
        let (left, top) = (cx - vw / 2.0, cy - vh / 2.0);
        let color = with_alpha(n.color, alpha);
        gfx2d::push();
        gfx2d::translate(view.w / 2.0, view.h / 2.0);
        gfx2d::scale(k, k);
        gfx2d::translate(-cx, -cy);
        if let Some(f) = n.fill {
            gfx2d::rect(left, n.y + h, vw, (top + vh - (n.y + h)).max(0.0), with_alpha(f, alpha));
        }
        let xs: Vec<f32> = if n.repeat_x {
            let first = n.x + ((left - n.x) / w).floor() * w;
            (0..).map(|i| first + i as f32 * w).take_while(|x| *x < left + vw).take(64).collect()
        } else {
            vec![n.x]
        };
        let ys: Vec<f32> = if n.repeat_y {
            let first = n.y + ((top - n.y) / h).floor() * h;
            (0..).map(|i| first + i as f32 * h).take_while(|y| *y < top + vh).take(64).collect()
        } else {
            vec![n.y]
        };
        for &y in &ys {
            for &x in &xs {
                if x + w < left || x > left + vw || y + h < top || y > top + vh {
                    continue;
                }
                match im.image {
                    Some(handle) => gfx2d::sprite(handle, im.src(), [x, y, w, h], color),
                    None => gfx2d::rect(x, y, w, h, with_alpha(color & 0xffffff00 | 0x40, 1.0)),
                }
            }
        }
        gfx2d::pop();
    }
}

fn draw_shape(n: &Node, color: u32) {
    match n.shape {
        ShapeKind::Rect => {
            let (x, y) = (-n.w / 2.0, -n.h / 2.0);
            if n.corner > 0.0 || n.line > 0.0 {
                gfx2d::rrect(x, y, n.w, n.h, n.corner, n.line, 0.0, color, color);
            } else {
                gfx2d::rect(x, y, n.w, n.h, color);
            }
        }
        ShapeKind::Circle => {
            if n.line > 0.0 {
                let r = n.radius;
                gfx2d::rrect(-r, -r, r * 2.0, r * 2.0, r, n.line, 0.0, color, color);
            } else {
                gfx2d::circle(0.0, 0.0, n.radius, color);
            }
        }
        ShapeKind::Poly if n.line <= 0.0 && n.points.len() >= 6 => gfx2d::poly(&n.points, color),
        ShapeKind::Poly | ShapeKind::Line => {
            let p = &n.points;
            let width = if n.line > 0.0 { n.line } else { 1.0 };
            for i in (0..p.len().saturating_sub(3)).step_by(2) {
                gfx2d::line(p[i], p[i + 1], p[i + 2], p[i + 3], width, color);
            }
            if n.shape == ShapeKind::Poly && p.len() >= 6 {
                let k = p.len();
                gfx2d::line(p[k - 2], p[k - 1], p[0], p[1], width, color);
            }
        }
    }
}

fn bounds(p: &[f32]) -> (f32, f32, f32, f32) {
    let (mut x0, mut y0, mut x1, mut y1) = (0f32, 0f32, 0f32, 0f32);
    for [x, y] in p.as_chunks::<2>().0 {
        (x0, y0, x1, y1) = (x0.min(*x), y0.min(*y), x1.max(*x), y1.max(*y));
    }
    (x0, y0, x1, y1)
}

/// Two colours multiplied channel by channel.
pub fn graphics_mul(a: u32, b: u32) -> u32 {
    let ch = |s: u32| (((a >> s) & 0xff) * ((b >> s) & 0xff) + 127) / 255;
    ch(24) << 24 | ch(16) << 16 | ch(8) << 8 | ch(0)
}

/// `rgba` with its alpha multiplied by `a`.
pub fn with_alpha(rgba: u32, a: f32) -> u32 {
    let al = ((rgba & 0xff) as f32 * a.clamp(0.0, 1.0)).round() as u32;
    (rgba & 0xffffff00) | al.min(255)
}

/// A 2D affine transform: `x' = a·x + c·y + e`, `y' = b·x + d·y + f`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Affine {
    pub const IDENTITY: Affine = Affine { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 };

    /// Translate, then rotate, then scale (a node's own transform).
    pub fn trs(x: f32, y: f32, r: f32, sx: f32, sy: f32) -> Affine {
        let (s, c) = r.sin_cos();
        Affine { a: c * sx, b: s * sx, c: -s * sy, d: c * sy, e: x, f: y }
    }

    /// `self` then `o` (o applied first to a point).
    pub fn mul(&self, o: &Affine) -> Affine {
        Affine {
            a: self.a * o.a + self.c * o.b,
            b: self.b * o.a + self.d * o.b,
            c: self.a * o.c + self.c * o.d,
            d: self.b * o.c + self.d * o.d,
            e: self.a * o.e + self.c * o.f + self.e,
            f: self.b * o.e + self.d * o.f + self.f,
        }
    }

    pub fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (self.a * x + self.c * y + self.e, self.b * x + self.d * y + self.f)
    }

    pub fn inverse(&self) -> Affine {
        let det = self.a * self.d - self.b * self.c;
        if det.abs() < 1e-12 {
            return Affine::IDENTITY;
        }
        let k = 1.0 / det;
        let (a, b, c, d) = (self.d * k, -self.b * k, -self.c * k, self.a * k);
        Affine { a, b, c, d, e: -(a * self.e + c * self.f), f: -(b * self.e + d * self.f) }
    }

    /// Position, rotation and scale (exact when there's no shear).
    pub fn decompose(&self) -> (f32, f32, f32, f32, f32) {
        let sx = (self.a * self.a + self.b * self.b).sqrt();
        let r = self.b.atan2(self.a);
        let det = self.a * self.d - self.b * self.c;
        let sy = if sx > 1e-12 { det / sx } else { (self.c * self.c + self.d * self.d).sqrt() };
        (self.e, self.f, r, sx, sy)
    }
}

#[cfg(test)]
mod tests;
