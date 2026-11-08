//! Studio 2.0's atlases (docs/STUDIO-2.md §4.2): small pictures packed into
//! a few big images, so a game with hundreds of them stays well under mb2d's
//! 128 images a session. Pictures up to 512 pixels a side are packed (PNGs
//! only: JPEG paintings and anything bigger stay whole), each with a
//! one-pixel border copied from its own edge, so linear sampling at an edge
//! never picks up a neighbour. Pixel art (`pixel = true` in the picture's
//! `.toml`) goes on pages of its own, which the engine samples nearest.
//!
//! The packing is a deterministic shelf packer: the same pictures always
//! give the same pages and places.

use std::collections::BTreeMap;

use crate::generate::img::Image;

/// Pictures this size or smaller (both sides) are packed.
pub const MAX_PACKED: u32 = 512;
/// An atlas page's width (and its greatest height).
pub const PAGE: u32 = 2048;

/// Where a picture went: page index and its top left there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placed {
    pub page: usize,
    pub at: [u32; 2],
}

/// Packed pages and where each picture went, by name.
pub struct Atlas {
    pub pages: Vec<Image>,
    /// Whether each page is pixel art (sampled nearest).
    pub pixel: Vec<bool>,
    pub placed: BTreeMap<String, Placed>,
}

/// Packs `(name, picture, pixel art)`; a group with one picture isn't worth
/// a page and is left out of `placed`.
pub fn pack(pictures: &[(String, Image, bool)]) -> Atlas {
    let mut out = Atlas { pages: Vec::new(), pixel: Vec::new(), placed: BTreeMap::new() };
    for pixel in [false, true] {
        let mut group: Vec<&(String, Image, bool)> = pictures.iter().filter(|p| p.2 == pixel && p.1.w <= MAX_PACKED && p.1.h <= MAX_PACKED).collect();
        if group.len() < 2 {
            continue;
        }
        group.sort_by(|a, b| b.1.h.cmp(&a.1.h).then(b.1.w.cmp(&a.1.w)).then(a.0.cmp(&b.0)));
        // Shelves on the current page: x, the shelf's top, its height.
        let mut page: Option<(usize, u32, u32, u32)> = None;
        for (name, img, _) in group {
            let (w, h) = (img.w + 2, img.h + 2);
            let (mut idx, mut x, mut top, mut shelf) = page.unwrap_or((usize::MAX, 0, 0, 0));
            if idx == usize::MAX || x + w > PAGE {
                (x, top, shelf) = (0, top + shelf, 0);
            }
            if idx == usize::MAX || top + h > PAGE {
                out.pages.push(Image::new(PAGE, PAGE, [0, 0, 0, 0]));
                out.pixel.push(pixel);
                (idx, x, top, shelf) = (out.pages.len() - 1, 0, 0, 0);
            }
            blit_extruded(&mut out.pages[idx], img, x + 1, top + 1);
            out.placed.insert(name.clone(), Placed { page: idx, at: [x + 1, top + 1] });
            page = Some((idx, x + w, top, shelf.max(h)));
        }
    }
    // Each page only as tall as it's used (to a multiple of 4).
    for (i, p) in out.pages.iter_mut().enumerate() {
        let bottom = out.placed.iter().filter(|(_, q)| q.page == i).map(|(n, q)| q.at[1] + pictures.iter().find(|x| &x.0 == n).map_or(0, |x| x.1.h) + 1).max().unwrap_or(0);
        let h = bottom.next_multiple_of(4).clamp(4, PAGE);
        let mut cropped = Image::new(PAGE, h, [0, 0, 0, 0]);
        for y in 0..h {
            for x in 0..PAGE {
                cropped.set(x, y, p.get(x, y));
            }
        }
        *p = cropped;
    }
    out
}

/// `img` at (x, y), with its edge pixels copied one pixel outward.
fn blit_extruded(page: &mut Image, img: &Image, x: u32, y: u32) {
    for j in -1..=img.h as i64 {
        for i in -1..=img.w as i64 {
            let sx = i.clamp(0, img.w as i64 - 1) as u32;
            let sy = j.clamp(0, img.h as i64 - 1) as u32;
            page.set((x as i64 + i) as u32, (y as i64 + j) as u32, img.get(sx, sy));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, c: u8) -> Image {
        Image::new(w, h, [c, c, c, 255])
    }

    #[test]
    fn small_pictures_share_pages_and_keep_their_pixels() {
        let pics = vec![
            ("a".to_string(), solid(10, 20, 10), false),
            ("b".to_string(), solid(30, 5, 20), false),
            ("c".to_string(), solid(16, 16, 30), true),
            ("d".to_string(), solid(16, 16, 40), true),
            ("big".to_string(), solid(600, 10, 50), false),
            ("e".to_string(), solid(8, 8, 60), false),
        ];
        let at = pack(&pics);
        assert_eq!(at.pages.len(), 2, "one smooth page, one pixel-art page");
        assert_eq!(at.pixel, [false, true]);
        assert!(!at.placed.contains_key("big"), "big pictures stay whole");
        for (name, img, _) in &pics {
            let Some(p) = at.placed.get(name) else { continue };
            let page = &at.pages[p.page];
            assert_eq!(page.get(p.at[0], p.at[1]), img.get(0, 0), "{name}");
            assert_eq!(page.get(p.at[0] + img.w - 1, p.at[1] + img.h - 1), img.get(0, 0), "{name}");
            // The border repeats the edge.
            assert_eq!(page.get(p.at[0] - 1, p.at[1] - 1), img.get(0, 0), "{name}");
        }
        // Tallest first: a (20 high) at the top left.
        assert_eq!(at.placed["a"], Placed { page: 0, at: [1, 1] });
        assert_eq!(at.pages[0].h % 4, 0);
        // The same pictures pack the same way.
        let again = pack(&pics);
        assert_eq!(again.placed, at.placed);
    }

    #[test]
    fn a_lone_picture_isnt_packed() {
        let at = pack(&[("a".into(), solid(4, 4, 1), false), ("p".into(), solid(4, 4, 1), true)]);
        assert!(at.pages.is_empty() && at.placed.is_empty());
    }
}
