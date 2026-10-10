//! TTF/OTF → `.mbf` (mb_format::font): a subset of the font's glyphs baked
//! into a signed-distance-field atlas (or a 1-bit one for pixel fonts), with
//! metrics and kerning (the `kern` table and GPOS pair adjustments).
//!
//! Outlines come from ttf-parser (variable fonts at any `wght`), filled at 8×
//! oversampling with the nonzero rule; distances are exact Euclidean (the
//! same method tools/fontbake uses for the host's Inter), so outlines and
//! glows the host adds stay smooth.

use mb_format::font::{Glyph, Kern, Kind, MAX_ATLAS, MAX_GLYPHS, MbFont};
use ttf_parser::{Face, GlyphId, Tag};

/// Oversampling of the inside/outside mask the distances are measured on.
const OVERSAMPLE: usize = 8;

#[derive(Clone, Debug)]
pub struct Options {
    /// SDF: pixels per em the glyphs are stored at (16–64). Bitmap: the
    /// font's native pixel grid (pixels per em).
    pub em: f32,
    /// Bake 1-bit glyphs on a pixel grid instead of distance fields.
    pub bitmap: bool,
    /// `wght` for variable fonts (ignored by static fonts).
    pub weight: Option<f32>,
    /// What to bake: `(char, char whose glyph it uses)`; usually the same char.
    pub chars: Vec<(char, char)>,
    /// `key=value` lines stored in the file.
    pub meta: String,
}

/// The characters behind a `--chars` value: `latin1` (the default: ASCII and
/// U+00A0–U+00FF, so "Touché" and "Señor" draw), `ascii`, `caps` (Latin-1,
/// lowercase drawn as capitals), `digits`, or literal characters; join with
/// `+` (e.g. `caps+ŁŚ`). Each set but `digits` adds curly quotes, dashes,
/// `…`, `•` and a few HUD symbols. Space and `?` are always in.
pub fn charset(spec: &str) -> Result<Vec<(char, char)>, String> {
    let mut out: Vec<(char, char)> = Vec::new();
    let mut push = |c: char, from: char| {
        if !out.iter().any(|(x, _)| *x == c) {
            out.push((c, from));
        }
    };
    const SYMBOLS: &str = "‘’“”•…–—←↑→↓★♥×·°€";
    for part in spec.split('+') {
        match part {
            "ascii" => (0x20u8..0x7f).map(char::from).chain(SYMBOLS.chars()).for_each(|c| push(c, c)),
            "latin1" | "" => latin1().chain(SYMBOLS.chars()).for_each(|c| push(c, c)),
            "caps" => latin1().chain(SYMBOLS.chars()).for_each(|c| push(c, upper(c))),
            "digits" => "0123456789 +-×x.,:%!?/★♥".chars().for_each(|c| push(c, c)),
            lit => lit.chars().filter(|c| !c.is_control()).for_each(|c| push(c, c)),
        }
    }
    push(' ', ' ');
    push('?', '?');
    if out.len() > MAX_GLYPHS {
        return Err(format!("{} characters; a font holds at most {MAX_GLYPHS}", out.len()));
    }
    Ok(out)
}

/// Printable ASCII and Latin-1 but the soft hyphen.
fn latin1() -> impl Iterator<Item = char> {
    (0x20u32..0x7f).chain(0xa0..0x100).filter(|&c| c != 0xad).filter_map(char::from_u32)
}

/// The capital a caps font draws for `c`: only within Latin-1 (`µ` isn't `Μ`, `ß` stays).
fn upper(c: char) -> char {
    let mut u = c.to_uppercase();
    match (u.next(), u.next()) {
        (Some(u), None) if (u as u32) < 0x100 => u,
        _ => c,
    }
}

