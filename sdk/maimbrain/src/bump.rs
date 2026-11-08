//! Axis-aligned box collision with responses (slide, touch, cross,
//! bounce): a port of bump.lua 3.1.7 to Rust, so a world of hundreds of
//! moving boxes costs no Lua. Studio engine 2.0's `bump` is this module
//! from Lua (docs/STUDIO-2.md §5 [libraries]); a Rust game can use it
//! directly (Cargo feature `scene`).
//!
//! Ported from bump.lua by Enrique García Cota, under its license:
//!
//! > MIT LICENSE
//! >
//! > Copyright (c) 2014 Enrique García Cota
//! >
//! > Permission is hereby granted, free of charge, to any person obtaining a
//! > copy of this software and associated documentation files (the
//! > "Software"), to deal in the Software without restriction, including
//! > without limitation the rights to use, copy, modify, merge, publish,
//! > distribute, sublicense, and/or sell copies of the Software, and to
//! > permit persons to whom the Software is furnished to do so, subject to
//! > the following conditions:
//! >
//! > The above copyright notice and this permission notice shall be included
//! > in all copies or substantial portions of the Software.
//! >
//! > THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
//! > OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
//! > MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
//! > IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
//! > CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
//! > TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
//! > SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
//!
//! What differs: items are numbers (the engine maps Lua values to them), and
//! where bump.lua visits a cell's items in `pairs` order this visits them in
//! item order, so the same world always answers the same way (collisions are
//! still sorted by time of impact, then distance, as bump.lua sorts them).
//! Positions are f64, as Lua's numbers are.

use std::collections::{BTreeMap, BTreeSet};

const DELTA: f64 = 1e-10;

/// A box: left, top, width, height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// How a moving item responds to another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Response {
    /// Stops where it touched.
    Touch,
    /// Goes through, still reporting the collision.
    Cross,
    /// Slides along the other's side.
    Slide,
    /// Bounces off it.
    Bounce,
}

impl Response {
    pub fn name(self) -> &'static str {
        match self {
            Response::Touch => "touch",
            Response::Cross => "cross",
            Response::Slide => "slide",
            Response::Bounce => "bounce",
        }
    }

    pub fn named(s: &str) -> Option<Response> {
        Some(match s {
            "touch" => Response::Touch,
            "cross" => Response::Cross,
            "slide" => Response::Slide,
            "bounce" => Response::Bounce,
            _ => return None,
        })
    }
}

/// A collision, as bump.lua's `col` tables carry it.
#[derive(Clone, Debug, PartialEq)]
pub struct Collision {
    pub item: u32,
    pub other: u32,
    pub response: Response,
    /// The item was already overlapping the other.
    pub overlaps: bool,
    /// When along the move it hit (0–1; negative: the overlap's area).
    pub ti: f64,
    pub moved: (f64, f64),
    pub normal: (f64, f64),
    pub touch: (f64, f64),
    pub item_rect: Rect,
    pub other_rect: Rect,
    /// Where a slide or bounce went on to.
    pub slide: Option<(f64, f64)>,
    pub bounce: Option<(f64, f64)>,
}

fn sign(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x == 0.0 {
        0.0
    } else {
        -1.0
    }
}

fn nearest(x: f64, a: f64, b: f64) -> f64 {
    if (a - x).abs() < (b - x).abs() { a } else { b }
}

fn nearest_corner(r: Rect, px: f64, py: f64) -> (f64, f64) {
    (nearest(px, r.x, r.x + r.w), nearest(py, r.y, r.y + r.h))
}

/// Liang–Barsky: where the segment (x1, y1)–(x2, y2) enters and leaves `r`,
/// as fractions of it from `ti1` to `ti2`, with the normals there.
#[allow(clippy::type_complexity)]
fn segment_intersection(r: Rect, x1: f64, y1: f64, x2: f64, y2: f64, mut ti1: f64, mut ti2: f64) -> Option<(f64, f64, f64, f64, f64, f64)> {
    let (dx, dy) = (x2 - x1, y2 - y1);
    let (mut nx1, mut ny1, mut nx2, mut ny2) = (0.0, 0.0, 0.0, 0.0);
    for side in 1..=4 {
        let (nx, ny, p, q) = match side {
            1 => (-1.0, 0.0, -dx, x1 - r.x),
            2 => (1.0, 0.0, dx, r.x + r.w - x1),
            3 => (0.0, -1.0, -dy, y1 - r.y),
            _ => (0.0, 1.0, dy, r.y + r.h - y1),
        };
        if p == 0.0 {
            if q <= 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                if t > ti2 {
                    return None;
                } else if t > ti1 {
                    (ti1, nx1, ny1) = (t, nx, ny);
                }
            } else if t < ti1 {
                return None;
            } else if t < ti2 {
                (ti2, nx2, ny2) = (t, nx, ny);
            }
        }
    }
    Some((ti1, ti2, nx1, ny1, nx2, ny2))
}

