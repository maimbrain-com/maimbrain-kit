//! Where generated art stands: a figure's feet (the lowest opaque row, and
//! how tall it is), a painting's floor line, a painting letterboxed inside
//! its own image, and a figure the generator cut off at the image's edge.
//! `mb art` records these in the sidecar and writes them to the game's
//! `src/art_meta.rs` as `maimbrain::ground::{Footing, Backdrop}` constants,
//! so `ground::Figure` stands a sprite on a painting's floor by its feet
//! (docs/GENERATE.md "Standing things on the floor").

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::img::{Image, oklab, oklab_dist};

/// A pixel counts as part of a figure from this alpha up.
const SOLID: u8 = 128;

/// Where a figure (a sprite, or every frame of a sheet) stands, in texels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Footing {
    /// The image, or one frame's cell.
    pub cell: [u32; 2],
    /// The point on the ground under the figure: the middle of what touches
    /// the ground, at the bottom of its lowest opaque row (frames: the lowest
    /// of any frame), from the cell's top-left.
    pub foot: [f32; 2],
    /// The figure's first opaque row (frames: the highest of any frame).
    pub top: u32,
    /// How wide what touches the ground is (the feet, a hem, a base).
    pub base: u32,
    /// The figure's opaque columns, left edge and right edge (frames: of any
    /// frame): with `top` and the feet, the box a tap on it should hit.
    pub span: [u32; 2],
}

impl Footing {
    /// The figure's height in texels, top to feet.
    pub fn height(&self) -> f32 {
        self.foot[1] - self.top as f32
    }

    pub fn to_json(self, height_units: f32) -> Value {
        json!({
            "cell": self.cell,
            "foot": [round1(self.foot[0]), round1(self.foot[1])],
            "top": self.top,
            "base": self.base,
            "span": self.span,
            // Logical units the figure is drawn tall (ground::Figure's default height).
            "height": round1(height_units),
        })
    }
}

fn round1(v: f32) -> f32 {
    (v * 10.0).round() / 10.0
}

/// Measures where `cells` frames of `cell` texels (laid out in rows of
/// `im.w / cell[0]`) stand. None if nothing is drawn.
pub fn footing(im: &Image, cell: [u32; 2], cells: u32) -> Option<Footing> {
    let [cw, ch] = [cell[0].clamp(1, im.w), cell[1].clamp(1, im.h)];
    let cols = (im.w / cw).max(1);
    let n = cells.clamp(1, cols * (im.h / ch).max(1));
    let origin = |i: u32| ((i % cols) * cw, (i / cols) * ch);
    let solid = |i: u32, x: u32, y: u32| {
        let (ox, oy) = origin(i);
        im.get(ox + x, oy + y)[3] >= SOLID
    };
    let (mut top, mut bottom) = (u32::MAX, None::<u32>);
    let (mut left, mut right) = (u32::MAX, 0);
    for i in 0..n {
        for y in 0..ch {
            let mut row = (0..cw).filter(|&x| solid(i, x, y));
            if let Some(x0) = row.next() {
                top = top.min(y);
                bottom = Some(bottom.map_or(y, |b| b.max(y)));
                left = left.min(x0);
                right = right.max(row.next_back().unwrap_or(x0) + 1);
            }
        }
    }
    let bottom = bottom?;
    let foot_y = bottom + 1;
    let band = ((foot_y - top) * 6 / 100).max(2).min(foot_y - top);
    let (mut x0, mut x1) = (u32::MAX, 0);
    for i in 0..n {
        for y in foot_y - band..foot_y {
            for x in 0..cw {
                if solid(i, x, y) {
                    x0 = x0.min(x);
                    x1 = x1.max(x);
                }
            }
        }
    }
    Some(Footing { cell: [cw, ch], foot: [(x0 + x1 + 1) as f32 / 2.0, foot_y as f32], top, base: x1 + 1 - x0, span: [left, right] })
}

/// Edges of an image, in the order [`edge_cover`] measures them.
pub const EDGES: [&str; 4] = ["top", "bottom", "left", "right"];