pub fn bake(data: &[u8], o: &Options) -> Result<MbFont, String> {
    let mut face = Face::parse(data, 0).map_err(|e| format!("not a font file ({e})"))?;
    if let Some(w) = o.weight {
        let wght = Tag::from_bytes(b"wght");
        if let Some(axis) = face.variation_axes().into_iter().find(|a| a.tag == wght) {
            face.set_variation(wght, w.clamp(axis.min_value, axis.max_value));
        }
    }
    let em = o.em.clamp(4.0, 64.0);
    let upem = face.units_per_em() as f32;
    let s = em / upem; // output px per font unit
    let spread = if o.bitmap { 0.0 } else { (em / 8.0).max(2.0) };

    // Rasterize each distinct glyph once; chars that share a glyph share its rect.
    struct Raster {
        gid: u16,
        w: usize,
        h: usize,
        px: Vec<u8>,
        ox: f32,
        oy: f32,
        at: [usize; 2],
    }
    let mut rasters: Vec<Raster> = Vec::new();
    let mut entries: Vec<(char, usize, f32)> = Vec::new(); // char, raster index, advance
    for &(c, from) in &o.chars {
        let Some(gid) = face.glyph_index(from).or_else(|| face.glyph_index(c)) else { continue };
        if gid.0 == 0 && c != ' ' {
            continue;
        }
        let advance = face.glyph_hor_advance(gid).unwrap_or(0) as f32 * s;
        let idx = match rasters.iter().position(|r| r.gid == gid.0) {
            Some(i) => i,
            None => {
                let (w, h, px, ox, oy) = if o.bitmap { bitmap_glyph(&face, gid, s) } else { sdf_glyph(&face, gid, s, spread) };
                rasters.push(Raster { gid: gid.0, w, h, px, ox, oy, at: [0, 0] });
                rasters.len() - 1
            }
        };
        entries.push((c, idx, advance));
    }
    if entries.iter().all(|e| e.0 == ' ' || e.0 == '?') {
        return Err("the font has none of the requested characters".into());
    }

    // Shelf packing, tallest first, into the narrowest power-of-two width that keeps it roughly square.
    let mut order: Vec<usize> = (0..rasters.len()).collect();
    order.sort_by(|&a, &b| rasters[b].h.cmp(&rasters[a].h).then(rasters[b].w.cmp(&rasters[a].w)));
    let mut packed = None;
    for width in [64usize, 128, 256, 512, 1024] {
        let (mut x, mut y, mut shelf) = (0, 0, 0);
        let mut at = vec![[0usize; 2]; rasters.len()];
        let mut fits = true;
        for &i in &order {
            let r = &rasters[i];
            if r.w + 1 > width {
                fits = false;
                break;
            }
            if x + r.w + 1 > width {
                x = 0;
                y += shelf + 1;
                shelf = 0;
            }
            at[i] = [x + 1, y + 1];
            x += r.w + 1;
            shelf = shelf.max(r.h);
        }
        let height = y + shelf + 2;
        if fits && (height <= width || width == MAX_ATLAS as usize) {
            packed = Some((width, height, at));
            break;
        }
    }
    let (aw, ah, at) = packed.ok_or("the glyphs don't fit a 1024×1024 atlas: use fewer --chars or a smaller --em")?;
    if ah > MAX_ATLAS as usize {
        return Err(format!("the glyphs need a 1024×{ah} atlas; use fewer --chars or a smaller --em"));
    }
    let mut atlas = vec![0u8; aw * ah];
    for (i, r) in rasters.iter_mut().enumerate() {
        r.at = at[i];
        for row in 0..r.h {
            let dst = (r.at[1] + row) * aw + r.at[0];
            atlas[dst..dst + r.w].copy_from_slice(&r.px[row * r.w..(row + 1) * r.w]);
        }
    }

    let mut glyphs: Vec<Glyph> = entries
        .iter()
        .map(|&(c, i, advance)| {
            let r = &rasters[i];
            let empty = c == ' ' || r.w == 0;
            Glyph {
                codepoint: c as u32,
                rect: if empty { [0, 0, 0, 0] } else { [r.at[0] as u16, r.at[1] as u16, r.w as u16, r.h as u16] },
                offset: [r.ox, r.oy],
                advance,
            }
        })
        .collect();
    glyphs.sort_by_key(|g| g.codepoint);

    // Kerning between every pair of baked characters.
    let gids: Vec<(char, GlyphId)> = o.chars.iter().filter_map(|&(c, from)| face.glyph_index(from).map(|g| (c, g))).filter(|(c, _)| glyphs.iter().any(|g| g.codepoint == *c as u32)).collect();
    let mut kerns = Vec::new();
    let pairs = PairKerning::new(&face);
    for &(a, ga) in &gids {
        for &(b, gb) in &gids {
            let k = pairs.get(&face, ga, gb);
            // Adjustments under ~1/80 em don't show at game sizes; dropping them keeps atlases small.
            if (k as f32 * s).abs() >= em * 0.0125 {
                kerns.push(Kern { left: a as u32, right: b as u32, adjust: k as f32 * s });
            }
        }
    }
    kerns.sort_by_key(|k| (k.left, k.right));
    kerns.truncate(mb_format::font::MAX_KERNS);

    let metric = |c: char, fallback: f32| face.glyph_index(c).and_then(|g| face.glyph_bounding_box(g)).map_or(fallback, |b| b.y_max as f32);
    let ascent = face.ascender() as f32 * s;
    let descent = -(face.descender() as f32) * s;
    let line_height = (face.ascender() as f32 - face.descender() as f32 + face.line_gap() as f32) * s;
    let cap_height = face.capital_height().map(|v| v as f32).filter(|v| *v > 0.0).unwrap_or_else(|| metric('H', upem * 0.7)) * s;
    let x_height = face.x_height().map(|v| v as f32).filter(|v| *v > 0.0).unwrap_or_else(|| metric('x', upem * 0.5)) * s;
    Ok(MbFont {
        kind: if o.bitmap { Kind::Bitmap } else { Kind::Sdf },
        em_px: em,
        ascent,
        descent,
        line_height,
        spread,
        cap_height,
        x_height,
        atlas: [aw as u16, ah as u16],
        meta: o.meta.clone(),
        glyphs,
        kerns,
        png: grey_png(aw as u32, ah as u32, &atlas),
    })
}

