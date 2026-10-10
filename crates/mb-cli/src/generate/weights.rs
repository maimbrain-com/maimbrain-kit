//! Skin weights for hard-surface rigs (`mb model --rigid`, `--pin`), and the
//! rig's own idea of where the model faces.
//!
//! Auto-riggers weight a mesh like skin: every vertex blends a few bones, so
//! armor bends and stretches at the joints like rubber, and anything that
//! hangs (a cape, a skirt, a long binder) is weighted to whatever limb is
//! nearest and stretches when that limb swings. Bolt Brawl's SWORDMASTER's
//! cape, hanging to its heels behind the legs and arms, came back weighted to
//! the calves, thighs and forearms.
//!
//! - `--rigid`: each vertex follows its single strongest bone, so plates move
//!   as plates (`all`, or bone names and groups: `arms`, `legs`, `spine`, `head`).
//! - `--pin cape=spine`, `skirt=hips`, `auto` (both): the vertices that hang
//!   off a limb (far from its bones, more than the limb is thick) are moved to
//!   the named bone, behind the body (`cape`) or below the hips in front and at
//!   the sides (`skirt`), blending over a short band so there's no crease.
//!
//! Everything is in the skin's bind space (joints at their inverse bind
//! matrices), so it works before placement, whatever way the rig points.

use serde_json::{Value, json};

use super::glb::{Mat4, Scene};
use super::shape;

type V3 = [f32; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn len(a: V3) -> f32 {
    dot(a, a).sqrt()
}
fn norm(a: V3) -> V3 {
    let l = len(a).max(1e-9);
    scale(a, 1.0 / l)
}

/// Where an inverse bind matrix puts its joint (the bind pose's joint origin).
fn bind_origin(ibm: &Mat4) -> V3 {
    let m = |r: usize, c: usize| ibm[c * 4 + r];
    let a = [[m(0, 0), m(0, 1), m(0, 2)], [m(1, 0), m(1, 1), m(1, 2)], [m(2, 0), m(2, 1), m(2, 2)]];
    let t = [m(0, 3), m(1, 3), m(2, 3)];
    let det = a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1]) - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0]) + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]);
    if det.abs() < 1e-12 {
        return [0.0; 3];
    }
    let inv = [
        [(a[1][1] * a[2][2] - a[1][2] * a[2][1]) / det, (a[0][2] * a[2][1] - a[0][1] * a[2][2]) / det, (a[0][1] * a[1][2] - a[0][2] * a[1][1]) / det],
        [(a[1][2] * a[2][0] - a[1][0] * a[2][2]) / det, (a[0][0] * a[2][2] - a[0][2] * a[2][0]) / det, (a[0][2] * a[1][0] - a[0][0] * a[1][2]) / det],
        [(a[1][0] * a[2][1] - a[1][1] * a[2][0]) / det, (a[0][1] * a[2][0] - a[0][0] * a[2][1]) / det, (a[0][0] * a[1][1] - a[0][1] * a[1][0]) / det],
    ];
    // p = −A⁻¹ t
    [0, 1, 2].map(|r| -(inv[r][0] * t[0] + inv[r][1] * t[1] + inv[r][2] * t[2]))
}

/// What a joint is, by its name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Leg,
    Arm,
    Head,
    Spine,
    Other,
}

pub fn part_of(name: &str) -> Part {
    let n = name.rsplit(':').next().unwrap_or(name).to_lowercase();
    let has = |w: &[&str]| w.iter().any(|x| n.contains(x));
    if has(&["thigh", "calf", "leg", "knee", "shin", "foot", "toe", "ankle"]) {
        Part::Leg
    } else if has(&["clavicle", "shoulder", "arm", "elbow", "hand", "finger", "thumb", "wrist", "index", "pinky", "middle", "ring"]) {
        Part::Arm
    } else if has(&["head", "neck", "jaw", "eye"]) {
        Part::Head
    } else if has(&["spine", "chest", "waist", "pelvis", "hip", "torso", "abdomen", "belly", "root", "body"]) {
        Part::Spine
    } else {
        Part::Other
    }
}

/// A rigged biped's frame in its bind space, from its joints' names and places.
#[derive(Clone, Debug)]
pub struct Body {
    /// Bind positions of skin 0's joints, by skin index, with their names.
    pub joints: Vec<(String, V3, Part)>,
    pub hips: V3,
    pub up: V3,
    /// Where it faces (horizontal), and what said so.
    pub forward: V3,
    pub forward_by: Vec<&'static str>,
    /// Whether the two ways of telling disagree.
    pub unsure: bool,
}