/// For each edge of a keyed image (top, bottom, left, right): the longest
/// run of it the subject covers (alpha ≥ 128, not one of the keyed
/// background colors `bgs` left behind), as a share of the edge.
pub fn edge_cover(k: &Image, bgs: &[[u8; 3]], tolerance: f32) -> [f32; 4] {
    let labs: Vec<[f32; 3]> = bgs.iter().map(|c| oklab(*c)).collect();
    let subject = |x: u32, y: u32| {
        let p = k.get(x, y);
        p[3] >= SOLID && !labs.iter().any(|b| oklab_dist(oklab([p[0], p[1], p[2]]), *b) < tolerance * 2.0)
    };
    let run = |pts: &mut dyn Iterator<Item = bool>, len: u32| {
        let (mut best, mut cur) = (0u32, 0u32);
        for s in pts {
            cur = if s { cur + 1 } else { 0 };
            best = best.max(cur);
        }
        best as f32 / len.max(1) as f32
    };
    let (w, h) = (k.w, k.h);
    [
        run(&mut (0..w).map(|x| subject(x, 0)), w),
        run(&mut (0..w).map(|x| subject(x, h - 1)), w),
        run(&mut (0..h).map(|y| subject(0, y)), h),
        run(&mut (0..h).map(|y| subject(w - 1, y)), h),
    ]
}

/// The edges a figure was cut off at: the subject runs along at least 1%
/// of the bottom edge (a hem, feet), or 2% of another, and at least 3
/// pixels. Generators draw whole figures with empty space around them; one
/// that touches the edge continues past it.
pub fn cut_edges(cover: [f32; 4], dims: [u32; 2]) -> Vec<&'static str> {
    (0..4)
        .filter(|&i| {
            let len = if i < 2 { dims[0] } else { dims[1] };
            let least = if i == 1 { 0.01 } else { 0.02 };
            cover[i] >= least && cover[i] * len as f32 >= 3.0
        })
        .map(|i| EDGES[i])
        .collect()
}

/// A painting's floor line: the row (in texels from the top) where its
/// floor meets what's behind it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Floor {
    pub y: f32,
    /// 0–1: how much more it stands out than any other line.
    pub confidence: f32,
    /// "edge" (an opaque painting's wall-to-floor line) or "surface" (the
    /// top of a transparent layer's solid ground).
    pub how: &'static str,
}

impl Floor {
    pub fn to_json(self, h: u32, given: bool) -> Value {
        json!({
            "y": round1(self.y),
            "fraction": (self.y / h.max(1) as f32 * 1000.0).round() / 1000.0,
            "found": if given { "given" } else { self.how },
            "confidence": (self.confidence * 100.0).round() / 100.0,
        })
    }
}

fn luma(p: [u8; 4]) -> f32 {
    oklab([p[0], p[1], p[2]])[0]
}

/// The floor line of an opaque painting (a corridor, a room, a street seen
/// from the side): the horizontal line across most of its width, in its
/// lower 60%, where the picture steps from one tone to another (the
/// baseboard, the foot of a wall, the near edge of a ground). Each row is
/// scored by the share of columns that step the same way across it (a
/// picture frame or a door spans only part of the width) times the mean
/// step, weighted a little toward the bottom. None when no row steps across
/// at least 40% of the width.
pub fn find_floor(im: &Image) -> Option<Floor> {
    if im.h < 24 {
        return None;
    }
    let step = im.w.div_ceil(512).max(1);
    let xs: Vec<u32> = (0..im.w).step_by(step as usize).collect();
    let d = (im.h / 240).max(1);
    // Not the last 2% of rows: a line there is the picture's edge, not a floor to stand on.
    let (lo, hi) = (im.h * 40 / 100, (im.h * 98 / 100).min(im.h - d - 1));
    let mut score = vec![0f32; im.h as usize];
    let mut cons = vec![0f32; im.h as usize];
    for y in lo.max(d)..hi {
        let (mut pos, mut neg, mut spos, mut sneg) = (0u32, 0u32, 0f32, 0f32);
        for &x in &xs {
            let delta = luma(im.get(x, y + d)) - luma(im.get(x, y - d));
            if delta > 0.03 {
                pos += 1;
                spos += delta;
            } else if delta < -0.03 {
                neg += 1;
                sneg -= delta;
            }
        }
        let (n, s) = if pos >= neg { (pos, spos) } else { (neg, sneg) };
        let c = n as f32 / xs.len() as f32;
        cons[y as usize] = c;
        score[y as usize] = c * (s / n.max(1) as f32) * (0.8 + 0.2 * y as f32 / im.h as f32);
    }
    let best = (lo..hi).max_by(|a, b| score[*a as usize].total_cmp(&score[*b as usize]))?;
    if cons[best as usize] < 0.4 {
        return None;
    }
    // A step d rows either side is found at a few neighbouring rows: the line is their middle.
    let peak = score[best as usize];
    let (mut a, mut b) = (best, best);
    while a > lo && score[a as usize - 1] >= peak * 0.8 {
        a -= 1;
    }
    while b + 1 < hi && score[b as usize + 1] >= peak * 0.8 {
        b += 1;
    }
    let y = (a + b) as f32 / 2.0 + 0.5;
    let apart = (im.h / 20).max(3);
    let second = (lo..hi).filter(|r| r.abs_diff(best) > apart).map(|r| score[r as usize]).fold(0.0, f32::max);
    let confidence = (peak / second.max(1e-6) - 1.0).clamp(0.0, 1.0);
    Some(Floor { y, confidence, how: "edge" })
}