/// A glyph's outline as line segments in output pixels (x right, y down from the baseline).
pub fn outline(face: &Face, gid: GlyphId, s: f32) -> Vec<[f32; 4]> {
    struct Flatten {
        s: f32,
        segs: Vec<[f32; 4]>,
        start: (f32, f32),
        at: (f32, f32),
    }
    impl Flatten {
        fn line(&mut self, x: f32, y: f32) {
            let p = (x * self.s, -y * self.s);
            if p != self.at {
                self.segs.push([self.at.0, self.at.1, p.0, p.1]);
            }
            self.at = p;
        }
    }
    impl ttf_parser::OutlineBuilder for Flatten {
        fn move_to(&mut self, x: f32, y: f32) {
            self.at = (x * self.s, -y * self.s);
            self.start = self.at;
        }
        fn line_to(&mut self, x: f32, y: f32) {
            self.line(x, y);
        }
        fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
            let (x0, y0) = (self.at.0 / self.s, -self.at.1 / self.s);
            let n = 12;
            for i in 1..=n {
                let t = i as f32 / n as f32;
                let u = 1.0 - t;
                self.line(u * u * x0 + 2.0 * u * t * x1 + t * t * x, u * u * y0 + 2.0 * u * t * y1 + t * t * y);
            }
        }
        fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
            let (x0, y0) = (self.at.0 / self.s, -self.at.1 / self.s);
            let n = 16;
            for i in 1..=n {
                let t = i as f32 / n as f32;
                let u = 1.0 - t;
                let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                self.line(a * x0 + b * x1 + c * x2 + d * x, a * y0 + b * y1 + c * y2 + d * y);
            }
        }
        fn close(&mut self) {
            if self.at != self.start {
                self.segs.push([self.at.0, self.at.1, self.start.0, self.start.1]);
            }
            self.at = self.start;
        }
    }
    let mut f = Flatten { s, segs: Vec::new(), start: (0.0, 0.0), at: (0.0, 0.0) };
    face.outline_glyph(gid, &mut f);
    f.segs
}