/// The Minkowski difference of two boxes.
fn diff(a: Rect, b: Rect) -> Rect {
    Rect { x: b.x - a.x - a.w, y: b.y - a.y - a.h, w: a.w + b.w, h: a.h + b.h }
}

fn contains_point(r: Rect, px: f64, py: f64) -> bool {
    px - r.x > DELTA && py - r.y > DELTA && r.x + r.w - px > DELTA && r.y + r.h - py > DELTA
}

fn intersecting(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

fn square_distance(a: Rect, b: Rect) -> f64 {
    let dx = a.x - b.x + (a.w - b.w) / 2.0;
    let dy = a.y - b.y + (a.h - b.h) / 2.0;
    dx * dx + dy * dy
}

/// What [`detect`] finds: overlapping already, the time of impact, the
/// item's move, the normal and the touch point.
pub type Detected = (bool, f64, (f64, f64), (f64, f64), (f64, f64));

/// Whether `item` moving to `goal` hits `other`, and how.
pub fn detect(item: Rect, other: Rect, goal_x: f64, goal_y: f64) -> Option<Detected> {
    let (dx, dy) = (goal_x - item.x, goal_y - item.y);
    let d = diff(item, other);
    let (overlaps, ti, mut n) = if contains_point(d, 0.0, 0.0) {
        // Already intersecting: ti is the negative area of the intersection.
        let (px, py) = nearest_corner(d, 0.0, 0.0);
        let (wi, hi) = (item.w.min(px.abs()), item.h.min(py.abs()));
        (true, -wi * hi, (0.0, 0.0))
    } else {
        let (ti1, ti2, nx1, ny1, ..) = segment_intersection(d, 0.0, 0.0, dx, dy, f64::NEG_INFINITY, f64::INFINITY)?;
        // It tunnels into the other (and not just through a corner).
        if ti1 < 1.0 && (ti1 - ti2).abs() >= DELTA && (0.0 < ti1 + DELTA || (ti1 == 0.0 && ti2 > 0.0)) {
            (false, ti1, (nx1, ny1))
        } else {
            return None;
        }
    };
    let touch = if overlaps {
        if dx == 0.0 && dy == 0.0 {
            // Not moving: out by the shortest way.
            let (mut px, mut py) = nearest_corner(d, 0.0, 0.0);
            if px.abs() < py.abs() {
                py = 0.0;
            } else {
                px = 0.0;
            }
            n = (sign(px), sign(py));
            (item.x + px, item.y + py)
        } else {
            // Moving: back the way it came.
            let (ti1, _, nx, ny, ..) = segment_intersection(d, 0.0, 0.0, dx, dy, f64::NEG_INFINITY, 1.0)?;
            n = (nx, ny);
            (item.x + dx * ti1, item.y + dy * ti1)
        }
    } else {
        (item.x + dx * ti, item.y + dy * ti)
    };
    Some((overlaps, ti, (dx, dy), n, touch))
}

/// A world of boxes in a grid of cells.
#[derive(Clone, Debug)]
pub struct World {
    cell: f64,
    rects: BTreeMap<u32, Rect>,
    cells: BTreeMap<(i64, i64), BTreeSet<u32>>,
}

/// bump's error for an item it doesn't have, or has twice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Missing,
    Already,
}

impl World {
    /// bump.newWorld(cellSize): 64 is bump's default.
    pub fn new(cell_size: f64) -> World {
        World { cell: if cell_size > 0.0 { cell_size } else { 64.0 }, rects: BTreeMap::new(), cells: BTreeMap::new() }
    }

