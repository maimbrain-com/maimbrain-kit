//! A [`Kind::Tilemap`](super::Kind)'s map: a grid of tiles from one tile
//! set's sheet, with the collision Studio 1.0's maps have (solid tiles,
//! one-way ledges, hazards). Positions are the map's own, (0, 0) its top
//! left; the node's position places it in the world.

/// One kind of tile in a set.
#[derive(Clone, Debug, PartialEq)]
pub struct Tile {
    pub name: String,
    /// Its frame in the sheet, from 1.
    pub frame: u32,
    pub solid: bool,
    /// Stands on it from above; passes through from below and the sides.
    pub oneway: bool,
    /// Hurts (not solid unless also `solid`).
    pub hazard: bool,
}

/// What a box ran into in [`TileMap::move_box`].
pub mod hit {
    /// A wall, left or right.
    pub const SIDE: u8 = 1;
    /// The ground (or a ledge) under it.
    pub const GROUND: u8 = 2;
    /// A ceiling.
    pub const CEILING: u8 = 4;
    /// It overlaps a hazard.
    pub const HAZARD: u8 = 8;
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TileMap {
    pub cols: u32,
    pub rows: u32,
    pub tile_w: f32,
    pub tile_h: f32,
    /// Row by row: an index into `tiles`.
    pub cells: Vec<Option<u16>>,
    pub tiles: Vec<Tile>,
    /// The tile set's sheet (frames `tile_w` × `tile_h`).
    pub image: Option<super::ImageId>,
    /// Named spots (`{ marker = "start" }` in a map's legend): name, x, y.
    pub markers: Vec<(String, f32, f32)>,
}

impl TileMap {
    /// The map's size in units.
    pub fn size(&self) -> (f32, f32) {
        (self.cols as f32 * self.tile_w, self.rows as f32 * self.tile_h)
    }

    /// The tile in cell (cx, cy), if any.
    pub fn cell(&self, cx: i64, cy: i64) -> Option<&Tile> {
        if cx < 0 || cy < 0 || cx >= self.cols as i64 || cy >= self.rows as i64 {
            return None;
        }
        self.cells[(cy as u32 * self.cols + cx as u32) as usize].and_then(|t| self.tiles.get(t as usize))
    }

    /// The cell a point is in.
    pub fn cell_of(&self, x: f32, y: f32) -> (i64, i64) {
        ((x / self.tile_w).floor() as i64, (y / self.tile_h).floor() as i64)
    }

    /// The tile at a point (map units).
    pub fn tile_at(&self, x: f32, y: f32) -> Option<&Tile> {
        let (cx, cy) = self.cell_of(x, y);
        self.cell(cx, cy)
    }

    /// Whether a point is in a solid tile.
    pub fn solid_at(&self, x: f32, y: f32) -> bool {
        self.tile_at(x, y).is_some_and(|t| t.solid)
    }

    /// A cell's collision: solid 1, one-way 2, hazard 4. Beyond the left
    /// and right edges is wall; above and below is open (a fall leaves).
    pub fn flags(&self, cx: i64, cy: i64) -> u8 {
        if cy < 0 || cy >= self.rows as i64 {
            return 0;
        }
        if cx < 0 || cx >= self.cols as i64 {
            return 1;
        }
        self.cell(cx, cy).map_or(0, |t| t.solid as u8 | (t.oneway as u8) << 1 | (t.hazard as u8) << 2)
    }