/// Whether each pixel centre of a `w`×`h` grid is inside the outline (nonzero
/// winding). `segs` are in grid pixels.
pub fn fill(segs: &[[f32; 4]], w: usize, h: usize) -> Vec<bool> {
    let mut inside = vec![false; w * h];
    let mut xs: Vec<(f32, i32)> = Vec::new();
    for row in 0..h {
        let y = row as f32 + 0.5;
        xs.clear();
        for &[x0, y0, x1, y1] in segs {
            if (y0 <= y) != (y1 <= y) {
                let t = (y - y0) / (y1 - y0);
                xs.push((x0 + t * (x1 - x0), if y1 > y0 { 1 } else { -1 }));
            }
        }
        xs.sort_by(|a, b| a.0.total_cmp(&b.0));
        // Between consecutive crossings the winding number is constant: fill
        // the pixel centres there when it's nonzero.
        let mut wind = 0;
        for k in 0..xs.len().saturating_sub(1) {
            wind += xs[k].1;
            if wind != 0 {
                let i0 = ((xs[k].0 - 0.5).ceil().max(0.0) as usize).min(w);
                let i1 = ((xs[k + 1].0 - 0.5).ceil().max(0.0) as usize).min(w);
                inside[row * w + i0..row * w + i1.max(i0)].iter_mut().for_each(|v| *v = true);
            }
        }
    }
    inside
}