/// The top of a transparent layer's solid ground: the highest row from
/// which every row down to the bottom is at least 90% opaque. None if the
/// bottom row isn't.
pub fn solid_top(im: &Image) -> Option<Floor> {
    let full = |y: u32| (0..im.w).filter(|&x| im.get(x, y)[3] >= SOLID).count() as f32 >= im.w as f32 * 0.9;
    let mut y = im.h;
    while y > 0 && full(y - 1) {
        y -= 1;
    }
    if y == im.h {
        return None;
    }
    // Crisp when the row above is mostly clear (a level top), soft when hills rise from it.
    let above = if y > 0 { (0..im.w).filter(|&x| im.get(x, y - 1)[3] >= SOLID).count() as f32 / im.w as f32 } else { 0.0 };
    Some(Floor { y: y as f32, confidence: (1.0 - above).clamp(0.0, 1.0), how: "surface" })
}

/// Flat bands along an image's edges: rows (or columns) of one color, as a
/// generator leaves when it draws a picture smaller than its canvas.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Letterbox {
    /// Texels to crop: top, bottom, left, right. Only bands on both
    /// opposite sides, of about the same color, are cropped.
    pub crop: [u32; 4],
    /// A band on one side only (side index as in [`EDGES`], its depth):
    /// maybe a letterbox, maybe a flat sky or floor, so it's left in.
    pub lone: Option<(usize, u32)>,
}

/// The flat band along one edge: how many lines deep, and its color. A
/// line is flat when `share` of it is within `tol` (in 0–255, on every
/// channel: Oklab exaggerates the noise JPEG leaves on black) of its median.
fn band(im: &Image, side: usize, key: Option<[u8; 3]>, tol: u8, share: f32) -> (u32, [u8; 3]) {
    let (len, depth) = if side < 2 { (im.w, im.h) } else { (im.h, im.w) };
    let step = len.div_ceil(1024).max(1);
    let at = |line: u32, i: u32| -> [u8; 4] {
        match side {
            0 => im.get(i, line),
            1 => im.get(i, im.h - 1 - line),
            2 => im.get(line, i),
            _ => im.get(im.w - 1 - line, i),
        }
    };
    let near = |a: [u8; 3], b: [u8; 3], t: u8| (0..3).all(|c| a[c].abs_diff(b[c]) <= t);
    let mut color: Option<[u8; 3]> = None;
    let mut n = 0;
    while n < depth * 2 / 5 {
        let px: Vec<[u8; 3]> = (0..len).step_by(step as usize).map(|i| at(n, i)).map(|p| [p[0], p[1], p[2]]).collect();
        let mut med = [0u8; 3];
        for (c, m) in med.iter_mut().enumerate() {
            let mut v: Vec<u8> = px.iter().map(|p| p[c]).collect();
            v.sort_unstable();
            *m = v[v.len() / 2];
        }
        let flat = px.iter().filter(|p| near(**p, med, tol)).count() as f32 >= px.len() as f32 * share;
        if !flat || color.is_some_and(|c| !near(c, med, tol)) {
            break;
        }
        color.get_or_insert(med);
        n += 1;
    }
    match color {
        // The flat key color behind a transparent layer's subject is background, not a letterbox.
        Some(c) if key.is_some_and(|k| oklab_dist(oklab(k), oklab(c)) < 0.25) => (0, c),
        Some(c) => (n, c),
        None => (0, [0; 3]),
    }
}