/// The frame of a named biped rig (hips, legs with feet and toes or sides
/// named L/R), or None. Forward is where its toes point from its ankles, and
/// up × (right side − left side); up is hips → head (else +y).
pub fn body(sc: &Scene) -> Option<Body> {
    let skin = sc.skins.first()?;
    let joints: Vec<(String, V3, Part)> =
        skin.joints.iter().zip(&skin.ibm).map(|(&j, ibm)| (sc.nodes[j].name.clone(), bind_origin(ibm), part_of(&sc.nodes[j].name))).collect();
    let find = |words: &[&str]| joints.iter().find(|(n, _, _)| super::spine::side_of(n).is_none() && words.iter().any(|w| n.to_lowercase().contains(w))).map(|j| j.1);
    let legs: Vec<&(String, V3, Part)> = joints.iter().filter(|j| j.2 == Part::Leg).collect();
    if legs.len() < 2 {
        return None;
    }
    let hips = find(&["pelvis", "hip"]).or_else(|| find(&["spine", "waist"]))?;
    let head = find(&["head"]).or_else(|| find(&["neck"]));
    let up = match head.map(|h| sub(h, hips)).filter(|d| len(*d) > 1e-6) {
        Some(d) => norm(d),
        None => [0.0, 1.0, 0.0],
    };
    let flat = |v: V3| {
        let h = sub(v, scale(up, dot(v, up)));
        (len(h) > 1e-6).then(|| norm(h))
    };
    let mut by = Vec::new();
    // Toes: each ankle/foot joint to its toe joint (same side).
    let mut toes = [0.0f32; 3];
    let mut n_toes = 0;
    for (n, p, _) in &joints {
        let low = n.to_lowercase();
        if !low.contains("toe") {
            continue;
        }
        let side = super::spine::side_of(n).map(|s| s.0);
        if let Some(foot) = joints.iter().find(|(m, _, _)| {
            let ml = m.to_lowercase();
            (ml.contains("foot") || ml.contains("ankle")) && !ml.contains("toe") && super::spine::side_of(m).map(|s| s.0) == side
        }) && let Some(d) = flat(sub(*p, foot.1))
        {
            toes = add(toes, d);
            n_toes += 1;
        }
    }
    let by_toes = (n_toes > 0).then(|| flat(toes)).flatten();
    // Sides: the mean of the right-side joints minus the left's.
    let (mut l, mut r, mut nl, mut nr) = ([0.0f32; 3], [0.0f32; 3], 0, 0);
    for (n, p, _) in &joints {
        match super::spine::side_of(n).map(|s| s.0) {
            Some('l') => {
                l = add(l, *p);
                nl += 1
            }
            Some('r') => {
                r = add(r, *p);
                nr += 1
            }
            _ => {}
        }
    }
    let right = (nl > 0 && nr > 0).then(|| flat(sub(scale(r, 1.0 / nr as f32), scale(l, 1.0 / nl as f32)))).flatten();
    let by_sides = right.map(|r| norm(cross(up, r)));
    let mut unsure = false;
    let forward = match (by_toes, by_sides) {
        (Some(t), Some(s)) => {
            by.extend(["toes", "sides"]);
            if dot(t, s) < 0.5 {
                unsure = true;
            }
            norm(add(t, s))
        }
        (Some(t), None) => {
            by.push("toes");
            t
        }
        (None, Some(s)) => {
            by.push("sides");
            s
        }
        (None, None) => return None,
    };
    Some(Body { joints, hips, up, forward, forward_by: by, unsure })
}

/// Where a rigged biped faces in its rest pose, in the scene's space (its
/// bind-space forward through the skin's bind-to-rest transform): (forward,
/// what said so, whether toes and sides disagree). None without a named biped rig.
pub fn rig_forward(sc: &Scene) -> Option<(V3, Vec<&'static str>, bool)> {
    let b = body(sc)?;
    let skin = sc.skins.first()?;
    let world = sc.world(&shape::pose(sc, None).local);
    let m = super::glb::mul(&world[*skin.joints.first()?], skin.ibm.first()?);
    let f = super::glb::dir(&m, b.forward);
    let flat = [f[0], 0.0, f[2]];
    (len(flat) > 1e-6).then(|| (norm(flat), b.forward_by, b.unsure))
}