/// Output-pixel bounds of a glyph's outline, padded: (x0, y0, w, h).
fn grid(segs: &[[f32; 4]], pad: f32) -> Option<(f32, f32, usize, usize)> {
    if segs.is_empty() {
        return None;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for s in segs {
        x0 = x0.min(s[0]).min(s[2]);
        x1 = x1.max(s[0]).max(s[2]);
        y0 = y0.min(s[1]).min(s[3]);
        y1 = y1.max(s[1]).max(s[3]);
    }
    let (gx, gy) = ((x0 - pad).floor(), (y0 - pad).floor());
    Some((gx, gy, ((x1 + pad).ceil() - gx) as usize, ((y1 + pad).ceil() - gy) as usize))
}

fn sdf_glyph(face: &Face, gid: GlyphId, s: f32, spread: f32) -> (usize, usize, Vec<u8>, f32, f32) {
    let segs = outline(face, gid, s);
    let Some((gx, gy, w, h)) = grid(&segs, spread) else { return (0, 0, vec![], 0.0, 0.0) };
    let k = OVERSAMPLE as f32;
    let hi: Vec<[f32; 4]> = segs.iter().map(|q| [(q[0] - gx) * k, (q[1] - gy) * k, (q[2] - gx) * k, (q[3] - gy) * k]).collect();
    let (gw, gh) = (w * OVERSAMPLE, h * OVERSAMPLE);
    let inside = fill(&hi, gw, gh);
    let to_outside = edt(&inside, gw, gh, false);
    let to_inside = edt(&inside, gw, gh, true);
    let mut px = vec![0u8; w * h];
    for oy in 0..h {
        for ox in 0..w {
            let i = (oy * OVERSAMPLE + OVERSAMPLE / 2) * gw + ox * OVERSAMPLE + OVERSAMPLE / 2;
            let d = if inside[i] { to_outside[i].sqrt() - 0.5 } else { 0.5 - to_inside[i].sqrt() } / k;
            px[oy * w + ox] = (128.0 + d * 127.0 / spread).round().clamp(0.0, 255.0) as u8;
        }
    }
    (w, h, px, gx, gy)
}

/// A pixel font glyph at its native grid: a pixel is on when most of it is inside.
fn bitmap_glyph(face: &Face, gid: GlyphId, s: f32) -> (usize, usize, Vec<u8>, f32, f32) {
    let segs = outline(face, gid, s);
    let Some((gx, gy, w, h)) = grid(&segs, 0.0) else { return (0, 0, vec![], 0.0, 0.0) };
    let k = OVERSAMPLE as f32;
    let hi: Vec<[f32; 4]> = segs.iter().map(|q| [(q[0] - gx) * k, (q[1] - gy) * k, (q[2] - gx) * k, (q[3] - gy) * k]).collect();
    let (gw, gh) = (w * OVERSAMPLE, h * OVERSAMPLE);
    let inside = fill(&hi, gw, gh);
    // One transparent pixel around each glyph so linear filtering never bleeds.
    let (bw, bh) = (w + 2, h + 2);
    let mut px = vec![0u8; bw * bh];
    for oy in 0..h {
        for ox in 0..w {
            let mut n = 0;
            for sy in 0..OVERSAMPLE {
                for sx in 0..OVERSAMPLE {
                    n += inside[(oy * OVERSAMPLE + sy) * gw + ox * OVERSAMPLE + sx] as usize;
                }
            }
            if n * 2 >= OVERSAMPLE * OVERSAMPLE {
                px[(oy + 1) * bw + ox + 1] = 255;
            }
        }
    }
    (bw, bh, px, gx - 1.0, gy - 1.0)
}

/// Squared Euclidean distance from each pixel to the nearest pixel whose
/// `inside` equals `target` (Felzenszwalb & Huttenlocher, separable).
fn edt(inside: &[bool], w: usize, h: usize, target: bool) -> Vec<f32> {
    const FAR: f32 = 1e12;
    let mut d: Vec<f32> = inside.iter().map(|&v| if v == target { 0.0 } else { FAR }).collect();
    let mut col = Vec::with_capacity(h);
    for x in 0..w {
        col.clear();
        col.extend((0..h).map(|y| d[y * w + x]));
        for (y, v) in edt_1d(&col).into_iter().enumerate() {
            d[y * w + x] = v;
        }
    }
    for y in 0..h {
        let row = edt_1d(&d[y * w..(y + 1) * w]);
        d[y * w..(y + 1) * w].copy_from_slice(&row);
    }
    d
}

fn edt_1d(f: &[f32]) -> Vec<f32> {
    let n = f.len();
    let mut v = vec![0usize; n];
    let mut z = vec![0f32; n + 1];
    let mut k = 0;
    z[0] = f32::NEG_INFINITY;
    z[1] = f32::INFINITY;
    let sect = |q: usize, p: usize| ((f[q] + (q * q) as f32) - (f[p] + (p * p) as f32)) / (2.0 * (q as f32 - p as f32));
    for q in 1..n {
        let mut s = sect(q, v[k]);
        while s <= z[k] {
            k -= 1;
            s = sect(q, v[k]);
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f32::INFINITY;
    }
    let mut out = vec![0f32; n];
    k = 0;
    for (q, o) in out.iter_mut().enumerate() {
        while z[k + 1] < q as f32 {
            k += 1;
        }
        let dq = q as f32 - v[k] as f32;
        *o = dq * dq + f[v[k]];
    }
    out
}

/// Pair kerning from GPOS `kern` lookups (pair adjustments), else the `kern` table.
struct PairKerning<'a> {
    lookups: Vec<ttf_parser::gpos::PairAdjustment<'a>>,
}

impl<'a> PairKerning<'a> {
    fn new(face: &Face<'a>) -> PairKerning<'a> {
        use ttf_parser::gpos::PositioningSubtable;
        let mut lookups = Vec::new();
        if let Some(gpos) = face.tables().gpos {
            let kern = Tag::from_bytes(b"kern");
            let mut seen = Vec::new();
            for f in gpos.features {
                if f.tag != kern {
                    continue;
                }
                for li in f.lookup_indices {
                    if seen.contains(&li) {
                        continue;
                    }
                    seen.push(li);
                    let Some(lookup) = gpos.lookups.get(li) else { continue };
                    for sub in lookup.subtables.into_iter::<PositioningSubtable>() {
                        if let PositioningSubtable::Pair(p) = sub {
                            lookups.push(p);
                        }
                    }
                }
            }
        }
        PairKerning { lookups }
    }

    /// Advance adjustment between two glyphs, in font units.
    fn get(&self, face: &Face, a: GlyphId, b: GlyphId) -> i32 {
        use ttf_parser::gpos::PairAdjustment;
        if self.lookups.is_empty() {
            return face
                .tables()
                .kern
                .map(|k| k.subtables.into_iter().filter(|s| s.horizontal && !s.variable).filter_map(|s| s.glyphs_kerning(a, b)).map(i32::from).sum())
                .unwrap_or(0);
        }
        // The first subtable that covers the pair decides it.
        for p in &self.lookups {
            let Some(ci) = p.coverage().get(a) else { continue };
            let hit = match p {
                PairAdjustment::Format1 { sets, .. } => sets.get(ci).and_then(|set| set.get(b)),
                PairAdjustment::Format2 { classes, matrix, .. } => matrix.get((classes.0.get(a), classes.1.get(b))),
            };
            if let Some((v1, _)) = hit {
                return v1.x_advance as i32;
            }
        }
        0
    }
}

pub fn grey_png(w: u32, h: u32, data: &[u8]) -> Vec<u8> {
    let mut best: Option<Vec<u8>> = None;
    for filter in [png::Filter::Adaptive, png::Filter::Paeth, png::Filter::Sub] {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, w, h);
            enc.set_color(png::ColorType::Grayscale);
            enc.set_depth(png::BitDepth::Eight);
            enc.set_deflate_compression(png::DeflateCompression::Level(9));
            enc.set_filter(filter);
            let mut wr = enc.write_header().expect("png header");
            wr.write_image_data(data).expect("png data");
        }
        if best.as_ref().is_none_or(|b| out.len() < b.len()) {
            best = Some(out);
        }
    }
    best.unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_with_the_nonzero_rule() {
        // A 4×4 square from (1,1) to (3,3), and a hole wound the other way inside a bigger one.
        let sq = |x0: f32, y0: f32, x1: f32, y1: f32| vec![[x0, y0, x1, y0], [x1, y0, x1, y1], [x1, y1, x0, y1], [x0, y1, x0, y0]];
        let inside = fill(&sq(1.0, 1.0, 3.0, 3.0), 4, 4);
        let on: Vec<usize> = (0..16).filter(|&i| inside[i]).collect();
        assert_eq!(on, vec![5, 6, 9, 10]);
        let mut ring = sq(0.0, 0.0, 6.0, 6.0);
        ring.extend(sq(2.0, 2.0, 4.0, 4.0).into_iter().map(|s| [s[2], s[3], s[0], s[1]]));
        let inside = fill(&ring, 6, 6);
        assert!(inside[0] && !inside[2 * 6 + 2] && !inside[3 * 6 + 3] && inside[6 * 6 - 1]);
    }

    #[test]
    fn charsets() {
        let a = charset("ascii").unwrap();
        assert!(a.iter().any(|&(c, _)| c == 'a') && a.iter().any(|&(c, _)| c == '★'));
        assert!(!a.iter().any(|&(c, _)| c == 'é'));
        let caps = charset("caps").unwrap();
        assert_eq!(caps.iter().find(|(c, _)| *c == 'q'), Some(&('q', 'Q')));
        assert_eq!(caps.iter().find(|(c, _)| *c == 'é'), Some(&('é', 'É')));
        assert_eq!(caps.iter().find(|(c, _)| *c == 'µ'), Some(&('µ', 'µ')));
        assert_eq!(caps.iter().find(|(c, _)| *c == 'ß'), Some(&('ß', 'ß')));
        // The default: "Touché" draws.
        let l = charset("").unwrap();
        assert!("Touché «Señor» — “ÿ” ¿ü?".chars().all(|ch| l.iter().any(|&(c, _)| c == ch)));
        assert!(!l.iter().any(|&(c, _)| c == '\u{ad}'));
        let d = charset("0123").unwrap();
        assert_eq!(d.len(), 6, "digits plus space and ?");
        assert!(charset("latin1+ŁŚ").unwrap().iter().any(|&(c, _)| c == 'Ł'));
    }
}