/// Finds a letterbox (or pillarbox) inside a generated painting: bands on
/// opposite sides of about one color, flat to JPEG's noise; or one band
/// alone, flat as only a drawn bar is (a painted floor or sky has more
/// texture than that).
pub fn letterbox(im: &Image, key: Option<[u8; 3]>) -> Letterbox {
    let bands: Vec<(u32, [u8; 3])> = (0..4).map(|s| band(im, s, key, 10, 0.97)).collect();
    let mut out = Letterbox::default();
    for (a, b, dim) in [(0usize, 1usize, im.h), (2, 3, im.w)] {
        let least = (dim / 120).max(2);
        let (na, ca) = bands[a];
        let (nb, cb) = bands[b];
        if na >= least && nb >= least && (0..3).all(|c| ca[c].abs_diff(cb[c]) <= 16) && na + nb < dim / 2 {
            out.crop[a] = na;
            out.crop[b] = nb;
        } else {
            for s in [a, b] {
                let (n, _) = band(im, s, key, 3, 0.995);
                if n >= (dim / 50).max(2) && out.lone.is_none_or(|l| n > l.1) {
                    out.lone = Some((s, n));
                }
            }
        }
    }
    out
}

impl Letterbox {
    pub fn any(&self) -> bool {
        self.crop.iter().any(|&c| c > 0)
    }

    pub fn apply(&self, im: &Image) -> Image {
        let [t, b, l, r] = self.crop;
        if !self.any() {
            return im.clone();
        }
        im.crop(l, t, im.w - l - r, im.h - t - b)
    }
}

/// An opaque painting fitted to `w`×`h` with its floor line (`floor`,
/// texels from the source's top) on output row `target`: scaled as little
/// as covers the frame with the floor there, cropped around the middle
/// horizontally.
pub fn fit_floor(src: &Image, w: u32, h: u32, floor: f32, target: f32) -> Image {
    let (sw, sh) = (src.w as f32, src.h as f32);
    let floor = floor.clamp(1.0, sh - 1.0);
    let target = target.clamp(1.0, h as f32 - 1.0);
    let s = (w as f32 / sw).max(target / floor).max((h as f32 - target) / (sh - floor));
    let (nw, nh) = (((sw * s).round() as u32).max(w), ((sh * s).round() as u32).max(h));
    let scaled = src.resize(nw, nh);
    let top = ((floor * s - target).round().max(0.0) as u32).min(nh - h);
    scaled.crop((nw - w) / 2, top, w, h)
}

/// A constant name for an asset: `corridor_0` → `CORRIDOR_0`.
pub fn const_name(stem: &str) -> String {
    let mut s: String = stem.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' }).collect();
    if s.starts_with(|c: char| c.is_ascii_digit()) {
        s.insert(0, '_');
    }
    s
}

/// What `src/art_meta.rs` says about one generated image.
#[derive(Debug, PartialEq)]
pub enum Meta {
    Figure {
        name: String,
        path: String,
        footing: Footing,
        cells: u32,
        cols: u32,
        height: f32,
        requested: bool,
        /// What it's of (its prompt's subject).
        subject: String,
        /// Its real-world height in meters, and where that came from: "given"
        /// (`--real`) or the thing the table's height is for ("mirror").
        real: Option<(f32, String)>,
        /// Its height came from the scene's scale (not --height).
        scaled: bool,
        /// When it was generated (its sidecar's date).
        made: String,
    },
    Backdrop {
        name: String,
        path: String,
        size: [u32; 2],
        floor: Option<Floor>,
        given: bool,
        /// The objects it shows (`result.shows`; an older painting's, from its prompt).
        shows: Vec<String>,
    },
}

impl Meta {
    pub fn name(&self) -> &str {
        match self {
            Meta::Figure { name, .. } | Meta::Backdrop { name, .. } => name,
        }
    }
}

/// A figure's real height from its sidecar (`result.real`), else the
/// table's for its subject.
fn real_of(result: &Value, subject: &str) -> Option<(f32, String)> {
    let r = if result["real"].is_object() { result["real"].clone() } else { super::scene::real_json(None, subject) };
    Some((r["m"].as_f64()? as f32, r["from"].as_str().unwrap_or("given").to_string()))
}