/// The axis (`+x`, `-z`…) a direction is nearest to, and the angle about +y
/// that turns it to face +z.
pub fn axis_name(v: V3) -> (&'static str, f32) {
    let yaw = v[0].atan2(v[2]);
    let names = [(0.0, "+z"), (90.0, "+x"), (180.0, "-z"), (-180.0, "-z"), (-90.0, "-x")];
    let deg = yaw.to_degrees();
    let name = names.iter().min_by(|a, b| (a.0 - deg).abs().total_cmp(&(b.0 - deg).abs())).map(|n| n.1).unwrap_or("+z");
    (name, -yaw)
}

/// The bones a `--rigid` list names: `all`, joint names, or groups (`arms`, `legs`, `spine`, `head`).
fn rigid_set(body_joints: &[(String, V3, Part)], spec: &str) -> Result<Vec<bool>, String> {
    let mut set = vec![false; body_joints.len()];
    for w in spec.split(',').map(str::trim).filter(|w| !w.is_empty()) {
        let group = match w.to_lowercase().as_str() {
            "all" | "armor" => Some(None),
            "arms" | "arm" => Some(Some(Part::Arm)),
            "legs" | "leg" => Some(Some(Part::Leg)),
            "spine" | "body" | "torso" => Some(Some(Part::Spine)),
            "head" => Some(Some(Part::Head)),
            _ => None,
        };
        match group {
            Some(g) => body_joints.iter().enumerate().filter(|(_, j)| g.is_none_or(|p| j.2 == p)).for_each(|(i, _)| set[i] = true),
            None => {
                let i = body_joints.iter().position(|j| j.0.eq_ignore_ascii_case(w)).ok_or_else(|| {
                    format!("--rigid {w}: no joint by that name (all, arms, legs, spine, head, or one of {})", body_joints.iter().map(|j| j.0.as_str()).collect::<Vec<_>>().join(", "))
                })?;
                set[i] = true;
            }
        }
    }
    Ok(set)
}

/// One `--pin`: a region and the bone it follows.
#[derive(Clone, Debug, PartialEq)]
pub struct Pin {
    pub region: Region,
    pub bone: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    Cape,
    Skirt,
    Hanging,
}

impl Pin {
    /// `cape=spine`, `skirt=hips`, `hanging=Spine02`, `auto` (cape=spine, skirt=hips).
    pub fn parse(s: &str) -> Result<Vec<Pin>, String> {
        if s.trim().eq_ignore_ascii_case("auto") {
            return Ok(vec![Pin { region: Region::Cape, bone: "spine".into() }, Pin { region: Region::Skirt, bone: "hips".into() }]);
        }
        let (r, b) = s.split_once('=').ok_or_else(|| format!("--pin {s:?}: write cape=spine, skirt=hips, hanging=<bone> or auto"))?;
        let region = match r.trim().to_lowercase().as_str() {
            "cape" | "binder" | "cloak" | "back" => Region::Cape,
            "skirt" | "tassets" | "coat" => Region::Skirt,
            "hanging" | "all" => Region::Hanging,
            o => return Err(format!("--pin {o}: the regions are cape (hanging behind the body), skirt (hanging below the hips) and hanging (both)")),
        };
        let bone = b.trim().to_string();
        if bone.is_empty() {
            return Err(format!("--pin {s:?}: name the bone (spine, chest, hips or a joint name)"));
        }
        Ok(vec![Pin { region, bone }])
    }
}

/// The joint a `--pin` bone names: `spine`/`chest` (the top spine joint below the neck), `hips`, or a joint.
fn pin_joint(body: &Body, bone: &str) -> Result<usize, String> {
    let j = &body.joints;
    let center = |i: &usize| super::spine::side_of(&j[*i].0).is_none();
    let height = |i: usize| dot(sub(j[i].1, body.hips), body.up);
    let named = |w: &str| (0..j.len()).find(|i| center(i) && j[*i].0.to_lowercase().contains(w));
    let pick = match bone.to_lowercase().as_str() {
        "spine" | "chest" | "back" | "upper_spine" => (0..j.len()).filter(|i| j[*i].2 == Part::Spine && center(i) && j[*i].0.to_lowercase().contains("spine")).max_by(|a, b| height(*a).total_cmp(&height(*b))),
        // The hips: the joint above both the legs and the spine (Tripo's Hip, not its Pelvis).
        "hips" | "hip" => (0..j.len()).find(|i| center(i) && j[*i].0.eq_ignore_ascii_case("hip")).or_else(|| named("hips")).or_else(|| named("pelvis")).or_else(|| named("hip")),
        "pelvis" => named("pelvis").or_else(|| named("hip")),
        "waist" => named("waist"),
        _ => (0..j.len()).find(|i| j[*i].0.eq_ignore_ascii_case(bone)),
    };
    pick.ok_or_else(|| format!("--pin …={bone}: no such joint (spine, chest, hips, waist, or one of {})", j.iter().map(|x| x.0.as_str()).collect::<Vec<_>>().join(", ")))
}