    pub fn cell_size(&self) -> f64 {
        self.cell
    }

    fn cell_rect(&self, r: Rect) -> (i64, i64, i64, i64) {
        let cx = (r.x / self.cell).floor() as i64 + 1;
        let cy = (r.y / self.cell).floor() as i64 + 1;
        let cr = ((r.x + r.w) / self.cell).ceil() as i64;
        let cb = ((r.y + r.h) / self.cell).ceil() as i64;
        (cx, cy, cr - cx + 1, cb - cy + 1)
    }

    fn cells_of(&self, r: Rect) -> impl Iterator<Item = (i64, i64)> {
        let (cl, ct, cw, ch) = self.cell_rect(r);
        (ct..ct + ch).flat_map(move |cy| (cl..cl + cw).map(move |cx| (cx, cy)))
    }

    pub fn add(&mut self, item: u32, r: Rect) -> Result<(), Error> {
        if self.rects.contains_key(&item) {
            return Err(Error::Already);
        }
        self.rects.insert(item, r);
        for c in self.cells_of(r).collect::<Vec<_>>() {
            self.cells.entry(c).or_default().insert(item);
        }
        Ok(())
    }

    pub fn remove(&mut self, item: u32) -> Result<(), Error> {
        let r = self.rects.remove(&item).ok_or(Error::Missing)?;
        for c in self.cells_of(r).collect::<Vec<_>>() {
            if let Some(set) = self.cells.get_mut(&c) {
                set.remove(&item);
                if set.is_empty() {
                    self.cells.remove(&c);
                }
            }
        }
        Ok(())
    }

    /// Moves or resizes an item without collisions.
    pub fn update(&mut self, item: u32, r: Rect) -> Result<(), Error> {
        let old = *self.rects.get(&item).ok_or(Error::Missing)?;
        if old != r {
            self.remove(item)?;
            self.add(item, r)?;
        }
        Ok(())
    }

    pub fn rect(&self, item: u32) -> Option<Rect> {
        self.rects.get(&item).copied()
    }

    pub fn has(&self, item: u32) -> bool {
        self.rects.contains_key(&item)
    }

    pub fn len(&self) -> usize {
        self.rects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rects.is_empty()
    }