/// A generated image's footing (sprites and frames) or floor (layers and
/// backgrounds), from its sidecar's recipe and the PNG as it is now.
pub fn meta_of(png: &Path, game: &Path) -> Option<Meta> {
    let sc = super::read_sidecar(png).ok()?;
    let r = &sc.recipe;
    let kind = r["kind"].as_str()?;
    // A `--walker` sheet's body has no feet (walk::WalkSprite stands it).
    if !matches!(kind, "sprite" | "frames" | "layer" | "background") || r["walker"] == true {
        return None;
    }
    let im = Image::decode(&std::fs::read(png).ok()?).ok()?;
    let path = super::rel(png, game);
    let name = png.file_stem()?.to_string_lossy().to_string();
    match kind {
        "sprite" | "frames" => {
            let size = r["size"].as_array().and_then(|a| Some([a.first()?.as_u64()? as u32, a.get(1)?.as_u64()? as u32]));
            let (cell, cells) = match (kind, size) {
                ("frames", Some(c)) => (c, (r["frames"].as_u64().unwrap_or(1) as u32).max(1)),
                _ => ([im.w, im.h], 1),
            };
            let footing = footing(&im, cell, cells)?;
            let cols = (im.w / footing.cell[0]).max(1);
            let requested = r["height"].as_f64().map(|h| h as f32);
            let subject = r["subject"].as_str().unwrap_or("").to_string();
            let real = real_of(&sc.result, &subject);
            let scaled = r["sized_by"].is_string();
            Some(Meta::Figure { name, path, footing, cells, cols, height: requested.unwrap_or(footing.height() / 2.0), requested: requested.is_some(), subject, real, scaled, made: sc.date.clone() })
        }
        _ => {
            // Measured from the image as it is now (it may have been edited since).
            let floor = if im.is_opaque() { find_floor(&im) } else { solid_top(&im) };
            let shows = match sc.result["shows"].as_array() {
                Some(a) => a.iter().filter_map(|o| o.as_str().map(str::to_string)).collect(),
                None => super::scene::objects_in(r["subject"].as_str().unwrap_or("")),
            };
            Some(Meta::Backdrop { name, path, size: [im.w, im.h], floor, given: sc.result["floor"]["found"] == "given", shows })
        }
    }
}

/// The scene's scale, units a meter, and the figure it's from: the first
/// figure generated with `--height` (the hero, usually) that has a real
/// height. None until there is one.
pub fn scene_scale(metas: &[Meta]) -> Option<(f32, String)> {
    metas
        .iter()
        .filter_map(|m| match m {
            Meta::Figure { name, height, requested: true, scaled: false, real: Some((r, _)), made, .. } if *r >= super::scene::HANDHELD => Some((made, height / r, name)),
            _ => None,
        })
        .min_by(|a, b| a.0.cmp(b.0))
        .map(|(_, k, n)| (k, n.clone()))
}

fn collect_pngs(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                collect_pngs(&p, out);
            } else if p.extension().is_some_and(|x| x == "png") {
                out.push(p);
            }
        }
    }
}

/// Every generated figure and painting in a game's assets/, sorted by path.
pub fn game_meta(game: &Path) -> Vec<Meta> {
    let mut pngs = Vec::new();
    collect_pngs(&game.join("assets"), &mut pngs);
    pngs.sort();
    pngs.iter().filter_map(|p| meta_of(p, game)).collect()
}

fn f(v: f32) -> String {
    let r = round1(v);
    if r.fract() == 0.0 { format!("{r:.1}") } else { format!("{r}") }
}