    /// Moves a box (centre x, y; size w × h; map units) by (dx, dy) against
    /// the solid tiles, in small steps, as Studio 1.0's collision does. A
    /// box falls through one-way ledges when `drop`. Returns where it ended
    /// and what it hit ([`hit`]).
    #[allow(clippy::too_many_arguments)]
    pub fn move_box(&self, mut x: f32, mut y: f32, w: f32, h: f32, dx: f32, dy: f32, drop: bool) -> (f32, f32, u8) {
        let (tw, th) = (self.tile_w, self.tile_h);
        let span = |lo: f32, hi: f32, t: f32| ((lo / t).floor() as i64, ((hi - 1e-4) / t).floor() as i64);
        let mut flags = 0;
        let steps = (dx.abs() / (tw * 0.5)).max(dy.abs() / (th * 0.5)).ceil().max(1.0);
        let (sx, sy) = (dx / steps, dy / steps);
        for _ in 0..steps as usize {
            if sx != 0.0 {
                x += sx;
                let (x0, x1) = span(x - w / 2.0, x + w / 2.0, tw);
                let (y0, y1) = span(y - h / 2.0, y + h / 2.0, th);
                let mut hit: Option<i64> = None;
                for cy in y0..=y1 {
                    for cx in x0..=x1 {
                        if self.flags(cx, cy) & 1 != 0 {
                            hit = Some(match hit {
                                None => cx,
                                Some(c) if sx > 0.0 => c.min(cx),
                                Some(c) => c.max(cx),
                            });
                        }
                    }
                }
                if let Some(cx) = hit {
                    x = if sx > 0.0 { cx as f32 * tw - w / 2.0 } else { (cx + 1) as f32 * tw + w / 2.0 };
                    flags |= hit::SIDE;
                }
            }
            if sy != 0.0 {
                let before = y + h / 2.0;
                y += sy;
                let (x0, x1) = span(x - w / 2.0, x + w / 2.0, tw);
                let (y0, y1) = span(y - h / 2.0, y + h / 2.0, th);
                let mut hit: Option<i64> = None;
                for cy in y0..=y1 {
                    for cx in x0..=x1 {
                        let f = self.flags(cx, cy);
                        let blocks = f & 1 != 0 || (f & 2 != 0 && sy > 0.0 && !drop && before <= cy as f32 * th + 1e-4);
                        if blocks {
                            hit = Some(match hit {
                                None => cy,
                                Some(c) if sy > 0.0 => c.min(cy),
                                Some(c) => c.max(cy),
                            });
                        }
                    }
                }
                if let Some(cy) = hit {
                    if sy > 0.0 {
                        y = cy as f32 * th - h / 2.0;
                        flags |= hit::GROUND;
                    } else {
                        y = (cy + 1) as f32 * th + h / 2.0;
                        flags |= hit::CEILING;
                    }
                }
            }
        }
        // Standing still on the ground still counts as on it.
        if flags & hit::GROUND == 0 && dy >= 0.0 {
            let (x0, x1) = span(x - w / 2.0, x + w / 2.0, tw);
            let bottom = y + h / 2.0;
            let below = ((bottom + 0.5) / th).floor() as i64;
            if (bottom / th - (bottom / th).round()).abs() < 1e-3 && (x0..=x1).any(|cx| self.flags(cx, below) & 3 != 0) {
                flags |= hit::GROUND;
            }
        }
        let (x0, x1) = span(x - w / 2.0, x + w / 2.0, tw);
        let (y0, y1) = span(y - h / 2.0, y + h / 2.0, th);
        if (y0..=y1).any(|cy| (x0..=x1).any(|cx| self.flags(cx, cy) & 4 != 0)) {
            flags |= hit::HAZARD;
        }
        (x, y, flags)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A floor, a wall at the right, a ledge and a spike.
    fn map() -> TileMap {
        let rows = ["....#", "..=.#", "...^#", "#####"];
        let tiles = vec![
            Tile { name: "rock".into(), frame: 1, solid: true, oneway: false, hazard: false },
            Tile { name: "ledge".into(), frame: 2, solid: false, oneway: true, hazard: false },
            Tile { name: "spike".into(), frame: 3, solid: false, oneway: false, hazard: true },
        ];
        let cells = rows.iter().flat_map(|r| r.chars().map(|c| match c { '#' => Some(0), '=' => Some(1), '^' => Some(2), _ => None })).collect();
        TileMap { cols: 5, rows: 4, tile_w: 16.0, tile_h: 16.0, cells, tiles, image: None, markers: vec![] }
    }

    #[test]
    fn boxes_land_stop_at_walls_and_pass_ledges_from_below() {
        let m = map();
        assert_eq!(m.size(), (80.0, 64.0));
        assert!(m.solid_at(70.0, 10.0) && !m.solid_at(10.0, 10.0));
        assert_eq!(m.tile_at(40.0, 20.0).map(|t| t.name.as_str()), Some("ledge"));
        // Falls onto the floor (top at y 48).
        let (_, y, f) = m.move_box(8.0, 30.0, 8.0, 8.0, 0.0, 40.0, false);
        assert_eq!((y, f & hit::GROUND), (44.0, hit::GROUND));
        // Walks into the wall at x 64.
        let (x, _, f) = m.move_box(40.0, 40.0, 8.0, 8.0, 40.0, 0.0, false);
        assert_eq!((x, f & hit::SIDE), (60.0, hit::SIDE));
        // Lands on the ledge from above; jumps up through it from below; drops through it.
        let (_, y, f) = m.move_box(40.0, 8.0, 8.0, 8.0, 0.0, 10.0, false);
        assert_eq!((y, f & hit::GROUND), (12.0, hit::GROUND));
        let (_, y, _) = m.move_box(40.0, 40.0, 8.0, 8.0, 0.0, -30.0, false);
        assert_eq!(y, 10.0);
        let (_, y, _) = m.move_box(40.0, 12.0, 8.0, 8.0, 0.0, 10.0, true);
        assert_eq!(y, 22.0);
        // Standing on the floor is on the ground; touching the spike hurts.
        assert_eq!(m.move_box(8.0, 44.0, 8.0, 8.0, 0.0, 0.0, false).2 & hit::GROUND, hit::GROUND);
        assert_eq!(m.move_box(56.0, 44.0, 8.0, 8.0, 0.0, 0.0, false).2 & hit::HAZARD, hit::HAZARD);
    }
}