/// What a pinned vertex follows: one bone, or (`spine`) the spine chain from
/// the hips up, by the vertex's height, so a long cape hangs from the upper
/// back and its lower part follows the hips instead of swinging out like a
/// board when the chest bends.
#[derive(Clone, Debug)]
enum Target {
    Bone(usize),
    /// (joint, height above the hips), lowest first.
    Chain(Vec<(usize, f32)>),
}

impl Target {
    fn of(body: &Body, bone: &str) -> Result<Target, String> {
        let top = pin_joint(body, bone)?;
        if !matches!(bone.to_lowercase().as_str(), "spine" | "chest" | "back" | "upper_spine") {
            return Ok(Target::Bone(top));
        }
        let hips = pin_joint(body, "hips")?;
        let height = |i: usize| dot(sub(body.joints[i].1, body.hips), body.up);
        let mut chain: Vec<(usize, f32)> = (0..body.joints.len())
            .filter(|&i| super::spine::side_of(&body.joints[i].0).is_none() && body.joints[i].0.to_lowercase().contains("spine") && height(i) <= height(top) + 1e-6)
            .map(|i| (i, height(i)))
            .collect();
        chain.push((hips, height(hips).min(chain.iter().map(|c| c.1).fold(f32::MAX, f32::min) - 1e-4)));
        chain.sort_by(|a, b| a.1.total_cmp(&b.1));
        chain.dedup_by(|a, b| (a.1 - b.1).abs() < 1e-5);
        Ok(Target::Chain(chain))
    }

    /// The top joint (what the run reports).
    fn top(&self) -> usize {
        match self {
            Target::Bone(b) => *b,
            Target::Chain(c) => c.last().map(|x| x.0).unwrap_or(0),
        }
    }

    /// The weights a vertex at height `h` (above the hips) gets.
    fn weights(&self, h: f32) -> Vec<(usize, f32)> {
        match self {
            Target::Bone(b) => vec![(*b, 1.0)],
            Target::Chain(c) => {
                if h <= c[0].1 {
                    return vec![(c[0].0, 1.0)];
                }
                for w in c.windows(2) {
                    if h <= w[1].1 {
                        let s = (h - w[0].1) / (w[1].1 - w[0].1).max(1e-6);
                        return vec![(w[0].0, 1.0 - s), (w[1].0, s)];
                    }
                }
                vec![(c[c.len() - 1].0, 1.0)]
            }
        }
    }
}

/// Distance from `p` to the segment a–b.
fn seg_dist(p: V3, a: V3, b: V3) -> f32 {
    let ab = sub(b, a);
    let t = (dot(sub(p, a), ab) / dot(ab, ab).max(1e-12)).clamp(0.0, 1.0);
    len(sub(p, add(a, scale(ab, t))))
}

/// The bone segments of each limb chain (a joint to each of its child joints), by side and part.
fn limb_segments(sc: &Scene, body: &Body) -> Vec<(char, Part, V3, V3)> {
    let skin = &sc.skins[0];
    let mut out = Vec::new();
    for (k, &j) in skin.joints.iter().enumerate() {
        let (name, p, part) = &body.joints[k];
        if !matches!(part, Part::Leg | Part::Arm) {
            continue;
        }
        let side = super::spine::side_of(name).map(|s| s.0).unwrap_or('c');
        let kids: Vec<usize> = sc.nodes[j].children.iter().filter_map(|c| skin.joints.iter().position(|x| x == c)).collect();
        for c in kids {
            if len(sub(body.joints[c].1, *p)) > 1e-5 {
                out.push((side, *part, *p, body.joints[c].1));
            }
        }
    }
    out
}

/// How a vertex hangs: (how far past its limb's thickness, 0 = on the limb;
/// behind the body?, below the hips?) for vertices led by a limb.
struct Hang {
    over: f32,
    behind: bool,
    below_hips: bool,
}