    pub fn items(&self) -> impl Iterator<Item = u32> + '_ {
        self.rects.keys().copied()
    }

    fn in_cells(&self, r: Rect) -> BTreeSet<u32> {
        let mut out = BTreeSet::new();
        for c in self.cells_of(r) {
            if let Some(set) = self.cells.get(&c) {
                out.extend(set.iter().copied());
            }
        }
        out
    }

    /// Items whose boxes intersect `r` (and that `filter` keeps), by item.
    pub fn query_rect(&self, r: Rect, mut filter: impl FnMut(u32) -> bool) -> Vec<u32> {
        self.in_cells(r).into_iter().filter(|i| intersecting(r, self.rects[i]) && filter(*i)).collect()
    }

    /// Items whose boxes hold the point (and that `filter` keeps).
    pub fn query_point(&self, x: f64, y: f64, mut filter: impl FnMut(u32) -> bool) -> Vec<u32> {
        let r = Rect { x, y, w: 0.0, h: 0.0 };
        self.in_cells(r).into_iter().filter(|i| contains_point(self.rects[i], x, y) && filter(*i)).collect()
    }

    /// The collisions `item` (at `r`) would have moving to the goal: those
    /// `filter` answers for, sorted by time of impact, then distance.
    pub fn project(&self, item: Option<u32>, r: Rect, goal_x: f64, goal_y: f64, filter: &mut dyn FnMut(Option<u32>, u32) -> Option<Response>) -> Vec<Collision> {
        let tl = goal_x.min(r.x);
        let tt = goal_y.min(r.y);
        let tr = (goal_x + r.w).max(r.x + r.w);
        let tb = (goal_y + r.h).max(r.y + r.h);
        let mut cols = Vec::new();
        for other in self.in_cells(Rect { x: tl, y: tt, w: tr - tl, h: tb - tt }) {
            if Some(other) == item {
                continue;
            }
            let Some(response) = filter(item, other) else { continue };
            let o = self.rects[&other];
            if let Some((overlaps, ti, moved, normal, touch)) = detect(r, o, goal_x, goal_y) {
                cols.push(Collision { item: item.unwrap_or(u32::MAX), other, response, overlaps, ti, moved, normal, touch, item_rect: r, other_rect: o, slide: None, bounce: None });
            }
        }
        cols.sort_by(|a, b| {
            if a.ti == b.ti {
                square_distance(a.item_rect, a.other_rect).total_cmp(&square_distance(a.item_rect, b.other_rect))
            } else {
                a.ti.total_cmp(&b.ti)
            }
        });
        cols
    }

    /// Where `item` can go toward the goal, and what it hit on the way
    /// (bump's `check`; `move_item` also moves it there).
    pub fn check(&self, item: u32, goal_x: f64, goal_y: f64, filter: &mut dyn FnMut(u32, u32) -> Option<Response>) -> Result<(f64, f64, Vec<Collision>), Error> {
        let r = self.rect(item).ok_or(Error::Missing)?;
        let mut visited: BTreeSet<u32> = BTreeSet::from([item]);
        let (mut gx, mut gy) = (goal_x, goal_y);
        let mut cols = Vec::new();
        let mut vf = |visited: &BTreeSet<u32>, other: u32| if visited.contains(&other) { None } else { filter(item, other) };
        let mut projected = self.project(Some(item), r, gx, gy, &mut |_, o| vf(&visited, o));
        while let Some(first) = projected.first().cloned() {
            let mut col = first;
            visited.insert(col.other);
            let (x, y) = col.touch;
            match col.response {
                Response::Touch => {
                    (gx, gy) = col.touch;
                    projected = Vec::new();
                }
                Response::Cross => {
                    projected = self.project(Some(item), r, gx, gy, &mut |_, o| vf(&visited, o));
                }
                Response::Slide => {
                    if col.moved.0 != 0.0 || col.moved.1 != 0.0 {
                        if col.normal.0 != 0.0 {
                            gx = x;
                        } else {
                            gy = y;
                        }
                    }
                    col.slide = Some((gx, gy));
                    let at = Rect { x, y, ..r };
                    projected = self.project(Some(item), at, gx, gy, &mut |_, o| vf(&visited, o));
                }
                Response::Bounce => {
                    let (mut bx, mut by) = (x, y);
                    if col.moved.0 != 0.0 || col.moved.1 != 0.0 {
                        let (mut bnx, mut bny) = (gx - x, gy - y);
                        if col.normal.0 == 0.0 {
                            bny = -bny;
                        } else {
                            bnx = -bnx;
                        }
                        (bx, by) = (x + bnx, y + bny);
                    }
                    col.bounce = Some((bx, by));
                    (gx, gy) = (bx, by);
                    let at = Rect { x, y, ..r };
                    projected = self.project(Some(item), at, gx, gy, &mut |_, o| vf(&visited, o));
                }
            }
            cols.push(col);
        }
        Ok((gx, gy, cols))
    }

    /// bump's `move`: `check`, then the item is moved where it can go.
    pub fn move_item(&mut self, item: u32, goal_x: f64, goal_y: f64, filter: &mut dyn FnMut(u32, u32) -> Option<Response>) -> Result<(f64, f64, Vec<Collision>), Error> {
        let (x, y, cols) = self.check(item, goal_x, goal_y, filter)?;
        let r = self.rect(item).ok_or(Error::Missing)?;
        self.update(item, Rect { x, y, ..r })?;
        Ok((x, y, cols))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { x, y, w, h }
    }

    fn slide(_: u32, _: u32) -> Option<Response> {
        Some(Response::Slide)
    }

    /// bump.lua's spec (spec/World_spec.lua): moving into a wall slides along it.
    #[test]
    fn a_box_slides_along_a_wall() {
        let mut w = World::new(64.0);
        w.add(1, r(0.0, 0.0, 10.0, 10.0)).unwrap();
        w.add(2, r(14.0, 0.0, 10.0, 10.0)).unwrap();
        let (x, y, cols) = w.move_item(1, 10.0, 10.0, &mut slide).unwrap();
        assert_eq!((x, y), (4.0, 10.0));
        assert_eq!(cols.len(), 1);
        assert_eq!((cols[0].other, cols[0].normal, cols[0].touch, cols[0].slide), (2, (-1.0, 0.0), (4.0, 4.0), Some((4.0, 10.0))));
        assert_eq!(w.rect(1), Some(r(4.0, 10.0, 10.0, 10.0)));
    }

    #[test]
    fn touch_stops_cross_passes_bounce_reflects() {
        let mut w = World::new(64.0);
        w.add(1, r(0.0, 0.0, 10.0, 10.0)).unwrap();
        w.add(2, r(0.0, 20.0, 10.0, 10.0)).unwrap();
        let (_, y, _) = w.check(1, 0.0, 30.0, &mut |_, _| Some(Response::Touch)).unwrap();
        assert_eq!(y, 10.0);
        let (_, y, cols) = w.check(1, 0.0, 30.0, &mut |_, _| Some(Response::Cross)).unwrap();
        assert_eq!((y, cols.len(), cols[0].response), (30.0, 1, Response::Cross));
        let (_, y, cols) = w.check(1, 0.0, 30.0, &mut |_, _| Some(Response::Bounce)).unwrap();
        assert_eq!((y, cols[0].bounce), (-10.0, Some((0.0, -10.0))));
        // No answer from the filter: no collision.
        let (_, y, cols) = w.check(1, 0.0, 30.0, &mut |_, _| None).unwrap();
        assert_eq!((y, cols.len()), (30.0, 0));
    }

    /// Collisions come nearest first: the first wall stops it.
    #[test]
    fn collisions_are_sorted_by_time_then_distance() {
        let mut w = World::new(16.0);
        w.add(1, r(0.0, 0.0, 4.0, 4.0)).unwrap();
        w.add(2, r(30.0, 0.0, 4.0, 4.0)).unwrap();
        w.add(3, r(10.0, 0.0, 4.0, 4.0)).unwrap();
        let cols = w.project(Some(1), w.rect(1).unwrap(), 40.0, 0.0, &mut |_, _| Some(Response::Cross));
        assert_eq!(cols.iter().map(|c| c.other).collect::<Vec<_>>(), [3, 2]);
        let (x, _, cols) = w.move_item(1, 40.0, 0.0, &mut |_, _| Some(Response::Touch)).unwrap();
        assert_eq!((x, cols.len()), (6.0, 1));
    }

    /// An item already overlapping another is pushed out the short way.
    #[test]
    fn overlaps_resolve_the_short_way() {
        let mut w = World::new(64.0);
        w.add(1, r(0.0, 0.0, 10.0, 10.0)).unwrap();
        w.add(2, r(8.0, 0.0, 10.0, 10.0)).unwrap();
        let (x, y, cols) = w.check(1, 0.0, 0.0, &mut |_, _| Some(Response::Touch)).unwrap();
        assert!(cols[0].overlaps && cols[0].ti == -20.0);
        assert_eq!((x, y, cols[0].normal), (-2.0, 0.0, (-1.0, 0.0)));
        // A slide that isn't moving stays put (bump.lua's rule).
        let (x, y, _) = w.check(1, 0.0, 0.0, &mut slide).unwrap();
        assert_eq!((x, y), (0.0, 0.0));
    }

    #[test]
    fn queries_and_bookkeeping() {
        let mut w = World::new(64.0);
        w.add(1, r(0.0, 0.0, 10.0, 10.0)).unwrap();
        w.add(2, r(100.0, 100.0, 10.0, 10.0)).unwrap();
        assert_eq!(w.add(1, r(0.0, 0.0, 1.0, 1.0)), Err(Error::Already));
        assert_eq!(w.query_rect(r(5.0, 5.0, 100.0, 100.0), |_| true), [1, 2]);
        assert_eq!(w.query_rect(r(5.0, 5.0, 100.0, 100.0), |i| i == 2), [2]);
        assert_eq!(w.query_point(5.0, 5.0, |_| true), [1]);
        assert!(w.query_point(10.0, 5.0, |_| true).is_empty(), "the edge isn't inside");
        w.update(2, r(1000.0, 0.0, 10.0, 10.0)).unwrap();
        assert!(w.query_rect(r(90.0, 90.0, 30.0, 30.0), |_| true).is_empty());
        w.remove(1).unwrap();
        assert_eq!((w.len(), w.remove(1)), (1, Err(Error::Missing)));
    }
}