/// The text of `src/art_meta.rs` for these.
pub fn meta_rust(metas: &[Meta]) -> String {
    let mut out = vec![
        "//! Generated by `mb art` from the generated images in assets/: where each figure's feet are and".to_string(),
        "//! where each painting's floor is, for `maimbrain::ground` (docs/GENERATE.md \"Standing things on".to_string(),
        "//! the floor\"). `mb art` rewrites it after every generation; don't edit it.".to_string(),
        "#![allow(dead_code)]".to_string(),
        "use maimbrain::ground::{Backdrop, Footing};".to_string(),
    ];
    let mut names: Vec<String> = Vec::new();
    let mut unique = |n: &str| {
        let mut c = const_name(n);
        while names.contains(&c) {
            c.push('_');
        }
        names.push(c.clone());
        c
    };
    for m in metas {
        out.push(String::new());
        match m {
            Meta::Figure { name, path, footing: ft, cells, cols, height, requested, real, .. } => {
                let c = unique(name);
                let frames = if *cells > 1 { format!(" ({cells} frames of {}×{})", ft.cell[0], ft.cell[1]) } else { String::new() };
                let why = if *requested { "as asked with --height" } else { "1 texel = 0.5 units" };
                let world = match real {
                    Some((m, from)) if from == "given" => format!(", {m} m in the world (--real)"),
                    Some((m, from)) => format!(", {m} m in the world (a typical {from})"),
                    None => String::new(),
                };
                out.push(format!("/// {path}{frames}: feet at ({}, {}), {} units tall ({why}){world}.", f(ft.foot[0]), f(ft.foot[1]), f(*height)));
                out.push(format!(
                    "pub const {c}: Footing = Footing {{ name: {name:?}, cell: [{}, {}], cols: {cols}, foot: [{}, {}], top: {}, base: {}, span: [{}, {}], height: {}, real: {} }};",
                    f(ft.cell[0] as f32),
                    f(ft.cell[1] as f32),
                    f(ft.foot[0]),
                    f(ft.foot[1]),
                    f(ft.top as f32),
                    f(ft.base as f32),
                    f(ft.span[0] as f32),
                    f(ft.span[1] as f32),
                    f(*height),
                    real.as_ref().map_or("0.0".to_string(), |r| format!("{:?}", (r.0 * 1000.0).round() / 1000.0))
                ));
            }
            Meta::Backdrop { name, path, size, floor, given, shows } => match floor {
                Some(fl) => {
                    let c = unique(name);
                    let how = if *given { "placed there with --floor".to_string() } else { format!("found: {}, confidence {:.2}", fl.how, fl.confidence) };
                    out.push(format!("/// {path}: floor at row {} of {} ({how}).", f(fl.y), size[1]));
                    if !shows.is_empty() {
                        out.push(format!("/// Painted into it (don't stand a prop of these beside it): {}.", shows.join(", ")));
                    }
                    out.push(format!("pub const {c}: Backdrop = Backdrop {{ name: {name:?}, size: [{}, {}], floor: {} }};", f(size[0] as f32), f(size[1] as f32), f(fl.y)));
                }
                None => {
                    out.push(format!("// {path}: no floor line found (a sky, or no level line across it)."));
                    if !shows.is_empty() {
                        out.push(format!("// Painted into it (don't stand a prop of these beside it): {}.", shows.join(", ")));
                    }
                }
            },
        }
    }
    out.join("\n") + "\n"
}