/// `--rigid` and `--pin` on skin 0's meshes. Returns facts (vertices changed,
/// the bones pinned to, the stretch before and after on the clips) and notes.
pub fn fix(sc: &mut Scene, rigid: Option<&str>, pins: &[String]) -> Result<(Value, Vec<String>), String> {
    let mut notes = Vec::new();
    if rigid.is_none() && pins.is_empty() {
        return Ok((Value::Null, notes));
    }
    let Some(body) = body(sc) else {
        return Err("--rigid and --pin need a rig with named legs (a biped from --rig); this model has none".into());
    };
    let pins: Vec<Pin> = pins.iter().map(|p| Pin::parse(p)).collect::<Result<Vec<_>, _>>()?.into_iter().flatten().collect();
    let targets: Vec<(Region, Target)> = pins.iter().map(|p| Target::of(&body, &p.bone).map(|t| (p.region, t))).collect::<Result<_, _>>()?;
    let rigid = rigid.map(|r| rigid_set(&body.joints, r)).transpose()?;
    let parts: Vec<Part> = body.joints.iter().map(|j| j.2).collect();
    let segs = limb_segments(sc, &body);
    let before = sc.clone();
    // Limb thickness: the median distance from the bones of the vertices each limb chain leads.
    let lead = |jj: &[u16; 4], ww: &[f32; 4]| -> Option<usize> { (0..4).filter(|&k| ww[k] > 0.0).max_by(|&a, &b| ww[a].total_cmp(&ww[b])).map(|k| jj[k] as usize) };
    let limb_of = |j: usize| -> Option<(char, Part)> {
        let p = parts.get(j)?;
        matches!(p, Part::Leg | Part::Arm).then(|| (super::spine::side_of(&body.joints[j].0).map(|s| s.0).unwrap_or('c'), *p))
    };
    let dist_to = |limb: (char, Part), p: V3| segs.iter().filter(|s| (s.0, s.1) == limb).map(|s| seg_dist(p, s.2, s.3)).fold(f32::MAX, f32::min);
    let mut radii: std::collections::HashMap<(char, Part), Vec<f32>> = Default::default();
    let skinned: Vec<(usize, usize)> =
        sc.nodes.iter().filter(|n| n.skin == Some(0)).filter_map(|n| n.mesh).flat_map(|m| (0..sc.meshes[m].prims.len()).map(move |p| (m, p))).collect();
    for &(m, pi) in &skinned {
        let p = &sc.meshes[m].prims[pi];
        let (Some(jj), Some(ww)) = (&p.joints, &p.weights) else { continue };
        for v in 0..p.pos.len() {
            if let Some(limb) = lead(&jj[v], &ww[v]).and_then(limb_of) {
                radii.entry(limb).or_default().push(dist_to(limb, p.pos[v]));
            }
        }
    }
    let radius: std::collections::HashMap<(char, Part), f32> = radii
        .into_iter()
        .map(|(k, mut v)| {
            v.sort_by(f32::total_cmp);
            (k, v[v.len() * 2 / 5].max(1e-4))
        })
        .collect();
    // The torso's back: how far behind the hips the spine-led vertices reach (their 10th percentile).
    let mut backs: Vec<f32> = Vec::new();
    for &(m, pi) in &skinned {
        let p = &sc.meshes[m].prims[pi];
        let (Some(jj), Some(ww)) = (&p.joints, &p.weights) else { continue };
        for v in 0..p.pos.len() {
            if lead(&jj[v], &ww[v]).is_some_and(|j| parts[j] == Part::Spine) {
                backs.push(dot(sub(p.pos[v], body.hips), body.forward));
            }
        }
    }
    backs.sort_by(f32::total_cmp);
    let back = backs.get(backs.len() / 10).copied().unwrap_or(0.0);
    let hang_of = |p: V3, jj: &[u16; 4], ww: &[f32; 4]| -> Option<Hang> {
        let limb = lead(jj, ww).and_then(limb_of)?;
        let r = radius.get(&limb).copied().unwrap_or(0.05);
        let d = dist_to(limb, p);
        let rel = sub(p, body.hips);
        let behind = dot(rel, body.forward) < back;
        let below_hips = dot(rel, body.up) < 0.0;
        // An arm's own armor (pauldrons, gauntlets) stays with the arm; what an arm leads behind the back hangs.
        if limb.1 == Part::Arm && !behind {
            return None;
        }
        // 1.8× the limb's thickness: hanging; 2.6×: fully pinned (a band between, no crease).
        Some(Hang { over: ((d / r - 1.8) / 0.8).clamp(0.0, 1.0), behind, below_hips })
    };
    let mut pinned = vec![0usize; targets.len()];
    let mut snapped = 0usize;
    let mut moved: Vec<Vec<Vec<bool>>> = sc.meshes.iter().map(|m| m.prims.iter().map(|p| vec![false; p.pos.len()]).collect()).collect();
    for &(m, pi) in &skinned {
        let p = &mut sc.meshes[m].prims[pi];
        let pos = p.pos.clone();
        let (Some(jj), Some(ww)) = (p.joints.as_mut(), p.weights.as_mut()) else { continue };
        for v in 0..pos.len() {
            let total: f32 = ww[v].iter().sum::<f32>().max(1e-9);
            let mut w: Vec<(usize, f32)> = (0..4).filter(|&k| ww[v][k] > 0.0).map(|k| (jj[v][k] as usize, ww[v][k] / total)).collect();
            // --rigid: the strongest bone alone.
            if let (Some(set), Some(&(top, _))) = (&rigid, w.iter().max_by(|a, b| a.1.total_cmp(&b.1)))
                && set.get(top).copied().unwrap_or(false)
                && w.len() > 1
            {
                w = vec![(top, 1.0)];
                snapped += 1;
            }
            // --pin: hanging vertices blend onto the pinned bone.
            if let Some(h) = hang_of(pos[v], &jj[v], &ww[v]).filter(|h| h.over > 0.0) {
                for (t, (region, target)) in targets.iter().enumerate() {
                    let hit = match region {
                        Region::Cape => h.behind,
                        Region::Skirt => h.below_hips && !h.behind,
                        Region::Hanging => h.behind || h.below_hips,
                    };
                    if !hit {
                        continue;
                    }
                    let s = h.over;
                    for x in w.iter_mut() {
                        x.1 *= 1.0 - s;
                    }
                    for (bone, tw) in target.weights(dot(sub(pos[v], body.hips), body.up)) {
                        match w.iter_mut().find(|x| x.0 == bone) {
                            Some(x) => x.1 += s * tw,
                            None => w.push((bone, s * tw)),
                        }
                    }
                    pinned[t] += 1;
                    moved[m][pi][v] = s > 0.99;
                    break;
                }
            }
            w.sort_by(|a, b| b.1.total_cmp(&a.1));
            w.truncate(4);
            let sum: f32 = w.iter().map(|x| x.1).sum::<f32>().max(1e-9);
            let mut nj = [0u16; 4];
            let mut nw = [0f32; 4];
            for (k, (j, x)) in w.iter().enumerate() {
                nj[k] = *j as u16;
                nw[k] = x / sum;
            }
            jj[v] = nj;
            ww[v] = nw;
        }
    }
    let mut facts = json!({ "rigid": rigid.as_ref().map(|_| json!({ "vertices": snapped })) });
    let mut pin_facts = Vec::new();
    for (t, (region, target)) in targets.iter().enumerate() {
        let chain = match target {
            Target::Chain(c) => json!(c.iter().map(|x| body.joints[x.0].0.clone()).collect::<Vec<_>>()),
            Target::Bone(_) => Value::Null,
        };
        pin_facts.push(json!({ "region": format!("{region:?}").to_lowercase(), "bone": body.joints[target.top()].0, "chain": chain, "vertices": pinned[t] }));
        if pinned[t] == 0 {
            notes.push(format!("--pin {}: nothing hangs there (no vertex led by an arm or leg reaches well past it)", format!("{region:?}").to_lowercase()));
        }
    }
    facts["pin"] = json!(pin_facts);
    // How much the pinned vertices' edges stretch on the clips, before and after.
    if !sc.anims.is_empty() && pinned.iter().sum::<usize>() > 0 {
        let b = stretch(&before, &moved);
        let a = stretch(sc, &moved);
        facts["stretch"] = json!({ "before": b.facts(), "after": a.facts() });
        notes.push(format!(
            "pinned {} hanging vertices; over the clips {:.0}% of their edges stretched or squashed past 25% before, {:.0}% now (90th percentile {:.2}× → {:.2}×)",
            pinned.iter().sum::<usize>(),
            b.share * 100.0,
            a.share * 100.0,
            b.p90,
            a.p90
        ));
    }
    Ok((facts, notes))
}

/// How much the edges between `marked` vertices distort (posed length over
/// rest length, or the inverse; the worst of 12 frames of each clip).
pub fn stretch(sc: &Scene, marked: &[Vec<Vec<bool>>]) -> Stretch {
    let rest = shape::posed(sc, &shape::pose(sc, None));
    let mut worst: std::collections::HashMap<(usize, usize, u32, u32), f32> = Default::default();
    let edges: Vec<(usize, usize, u32, u32)> = rest
        .iter()
        .flat_map(|pr| {
            let p = &sc.meshes[pr.mesh].prims[pr.prim];
            let mk = &marked[pr.mesh][pr.prim];
            p.indices.chunks(3).flat_map(move |t| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])]).filter(move |(a, b)| mk[*a as usize] && mk[*b as usize]).map(move |(a, b)| (pr.mesh, pr.prim, a.min(b), a.max(b)))
        })
        .collect();
    for a in &sc.anims {
        let d = a.duration();
        for k in 0..12 {
            let posed = shape::posed(sc, &shape::pose(sc, Some((a, d * k as f32 / 11.0))));
            for &(m, pi, i, j) in &edges {
                let (Some(r), Some(q)) = (rest.iter().find(|x| x.mesh == m && x.prim == pi), posed.iter().find(|x| x.mesh == m && x.prim == pi)) else { continue };
                let l0 = len(sub(r.pos[i as usize], r.pos[j as usize]));
                if l0 < 1e-6 {
                    continue;
                }
                let l1 = len(sub(q.pos[i as usize], q.pos[j as usize]));
                let e = worst.entry((m, pi, i, j)).or_insert(1.0);
                *e = e.max(l1 / l0).max(l0 / l1.max(1e-9));
            }
        }
    }
    let mut v: Vec<f32> = worst.into_values().collect();
    if v.is_empty() {
        return Stretch { median: 1.0, p90: 1.0, share: 0.0 };
    }
    v.sort_by(f32::total_cmp);
    let at = |q: usize| v[(v.len() * q / 100).min(v.len() - 1)];
    Stretch { median: at(50), p90: at(90), share: v.iter().filter(|&&x| x > 1.25).count() as f32 / v.len() as f32 }
}