/// Writes `src/art_meta.rs` when the game has a src/ and anything to say;
/// returns its path when it changed. Says how to use it the first time.
pub fn write_meta(game: &Path) -> Result<Option<PathBuf>, String> {
    let src = game.join("src");
    if !src.is_dir() {
        return Ok(None);
    }
    let metas = game_meta(game);
    let path = src.join("art_meta.rs");
    if metas.is_empty() && !path.exists() {
        return Ok(None);
    }
    let text = meta_rust(&metas);
    if std::fs::read_to_string(&path).ok().as_deref() == Some(text.as_str()) {
        return Ok(None);
    }
    std::fs::write(&path, &text).map_err(|e| format!("{}: {e}", path.display()))?;
    let used = std::fs::read_to_string(src.join("lib.rs")).is_ok_and(|t| t.contains("mod art_meta"));
    eprintln!(
        "  wrote {}: each figure's feet and each painting's floor{}",
        path.display(),
        if used { "" } else { " (add `mod art_meta;` to src/lib.rs, and stand things with maimbrain::ground::Figure)" }
    );
    Ok(Some(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    const WALL: [u8; 4] = [120, 90, 70, 255];
    const BOARD: [u8; 4] = [40, 28, 22, 255];

    /// A corridor: a wall with noise and a picture frame, a floor below row `floor`.
    fn corridor(w: u32, h: u32, floor: u32) -> Image {
        let mut im = Image::new(w, h, WALL);
        let mut seed = 7u32;
        for y in 0..h {
            for x in 0..w {
                seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
                let n = ((seed >> 16) % 17) as u8;
                let base = if y >= floor { BOARD } else { WALL };
                im.set(x, y, [base[0] + n, base[1] + n, base[2] + n, 255]);
            }
        }
        // A picture frame on the wall: a strong edge, but only across part of the width.
        for y in h / 3..h / 2 {
            for x in w / 5..w * 2 / 5 {
                im.set(x, y, [230, 210, 120, 255]);
            }
        }
        // A door reaching down to the floor breaks the line in one place.
        for y in h / 4..floor {
            for x in w * 3 / 5..w * 3 / 5 + w / 10 {
                im.set(x, y, [60, 40, 30, 255]);
            }
        }
        im
    }

    #[test]
    fn a_corridors_floor_is_its_long_level_line() {
        let im = corridor(720, 480, 380);
        let f = find_floor(&im).expect("a floor");
        assert!((f.y - 380.0).abs() <= 2.0, "{f:?}");
        assert_eq!(f.how, "edge");
        assert!(f.confidence > 0.3, "{f:?}");
        // A sky with nothing level in it has none.
        let mut sky = Image::new(200, 200, [0; 4]);
        for y in 0..200 {
            for x in 0..200 {
                sky.set(x, y, [(x / 2) as u8, 40, 120, 255]);
            }
        }
        assert_eq!(find_floor(&sky), None);
    }

    #[test]
    fn a_transparent_layers_floor_is_the_top_of_its_ground() {
        let mut im = Image::new(100, 100, [0; 4]);
        for y in 70..100 {
            for x in 0..100 {
                im.set(x, y, [50, 50, 50, 255]);
            }
        }
        let f = solid_top(&im).unwrap();
        assert_eq!((f.y, f.how), (70.0, "surface"));
        assert!(f.confidence > 0.9);
        assert_eq!(solid_top(&Image::new(10, 10, [0; 4])), None);
    }

    #[test]
    fn letterbox_bars_are_found_and_cropped() {
        let inner = corridor(400, 200, 160);
        let mut boxed = Image::new(400, 300, [0, 0, 0, 255]);
        boxed.blit(&inner, 0, 40);
        let lb = letterbox(&boxed, None);
        assert_eq!(lb.crop, [40, 60, 0, 0], "{lb:?}");
        let cropped = lb.apply(&boxed);
        assert_eq!((cropped.w, cropped.h), (400, 200));
        assert!((find_floor(&cropped).unwrap().y - 160.0).abs() <= 2.0);
        // A flat band on one side only is reported, not cropped (it may be a flat sky).
        let mut sky = corridor(400, 300, 250);
        for y in 0..30 {
            for x in 0..400 {
                sky.set(x, y, [20, 20, 60, 255]);
            }
        }
        let lb = letterbox(&sky, None);
        assert!(!lb.any());
        assert_eq!(lb.lone, Some((0, 30)));
        // The key color behind a transparent layer is background, not a letterbox.
        let mut keyed = Image::new(100, 100, [255, 0, 255, 255]);
        for y in 40..60 {
            for x in 0..100 {
                keyed.set(x, y, [(x * 2) as u8, 90, 30, 255]);
            }
        }
        assert!(!letterbox(&keyed, Some([255, 0, 255])).any());
        // A painting without bars is left alone.
        assert_eq!(letterbox(&corridor(300, 200, 150), None), Letterbox::default());
    }

    #[test]
    fn a_painting_is_fitted_with_its_floor_on_the_target_row() {
        let src = corridor(600, 400, 300);
        let out = fit_floor(&src, 300, 300, 300.0, 250.0);
        assert_eq!((out.w, out.h), (300, 300));
        let f = find_floor(&out).unwrap();
        assert!((f.y - 250.0).abs() <= 2.0, "{f:?}");
        // A floor that would need more room below is reached by scaling up.
        let out = fit_floor(&src, 300, 300, 300.0, 120.0);
        assert!((find_floor(&out).map_or(0.0, |f| f.y) - 120.0).abs() <= 3.0);
    }

    /// A figure: a body from row `top` down to `feet` (exclusive), legs at the bottom.
    fn figure(w: u32, h: u32, top: u32, feet: u32) -> Image {
        let mut im = Image::new(w, h, [0; 4]);
        for y in top..feet {
            let (x0, x1) = if y >= feet - (feet - top) / 4 { (w / 2 - 6, w / 2 + 8) } else { (w / 2 - 12, w / 2 + 12) };
            for x in x0..x1 {
                im.set(x, y, [200, 60, 60, 255]);
            }
        }
        im
    }

    #[test]
    fn a_figures_footing_is_its_lowest_opaque_row() {
        let im = figure(64, 64, 10, 58);
        let ft = footing(&im, [64, 64], 1).unwrap();
        assert_eq!(ft.foot[1], 58.0);
        assert_eq!(ft.top, 10);
        assert_eq!(ft.base, 14);
        assert_eq!(ft.span, [20, 44]);
        assert_eq!(ft.foot[0], 33.0);
        assert_eq!(ft.height(), 48.0);
        // Frames: the lowest feet of any frame, the highest head.
        let mut sheet = Image::new(128, 64, [0; 4]);
        sheet.blit(&figure(64, 64, 12, 56), 0, 0);
        sheet.blit(&figure(64, 64, 8, 58), 64, 0);
        let ft = footing(&sheet, [64, 64], 2).unwrap();
        assert_eq!((ft.foot[1], ft.top, ft.cell), (58.0, 8, [64, 64]));
        assert_eq!(footing(&Image::new(8, 8, [0; 4]), [8, 8], 1), None);
    }

    #[test]
    fn a_figure_running_off_the_bottom_was_cut_off() {
        // A witch whose skirt runs into the bottom edge: cut.
        let cut = figure(200, 200, 20, 200);
        let cover = edge_cover(&cut, &[], 0.09);
        assert!(cover[1] > 0.05, "{cover:?}");
        assert_eq!(cut_edges(cover, [200, 200]), vec!["bottom"]);
        // A whole figure with space under its feet: not.
        let whole = figure(200, 200, 20, 180);
        assert!(cut_edges(edge_cover(&whole, &[], 0.09), [200, 200]).is_empty());
        // Background the key left along the edge isn't the subject.
        let mut left = whole.clone();
        for x in 0..200 {
            left.set(x, 199, [250, 0, 250, 255]);
        }
        assert!(cut_edges(edge_cover(&left, &[[255, 0, 255]], 0.09), [200, 200]).is_empty());
        // A stray pixel or two isn't a cut.
        let mut speck = whole;
        speck.set(10, 199, [9, 9, 9, 255]);
        assert!(cut_edges(edge_cover(&speck, &[], 0.09), [200, 200]).is_empty());
    }

    #[test]
    fn art_meta_lists_figures_and_floors() {
        let metas = vec![
            Meta::Figure {
                name: "girl".into(),
                path: "assets/girl.png".into(),
                footing: Footing { cell: [128, 128], foot: [64.0, 120.0], top: 20, base: 30, span: [40, 90] },
                cells: 4,
                cols: 4,
                height: 50.0,
                requested: false,
                subject: "a girl in a nightgown".into(),
                real: Some((1.3, "girl".into())),
                scaled: false,
                made: "2026-10-13T10:00:00Z".into(),
            },
            Meta::Figure {
                name: "1up".into(),
                path: "assets/1up.png".into(),
                footing: Footing { cell: [64, 64], foot: [32.5, 60.0], top: 4, base: 10, span: [20, 45] },
                cells: 1,
                cols: 1,
                height: 70.0,
                requested: true,
                subject: "a mushroom".into(),
                real: Some((0.5, "given".into())),
                scaled: false,
                made: "2026-10-13T10:00:00Z".into(),
            },
            Meta::Backdrop { name: "hall_0".into(), path: "assets/hall_0.png".into(), size: [1440, 640], floor: Some(Floor { y: 540.0, confidence: 0.8, how: "edge" }), given: false, shows: vec!["rocking horse".into(), "mirror".into()] },
            Meta::Backdrop { name: "sky_0".into(), path: "assets/sky_0.png".into(), size: [1440, 640], floor: None, given: false, shows: vec![] },
        ];
        let rs = meta_rust(&metas);
        for want in [
            "use maimbrain::ground::{Backdrop, Footing};",
            "pub const GIRL: Footing = Footing { name: \"girl\", cell: [128.0, 128.0], cols: 4, foot: [64.0, 120.0], top: 20.0, base: 30.0, span: [40.0, 90.0], height: 50.0, real: 1.3 };",
            "/// assets/girl.png (4 frames of 128×128): feet at (64.0, 120.0), 50.0 units tall (1 texel = 0.5 units), 1.3 m in the world (a typical girl).",
            "real: 0.5 };",
            "0.5 m in the world (--real)",
            "/// Painted into it (don't stand a prop of these beside it): rocking horse, mirror.",
            "pub const _1UP: Footing",
            "foot: [32.5, 60.0]",
            "(as asked with --height)",
            "pub const HALL_0: Backdrop = Backdrop { name: \"hall_0\", size: [1440.0, 640.0], floor: 540.0 };",
            "// assets/sky_0.png: no floor line found",
        ] {
            assert!(rs.contains(want), "missing {want:?} in\n{rs}");
        }
    }
}