/// How much edges distort over the clips (posed length over rest length, or
/// the inverse when shorter; worst frame per edge).
#[derive(Clone, Copy, Debug)]
pub struct Stretch {
    pub median: f32,
    pub p90: f32,
    /// The share of edges stretched or squashed more than 25 %.
    pub share: f32,
}

impl Stretch {
    fn facts(&self) -> Value {
        let r = |x: f32| (x * 100.0).round() / 100.0;
        json!({ "median": r(self.median), "p90": r(self.p90), "share_over_25pct": r(self.share) })
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::generate::glb::{self, Anim, Channel, Node, Path, Prim, Skin};

    /// A stick biped facing +x (as Tripo's mechs do: sides along ±z), a box
    /// on each thigh and a cape panel hanging behind, weighted to the thighs.
    pub fn biped() -> Scene {
        let mut sc = glb::tests::cube_scene();
        // Joints: Hip (0), L_Thigh, L_Calf, L_Foot, L_ToeBase, the same on the right, Spine01, Spine02, Head.
        let at: [(&str, V3, Option<usize>); 12] = [
            ("Hip", [0.0, 1.0, 0.0], None),
            ("L_Thigh", [0.0, 1.0, -0.2], Some(0)),
            ("L_Calf", [0.0, 0.5, -0.2], Some(1)),
            ("L_Foot", [-0.05, 0.05, -0.2], Some(2)),
            ("L_ToeBase", [0.15, 0.0, -0.2], Some(3)),
            ("R_Thigh", [0.0, 1.0, 0.2], Some(0)),
            ("R_Calf", [0.0, 0.5, 0.2], Some(5)),
            ("R_Foot", [-0.05, 0.05, 0.2], Some(6)),
            ("R_ToeBase", [0.15, 0.0, 0.2], Some(7)),
            ("Spine01", [0.0, 1.2, 0.0], Some(0)),
            ("Spine02", [0.0, 1.5, 0.0], Some(9)),
            ("Head", [0.0, 1.8, 0.0], Some(10)),
        ];
        let mut nodes: Vec<Node> = Vec::new();
        for (k, (name, p, parent)) in at.iter().enumerate() {
            let pp = parent.map(|q| at[q].1).unwrap_or([0.0; 3]);
            nodes.push(Node { name: name.to_string(), t: sub(*p, pp), ..Node::default() });
            if let Some(q) = parent {
                nodes[*q].children.push(k);
            }
        }
        nodes.push(Node { name: "body".into(), mesh: Some(0), skin: Some(0), ..Node::default() });
        let ibm = at.iter().map(|(_, p, _)| glb::trs(scale(*p, -1.0), [0.0, 0.0, 0.0, 1.0], [1.0; 3])).collect();
        sc.skins = vec![Skin { name: "Armature".into(), joints: (0..12).collect(), ibm, skeleton: Some(0) }];
        sc.roots = vec![0, 12];
        // Thigh skin (near the bones) and a cape panel 0.4 behind the legs (−x), both led by the thighs.
        let mut prim = Prim { material: Some(0), ..Prim::default() };
        let (mut pos, mut jj, mut ww, mut idx) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        // Each leg: three vertices around the bone per row (the leg's own skin).
        for (side, thigh, calf) in [(-0.2f32, 1u16, 2u16), (0.2, 5, 6)] {
            for i in 0..=4 {
                let y = 1.0 - 0.25 * i as f32;
                let w = i as f32 / 4.0;
                for (x, z) in [(0.05, side - 0.05), (0.05, side + 0.05), (-0.05, side)] {
                    pos.push([x, y, z]);
                    jj.push([thigh, calf, 0, 0]);
                    ww.push([1.0 - w, w, 0.0, 0.0]);
                }
            }
            let a = pos.len() as u32 - 15;
            for i in 0..4u32 {
                idx.extend([a + i * 3, a + i * 3 + 1, a + i * 3 + 3]);
            }
        }
        // The cape: one sheet behind both legs (4 columns × 5 rows), each half led by its leg.
        let c0 = pos.len() as u32;
        for i in 0..=4 {
            let y = 1.0 - 0.25 * i as f32;
            let w = i as f32 / 4.0;
            for z in [-0.3f32, -0.1, 0.1, 0.3] {
                let (thigh, calf) = if z < 0.0 { (1, 2) } else { (5, 6) };
                pos.push([-0.4, y, z]);
                jj.push([thigh, calf, 0, 0]);
                ww.push([1.0 - w, w, 0.0, 0.0]);
            }
        }
        for i in 0..4u32 {
            for c in 0..3u32 {
                let a = c0 + i * 4 + c;
                idx.extend([a, a + 1, a + 4, a + 1, a + 5, a + 4]);
            }
        }
        prim.pos = pos;
        prim.joints = Some(jj);
        prim.weights = Some(ww);
        prim.indices = idx;
        sc.meshes = vec![glb::Mesh { name: "body".into(), prims: vec![prim], weights: vec![] }];
        sc.nodes = nodes;
        // A big kick: the left thigh swings 70° forward.
        let q = |a: f32| [0.0, 0.0, (a / 2.0).sin(), (a / 2.0).cos()];
        sc.anims = vec![Anim { name: "Kick".into(), channels: vec![Channel { node: 1, path: Path::R, interp: "LINEAR".into(), input: vec![0.0, 1.0], output: [q(0.0), q(1.2)].concat() }] }];
        sc
    }

    #[test]
    fn a_rig_says_which_way_it_faces() {
        let sc = biped();
        let b = body(&sc).unwrap();
        assert!(b.forward[0] > 0.99, "faces +x: {:?}", b.forward);
        assert_eq!(b.forward_by, ["toes", "sides"]);
        assert!(!b.unsure);
        assert_eq!(axis_name(b.forward).0, "+x");
        assert!((axis_name(b.forward).1 + std::f32::consts::FRAC_PI_2).abs() < 1e-3, "turned −90° to face +z");
        assert_eq!(part_of("L_ForearmTwist01"), Part::Arm);
        assert_eq!(part_of("R_ToeBase"), Part::Leg);
        assert_eq!(part_of("NeckTwist02"), Part::Head);
    }

    #[test]
    fn a_hanging_cape_is_pinned_to_the_spine_and_stops_stretching() {
        let mut sc = biped();
        let (facts, notes) = fix(&mut sc, None, &["cape=spine".into()]).unwrap();
        assert_eq!(facts["pin"][0]["bone"], "Spine02");
        assert_eq!(facts["pin"][0]["vertices"], 20, "the cape's 2 × 2 × 5 vertices, not the thighs': {facts}");
        let (b, a) = (facts["stretch"]["before"]["p90"].as_f64().unwrap(), facts["stretch"]["after"]["p90"].as_f64().unwrap());
        assert!(b > 1.3 && a < b, "{b} → {a}");
        assert!(notes[0].contains("pinned 20"), "{notes:?}");
        // The thigh's own skin stays on the thigh.
        let p = &sc.meshes[0].prims[0];
        assert_eq!(p.joints.as_ref().unwrap()[0][0], 1);
        // --rigid: one bone per vertex.
        let mut sc = biped();
        let (facts, _) = fix(&mut sc, Some("legs"), &[]).unwrap();
        assert!(facts["rigid"]["vertices"].as_u64().unwrap() > 0);
        let p = &sc.meshes[0].prims[0];
        assert!(p.weights.as_ref().unwrap().iter().all(|w| w[1] == 0.0 && w[0] == 1.0));
        assert!(fix(&mut biped(), Some("tail"), &[]).unwrap_err().contains("no joint"));
        assert!(Pin::parse("cape").is_err() && Pin::parse("auto").unwrap().len() == 2);
    }
}
