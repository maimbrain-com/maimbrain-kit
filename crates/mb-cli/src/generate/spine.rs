//! `mb model --spine`: bones for any generated mesh. Replaces whatever rig a
//! model has (none, or one mb3d can't use) with a chain of joints along the
//! model's length, head first (`spine_0` at the head … `spine_N-1` at the
//! tail), with smooth weights, so the SDK's `Skeleton::spine` + `Swim`/`Wave`
//! bend it like the koi built in code. Birds also get a two-bone wing on each
//! side (`wing_l_0`/`wing_l_1`, `wing_r_0`/`wing_r_1`, hanging from a node
//! named `wings`) for `gfx3d::Flight` to flap.
//!
//! The mesh is baked into the placed model's space (so the root node is the
//! identity), its nodes are named `<asset>/body`, `<asset>/body2`…, and the
//! old skin and clips are dropped (they moved the old bones).
//!
//! Also here: the rig report every rigged model gets (joint count, tree,
//! mirror check) and the leaf-merging simplifier for rigs over 64 joints.

use serde_json::{Value, json};

use super::glb::{self, Mat4, Prim, Scene};
use super::shape;

/// How `--spine` lays out the joints.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    clap::ValueEnum,
    Default,
)]
#[serde(rename_all = "kebab-case")]
pub enum SpineMode {
    /// Keep the provider's rig (default).
    #[default]
    Off,
    /// Head to tail along the facing axis, plus wings when it looks like a bird
    /// (an avian rig type or a bird/wing prompt, and about as wide as it is long).
    Auto,
    /// Head to tail along the facing axis, no wings.
    HeadToTail,
    /// Head to tail plus two-bone wings, whatever its shape.
    Wings,
}

/// Words that make `--spine auto` look for wings.
const BIRD_WORDS: &[&str] = &[
    "bird", "wing", "heron", "crane", "eagle", "hawk", "owl", "gull", "bat", "dragon", "phoenix",
    "crow", "raven", "egret", "stork", "swan", "duck", "goose", "parrot", "falcon",
];

pub fn wants_wings(mode: SpineMode, rig_type: Option<&str>, subject: &str, ext: [f32; 3]) -> bool {
    match mode {
        SpineMode::Off | SpineMode::HeadToTail => false,
        SpineMode::Wings => true,
        SpineMode::Auto => {
            let s = subject.to_lowercase();
            let birdish = rig_type == Some("avian") || BIRD_WORDS.iter().any(|w| s.contains(w));
            // Spread wings make it about as wide as it is long; a serpent dragon isn't.
            birdish && ext[0] >= 0.7 * ext[2]
        }
    }
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = sub(a, b);
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
}

fn translation(t: [f32; 3]) -> Mat4 {
    glb::trs(t, [0.0, 0.0, 0.0, 1.0], [1.0; 3])
}

/// Bakes the placed model, lays a spine (and wings) through it and skins it.
/// `head_neg_z`: the head is at −z (the model faces −z). Returns facts.
pub fn add_spine(
    sc: &mut Scene,
    name: &str,
    joints: usize,
    wings: bool,
    head_neg_z: bool,
) -> Result<Value, String> {
    let joints = joints.clamp(2, 32);
    let posed = shape::posed(sc, &shape::pose(sc, None));
    let (lo, hi) = shape::bounds(&posed).ok_or("the model has no triangles")?;
    let all: Vec<[f32; 3]> = posed.iter().flat_map(|p| p.pos.iter().copied()).collect();
    let half_span = (-lo[0]).max(hi[0]).max(1e-6);
    let shoulder = 0.16 * half_span;
    let elbow = 0.55 * half_span;
    let in_body = |p: &[f32; 3]| !wings || p[0].abs() < shoulder;
    // The body's long axis in the side plane (y–z): along z for a fish or a serpent, near
    // vertical for a bird sculpted standing with its wings spread.
    let body: Vec<[f32; 3]> = all.iter().copied().filter(|p| in_body(p)).collect();
    let body = if body.len() < 8 { all.clone() } else { body };
    let nb = body.len() as f32;
    let mean = [
        body.iter().map(|p| p[0]).sum::<f32>() / nb,
        body.iter().map(|p| p[1]).sum::<f32>() / nb,
        body.iter().map(|p| p[2]).sum::<f32>() / nb,
    ];
    let (mut cyy, mut cyz, mut czz) = (0.0f32, 0.0f32, 0.0f32);
    for p in &body {
        let (y, z) = (p[1] - mean[1], p[2] - mean[2]);
        cyy += y * y;
        cyz += y * z;
        czz += z * z;
    }
    let th = 0.5 * (2.0 * cyz).atan2(czz - cyy);
    let mut axis = [0.0, th.sin(), th.cos()];
    let along = |p: &[f32; 3], a: [f32; 3]| (p[1] - mean[1]) * a[1] + (p[2] - mean[2]) * a[2];
    // The head is the end that reaches furthest the way the model faces (a beak, a snout).
    let (smin, smax) = body.iter().fold((f32::MAX, f32::MIN), |(a, b), p| {
        (a.min(along(p, axis)), b.max(along(p, axis)))
    });
    let end_z = |lo_end: bool| {
        let cut = smin + (smax - smin) * if lo_end { 0.12 } else { 0.88 };
        let sel: Vec<f32> = body
            .iter()
            .filter(|p| {
                if lo_end {
                    along(p, axis) <= cut
                } else {
                    along(p, axis) >= cut
                }
            })
            .map(|p| p[2])
            .collect();
        sel.iter().sum::<f32>() / sel.len().max(1) as f32
    };
    let lo_forward = if head_neg_z {
        end_z(true) < end_z(false)
    } else {
        end_z(true) > end_z(false)
    };
    if lo_forward {
        axis = [0.0, -axis[1], -axis[2]];
    }
    // Now the head is at the high end of `along`.
    let (z0, z1) = {
        let (a, b) = body.iter().fold((f32::MAX, f32::MIN), |(a, b), p| {
            (a.min(along(p, axis)), b.max(along(p, axis)))
        });
        (b, a)
    };
    // Joints at the centroid of each slice of the body, so the spine follows a raised neck or a curved tail.
    let step = (z1 - z0) / (joints - 1) as f32;
    let at_s = |sv: f32| [mean[0], mean[1] + axis[1] * sv, mean[2] + axis[2] * sv];
    let mut pts: Vec<Option<[f32; 3]>> = (0..joints)
        .map(|i| {
            let sv = z0 + step * i as f32;
            let band = step.abs() * 0.6;
            let (mut c, mut n) = ([0.0f32; 3], 0usize);
            for p in body.iter().filter(|p| (along(p, axis) - sv).abs() <= band) {
                (0..3).for_each(|k| c[k] += p[k]);
                n += 1;
            }
            (n > 0).then(|| {
                // Keep the joint on its slice's plane (centroid across it, not along it).
                let c = [c[0] / n as f32, c[1] / n as f32, c[2] / n as f32];
                let off = along(&c, axis) - sv;
                [c[0], c[1] - axis[1] * off, c[2] - axis[2] * off]
            })
        })
        .collect();
    // Fill empty slices from their neighbours (a thin tail tip), else the axis itself.
    for i in 0..joints {
        if pts[i].is_none() {
            let sv = z0 + step * i as f32;
            let near = (1..joints).find_map(|d| {
                pts.get(i.wrapping_sub(d))
                    .copied()
                    .flatten()
                    .or_else(|| pts.get(i + d).copied().flatten())
            });
            let base = at_s(sv);
            pts[i] = Some(near.map_or(base, |p| {
                let off = along(&p, axis) - sv;
                [p[0], p[1] - axis[1] * off, p[2] - axis[2] * off]
            }));
        }
    }
    let spine: Vec<[f32; 3]> = pts.into_iter().map(Option::unwrap).collect();
    // Wing joints: the shoulder and elbow bands' centroids on each side.
    let band_centroid = |side: f32, x0: f32, x1: f32| -> Option<[f32; 3]> {
        let (mut s, mut n) = ([0.0f32; 3], 0usize);
        for p in all
            .iter()
            .filter(|p| p[0] * side >= x0 && p[0] * side <= x1)
        {
            (0..3).for_each(|k| s[k] += p[k]);
            n += 1;
        }
        (n > 0).then(|| [s[0] / n as f32, s[1] / n as f32, s[2] / n as f32])
    };
    // (name, position, parent: index into `bones` or None for the root / the wings node)
    let mut bones: Vec<(String, [f32; 3], Option<usize>)> = spine
        .iter()
        .enumerate()
        .map(|(i, &p)| (format!("spine_{i}"), p, i.checked_sub(1)))
        .collect();
    let mut wing_parent = None;
    let mut wing_bones = Vec::new();
    if wings {
        // One symmetric pair: the two sides' shoulder and elbow bands averaged (a generated bird's
        // wings are rarely sculpted alike, and a lopsided rig flaps lopsided).
        let pair = |x0: f32, x1: f32| -> Option<[f32; 3]> {
            let (l, r) = (band_centroid(-1.0, x0, x1)?, band_centroid(1.0, x0, x1)?);
            Some([
                (r[0] - l[0]) / 2.0,
                (l[1] + r[1]) / 2.0,
                (l[2] + r[2]) / 2.0,
            ])
        };
        let s0 = pair(shoulder * 0.8, shoulder * 1.4).ok_or(
            "no wing on one side: is it a bird with spread wings? (use --spine head-to-tail)",
        )?;
        let s1 = pair(elbow * 0.9, elbow * 1.15).unwrap_or([elbow, s0[1], s0[2]]);
        let k = (0..joints)
            .min_by(|&a, &b| dist(spine[a], s0).total_cmp(&dist(spine[b], s0)))
            .unwrap();
        wing_parent = Some(k);
        for (side, tag) in [(-1.0f32, "l"), (1.0, "r")] {
            let i0 = bones.len();
            bones.push((
                format!("wing_{tag}_0"),
                [side * shoulder, s0[1], s0[2]],
                None,
            ));
            bones.push((
                format!("wing_{tag}_1"),
                [side * elbow, s1[1], s1[2]],
                Some(i0),
            ));
            wing_bones.push(i0);
        }
    }
    // Weights: a soft spread over the nearest spine joints (σ ≈ 0.6 joint), blended into the wing bones by |x|.
    let weigh = |p: [f32; 3]| -> ([u16; 4], [f32; 4]) {
        let t = ((along(&p, axis) - z0) / (z1 - z0)).clamp(0.0, 1.0) * (joints - 1) as f32;
        let mut ws: Vec<(usize, f32)> = (0..joints)
            .map(|j| (j, (-((t - j as f32) / 0.6).powi(2)).exp()))
            .collect();
        if wings {
            let side = if p[0] < 0.0 { 0 } else { 1 };
            let a = smoothstep(shoulder * 0.7, shoulder * 1.6, p[0].abs());
            let b = smoothstep(elbow * 0.8, elbow * 1.2, p[0].abs());
            let total: f32 = ws.iter().map(|w| w.1).sum();
            ws.iter_mut().for_each(|w| w.1 *= (1.0 - a) / total);
            let i0 = joints + side * 2;
            ws.push((i0, a * (1.0 - b)));
            ws.push((i0 + 1, a * b));
        }
        ws.sort_by(|x, y| y.1.total_cmp(&x.1));
        ws.truncate(4);
        let sum: f32 = ws.iter().map(|w| w.1).sum::<f32>().max(1e-9);
        let mut j = [0u16; 4];
        let mut w = [0.0f32; 4];
        for (k, (ji, wi)) in ws.into_iter().enumerate() {
            j[k] = ji as u16;
            w[k] = wi / sum;
        }
        (j, w)
    };
    // The new scene: the root (identity) → the baked mesh nodes + the bones.
    let world = sc.world(&shape::pose(sc, None).local);
    let mut nodes = vec![glb::Node {
        name: name.to_string(),
        ..Default::default()
    }];
    let mut meshes = Vec::new();
    // In walk order, as `posed` lists them.
    let drawn: Vec<usize> = sc
        .walk()
        .into_iter()
        .map(|(n, _)| n)
        .filter(|&n| sc.nodes[n].mesh.is_some())
        .collect();
    let mut pi = 0;
    for (k, &n) in drawn.iter().enumerate() {
        let mi = sc.nodes[n].mesh.unwrap();
        let mesh = &sc.meshes[mi];
        let m = &world[n];
        let mut prims = Vec::new();
        for prim in &mesh.prims {
            let baked = &posed[pi];
            pi += 1;
            let (jj, ww): (Vec<[u16; 4]>, Vec<[f32; 4]>) =
                baked.pos.iter().map(|&p| weigh(p)).unzip();
            prims.push(Prim {
                pos: baked.pos.clone(),
                nrm: Some(baked.nrm.clone()),
                tan: prim.tan.as_ref().map(|t| {
                    t.iter()
                        .map(|v| {
                            let d = glb::normalize(glb::dir(m, [v[0], v[1], v[2]]));
                            [d[0], d[1], d[2], v[3]]
                        })
                        .collect()
                }),
                uv: prim.uv.clone(),
                color: prim.color.clone(),
                joints: Some(jj),
                weights: Some(ww),
                targets: prim
                    .targets
                    .iter()
                    .map(|t| glb::Target {
                        pos: t.pos.iter().map(|d| glb::dir(m, *d)).collect(),
                        nrm: t.nrm.clone(),
                    })
                    .collect(),
                indices: prim.indices.clone(),
                material: prim.material,
            });
        }
        meshes.push(glb::Mesh {
            name: format!("{name}/body"),
            prims,
            weights: mesh.weights.clone(),
        });
        let label = if k == 0 {
            format!("{name}/body")
        } else {
            format!("{name}/body{}", k + 1)
        };
        nodes.push(glb::Node {
            name: label,
            mesh: Some(k),
            skin: Some(0),
            ..Default::default()
        });
    }
    let mesh_nodes: Vec<usize> = (1..nodes.len()).collect();
    let first_bone = nodes.len();
    let wings_node = wing_parent.map(|k| {
        // A plain node at the wing-carrying spine joint: the wing roots hang from it, so the SDK can find their parent by name.
        let i = first_bone + bones.len();
        (k, i)
    });
    for (i, (bn, p, parent)) in bones.iter().enumerate() {
        let parent_pos = match parent {
            Some(q) => bones[*q].1,
            None if i >= joints => spine[wing_parent.unwrap()],
            None => [0.0; 3],
        };
        nodes.push(glb::Node {
            name: bn.clone(),
            t: sub(*p, parent_pos),
            ..Default::default()
        });
    }
    for (i, (_, _, parent)) in bones.iter().enumerate() {
        match parent {
            Some(q) => nodes[first_bone + q].children.push(first_bone + i),
            None if i >= joints => {} // under `wings`, wired below
            None => nodes[0].children.push(first_bone + i),
        }
    }
    if let Some((k, i)) = wings_node {
        // `wings` was pushed after the bones, so the children wired above point at the right index.
        debug_assert_eq!(i, nodes.len());
        let mut w = glb::Node {
            name: "wings".into(),
            ..Default::default()
        };
        w.children = bones
            .iter()
            .enumerate()
            .filter(|(j, b)| *j >= joints && b.2.is_none())
            .map(|(j, _)| first_bone + j)
            .collect();
        nodes.push(w);
        nodes[first_bone + k].children.push(i);
    }
    nodes[0].children.splice(0..0, mesh_nodes);
    let skin = glb::Skin {
        name: format!("{name}/spine"),
        joints: (0..bones.len()).map(|i| first_bone + i).collect(),
        ibm: bones
            .iter()
            .map(|b| translation([-b.1[0], -b.1[1], -b.1[2]]))
            .collect(),
        skeleton: Some(first_bone),
    };
    let dropped_clips: Vec<String> = sc.anims.iter().map(|a| a.name.clone()).collect();
    sc.nodes = nodes;
    sc.roots = vec![0];
    sc.meshes = meshes;
    sc.skins = vec![skin];
    sc.anims.clear();
    let length = (z1 - z0).abs();
    Ok(json!({
        "joints": bones.len(),
        "spine_joints": joints,
        "wings": wings,
        "bones": bones.iter().map(|b| json!({ "name": b.0, "at": b.1.map(|v| (v * 1000.0).round() / 1000.0) })).collect::<Vec<_>>(),
        "length_m": (length * 1000.0).round() / 1000.0,
        "axis_head_to_tail": [0.0, (-axis[1] * 1000.0).round() / 1000.0, (-axis[2] * 1000.0).round() / 1000.0],
        "head": if head_neg_z { "-z" } else { "+z" },
        "replaced_clips": dropped_clips,
    }))
}

// --- facing ------------------------------------------------------------------------------------

/// Which end of a long creature (placed facing −z) looks like its head: the
/// thicker end (cross-section width × height over the last 15 % of its
/// length), since heads, snouts and horns are bulkier than tails. Returns
/// (head at −z?, how much thicker that end is) or None for a model that isn't
/// long (length < 1.8 × its width and height), where this says nothing.
pub fn head_end(sc: &Scene) -> Option<(bool, f32)> {
    let posed = shape::posed(sc, &shape::pose(sc, None));
    let (lo, hi) = shape::bounds(&posed)?;
    let ext = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    if ext[2] < 1.8 * ext[0].max(ext[1]) {
        return None;
    }
    let bulk = |front: bool| {
        let cut = if front {
            lo[2] + 0.15 * ext[2]
        } else {
            hi[2] - 0.15 * ext[2]
        };
        let (mut a, mut b) = ([f32::MAX; 2], [f32::MIN; 2]);
        for p in posed
            .iter()
            .flat_map(|p| &p.pos)
            .filter(|p| if front { p[2] <= cut } else { p[2] >= cut })
        {
            a = [a[0].min(p[0]), a[1].min(p[1])];
            b = [b[0].max(p[0]), b[1].max(p[1])];
        }
        ((b[0] - a[0]) * (b[1] - a[1])).max(1e-9)
    };
    let (f, r) = (bulk(true), bulk(false));
    Some(if f >= r {
        (true, f / r)
    } else {
        (false, r / f)
    })
}

// --- rig report and repair -----------------------------------------------------------------

/// The joint tree of skin 0 as lines ("  Hips (412 verts)"), parents first.
pub fn joint_tree(sc: &Scene) -> Vec<String> {
    let Some(skin) = sc.skins.first() else {
        return Vec::new();
    };
    let mut counts = vec![0usize; skin.joints.len()];
    for n in &sc.nodes {
        if n.skin != Some(0) {
            continue;
        }
        let Some(m) = n.mesh else { continue };
        for p in &sc.meshes[m].prims {
            if let (Some(j), Some(w)) = (&p.joints, &p.weights) {
                for (jj, ww) in j.iter().zip(w) {
                    let k = (0..4).max_by(|&a, &b| ww[a].total_cmp(&ww[b])).unwrap();
                    if let Some(c) = counts.get_mut(jj[k] as usize) {
                        *c += 1;
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    let in_skin = |n: usize| skin.joints.iter().position(|&j| j == n);
    let mut depth = vec![0usize; sc.nodes.len()];
    for (n, p) in sc.walk() {
        depth[n] = p.map_or(0, |p| depth[p] + usize::from(in_skin(p).is_some()));
        if let Some(k) = in_skin(n) {
            out.push(format!(
                "{}{} ({} verts)",
                "  ".repeat(depth[n]),
                if sc.nodes[n].name.is_empty() {
                    format!("#{n}")
                } else {
                    sc.nodes[n].name.clone()
                },
                counts[k]
            ));
        }
    }
    out
}

/// Which side a joint's name puts it on, and its name without the side:
/// `L_Thigh`/`R_Thigh` (Tripo), `LeftUpLeg` (Mixamo, `mixamorig:` too),
/// `thigh.L`, `thigh_l`, `wing_l_0`, `Left Arm`. None for a center joint.
pub fn side_of(name: &str) -> Option<(char, String)> {
    let name = name.rsplit(':').next().unwrap_or(name);
    let mut side = None;
    let mut base = Vec::new();
    for t in name.split(['_', '.', '-', ' ']).filter(|t| !t.is_empty()) {
        let low = t.to_lowercase();
        let marker = match low.as_str() {
            "l" | "left" => Some('l'),
            "r" | "right" => Some('r'),
            _ => None,
        };
        if let (Some(m), None) = (marker, side) {
            side = Some(m);
            continue;
        }
        // CamelCase: `LeftUpLeg`, `RightHand` (not `Leftover`, `Rightmost`).
        let camel = [("Left", 'l'), ("Right", 'r')].into_iter().find_map(|(w, m)| {
            let rest = t.strip_prefix(w)?;
            rest.chars().next().is_none_or(|c| c.is_ascii_uppercase() || c.is_ascii_digit()).then_some((m, rest))
        });
        match camel {
            Some((m, rest)) if side.is_none() => {
                side = Some(m);
                if !rest.is_empty() {
                    base.push(rest.to_lowercase());
                }
            }
            _ => base.push(low),
        }
    }
    side.map(|s| (s, base.join("_")))
}

/// Joints named for one side (`L_…`, `Left…`, `….L`, `…_l`) whose other side
/// is missing; None when fewer than two joints are named for a side (then the
/// positions decide).
fn mirror_by_name(names: &[&str]) -> Option<Vec<String>> {
    let sided: Vec<(char, String, &str)> = names.iter().filter_map(|n| side_of(n).map(|(s, b)| (s, b, *n))).collect();
    if sided.len() < 2 {
        return None;
    }
    let has = |s: char, b: &str| sided.iter().any(|(s2, b2, _)| *s2 == s && b2 == b);
    let lone: Vec<&(char, String, &str)> = sided.iter().filter(|(s, b, _)| !has(if *s == 'l' { 'r' } else { 'l' }, b)).collect();
    if lone.is_empty() {
        return Some(Vec::new());
    }
    let list = |side: char| lone.iter().filter(|x| x.0 == side).map(|x| x.2).collect::<Vec<_>>();
    let (l, r) = (list('l'), list('r'));
    let say = |v: &[&str]| if v.len() > 4 { format!("{} and {} more", v[..4].join(", "), v.len() - 4) } else { v.join(", ") };
    let mut parts = Vec::new();
    if !l.is_empty() {
        parts.push(format!("{} on the left", say(&l)));
    }
    if !r.is_empty() {
        parts.push(format!("{} on the right", say(&r)));
    }
    Some(vec![format!(
        "the rig isn't symmetric: {} {} no twin on the other side by name (a missing wing or limb?)",
        parts.join(" and "),
        if lone.len() == 1 { "has" } else { "have" }
    )])
}

/// Paired limbs: joints named for one side (`L_`/`R_`, `Left`/`Right`,
/// `.L`/`.R`, `_l`/`_r`) without the other side's twin; for a rig whose names
/// don't say, joints on the left (−x) without a mirror on the right at rest,
/// and the other way round. Empty when the rig is symmetric (or has no skin).
pub fn mirror_check(sc: &Scene) -> Vec<String> {
    let Some(skin) = sc.skins.first() else {
        return Vec::new();
    };
    let names: Vec<&str> = skin.joints.iter().map(|&j| sc.nodes[j].name.as_str()).collect();
    if let Some(found) = mirror_by_name(&names) {
        return found;
    }
    let world = sc.world(&shape::pose(sc, None).local);
    let pts: Vec<[f32; 3]> = skin
        .joints
        .iter()
        .map(|&j| glb::point(&world[j], [0.0; 3]))
        .collect();
    let span = pts
        .iter()
        .map(|p| p[0].abs())
        .fold(0.0f32, f32::max)
        .max(1e-6);
    let (mut left, mut right) = (0, 0);
    for (i, p) in pts.iter().enumerate() {
        if p[0].abs() < 0.1 * span {
            continue;
        }
        let mirrored = pts.iter().enumerate().any(|(k, q)| {
            k != i
                && (q[0] + p[0]).abs() < 0.12 * span
                && (q[1] - p[1]).abs() < 0.15 * span
                && (q[2] - p[2]).abs() < 0.15 * span
        });
        if !mirrored {
            if p[0] < 0.0 { left += 1 } else { right += 1 }
        }
    }
    let mut out = Vec::new();
    if left + right > 0 {
        out.push(format!("the rig isn't symmetric: {left} joint(s) on the left and {right} on the right have no mirror (a missing wing or limb?)"));
    }
    out
}

/// Merges leaf joints into their parents until the skin has at most `max`
/// joints (a vertex weighted to a leaf follows its parent instead). Returns how many went.
pub fn merge_leaf_joints(sc: &mut Scene, max: usize) -> usize {
    let mut removed = 0;
    for si in 0..sc.skins.len() {
        loop {
            let skin = &sc.skins[si];
            if skin.joints.len() <= max {
                break;
            }
            let parent_of = |n: usize| sc.nodes.iter().position(|p| p.children.contains(&n));
            // A leaf: a joint none of whose children are joints of this skin; take the smallest-weighted first by walking from the end.
            let leaf = (0..skin.joints.len()).rev().find(|&k| {
                let n = skin.joints[k];
                !sc.nodes[n].children.iter().any(|c| skin.joints.contains(c))
                    && parent_of(n)
                        .and_then(|p| skin.joints.iter().position(|&j| j == p))
                        .is_some()
            });
            let Some(k) = leaf else { break };
            let n = skin.joints[k];
            let pk = skin
                .joints
                .iter()
                .position(|&j| Some(j) == parent_of(n))
                .unwrap();
            let pk_new = if pk > k { pk - 1 } else { pk };
            // Remap vertex joints of every mesh using this skin.
            let users: Vec<usize> = sc
                .nodes
                .iter()
                .filter(|nd| nd.skin == Some(si))
                .filter_map(|nd| nd.mesh)
                .collect();
            for m in users {
                for p in &mut sc.meshes[m].prims {
                    if let Some(js) = &mut p.joints {
                        for j in js.iter_mut().flat_map(|j| j.iter_mut()) {
                            let v = *j as usize;
                            *j = if v == k {
                                pk_new as u16
                            } else if v > k {
                                (v - 1) as u16
                            } else {
                                v as u16
                            };
                        }
                    }
                }
            }
            let skin = &mut sc.skins[si];
            skin.joints.remove(k);
            skin.ibm.remove(k);
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A long box along z (head at −z) with flat "wings" sticking out along x.
    fn creature(wings: bool) -> Scene {
        let mut sc = glb::tests::cube_scene();
        let p = &mut sc.meshes[0].prims[0];
        let mut pos = Vec::new();
        let mut idx = Vec::new();
        // A grid of points: the body −0.1…0.1 × −0.1…0.1 × −1…1; wings out to ±1.2 at z ≈ 0.
        for i in 0..=20 {
            for (x, y) in [(-0.1, -0.1), (0.1, -0.1), (0.1, 0.1), (-0.1, 0.1)] {
                pos.push([x, y, -1.0 + i as f32 * 0.1]);
            }
        }
        if wings {
            for i in 0..=12 {
                let x = 0.1 * i as f32;
                for s in [-1.0, 1.0] {
                    pos.push([s * x, 0.0, -0.2]);
                    pos.push([s * x, 0.0, 0.2]);
                }
            }
        }
        for i in 0..pos.len().saturating_sub(2) {
            idx.extend([i as u32, i as u32 + 1, i as u32 + 2]);
        }
        p.pos = pos;
        p.nrm = None;
        p.tan = None;
        p.uv = None;
        p.joints = None;
        p.weights = None;
        p.indices = idx;
        sc
    }

    #[test]
    fn a_spine_runs_head_to_tail_and_bends_the_mesh() {
        let mut sc = creature(false);
        let facts = add_spine(&mut sc, "dragon", 6, false, true).unwrap();
        assert_eq!(facts["joints"], 6);
        assert_eq!(sc.nodes[0].name, "dragon");
        assert_eq!(sc.nodes[1].name, "dragon/body");
        let names: Vec<&str> = sc.skins[0]
            .joints
            .iter()
            .map(|&j| sc.nodes[j].name.as_str())
            .collect();
        assert_eq!(
            names,
            [
                "spine_0", "spine_1", "spine_2", "spine_3", "spine_4", "spine_5"
            ]
        );
        let w = sc.world(&sc.rest_locals());
        assert!(
            (w[sc.skins[0].joints[0]][14] + 1.0).abs() < 1e-4,
            "the head joint at −z"
        );
        assert!(
            (w[sc.skins[0].joints[5]][14] - 1.0).abs() < 1e-4,
            "the tail joint at +z"
        );
        // Rest pose: skinning gives back the baked mesh.
        let rest = shape::posed(&sc, &shape::pose(&sc, None));
        let before = creature(false);
        let orig = shape::posed(&before, &shape::pose(&before, None));
        for (a, b) in rest[0].pos.iter().zip(&orig[0].pos) {
            assert!((0..3).all(|k| (a[k] - b[k]).abs() < 1e-4), "{a:?} vs {b:?}");
        }
        // Weights sum to one and the tail follows the tail joints.
        let p = &sc.meshes[0].prims[0];
        for (j, w) in p
            .joints
            .as_ref()
            .unwrap()
            .iter()
            .zip(p.weights.as_ref().unwrap())
        {
            assert!((w.iter().sum::<f32>() - 1.0).abs() < 1e-4);
            assert!(j.iter().all(|&j| (j as usize) < 6));
        }
        let tail = p.pos.iter().position(|v| v[2] > 0.99).unwrap();
        assert!(
            p.joints.as_ref().unwrap()[tail][0] == 5,
            "the tail vertex mostly follows the last joint"
        );
        // Written and read back, it's still a valid model for mb3d.
        let bytes = glb::write(&sc, "test");
        let (facts, warnings) = shape::check(&bytes).unwrap();
        assert_eq!(facts["joints"], 6);
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn birds_get_two_bone_wings_under_a_wings_node() {
        let mut sc = creature(true);
        assert!(wants_wings(
            SpineMode::Auto,
            Some("avian"),
            "a heron",
            [2.4, 0.3, 2.0]
        ));
        assert!(
            !wants_wings(SpineMode::Auto, None, "a dragon", [0.6, 0.6, 4.5]),
            "a serpent dragon is long, not winged"
        );
        let facts = add_spine(&mut sc, "heron", 5, true, true).unwrap();
        assert_eq!(facts["joints"], 9);
        let find = |n: &str| sc.nodes.iter().position(|x| x.name == n).unwrap();
        let wings = find("wings");
        assert!(
            sc.nodes[wings].children.contains(&find("wing_l_0"))
                && sc.nodes[wings].children.contains(&find("wing_r_0"))
        );
        assert!(
            sc.nodes[find("wing_l_0")]
                .children
                .contains(&find("wing_l_1"))
        );
        let w = sc.world(&sc.rest_locals());
        assert!(
            w[find("wing_l_1")][12] < -0.4 && w[find("wing_r_1")][12] > 0.4,
            "left is −x"
        );
        // A wingtip vertex follows the outer wing bone.
        let p = &sc.meshes[0].prims[0];
        let tip = p.pos.iter().position(|v| v[0] < -1.15).unwrap();
        let skin_joint = sc.skins[0].joints[p.joints.as_ref().unwrap()[tip][0] as usize];
        assert_eq!(sc.nodes[skin_joint].name, "wing_l_1");
        assert!(mirror_check(&sc).is_empty(), "{:?}", mirror_check(&sc));
        let tree = joint_tree(&sc);
        assert_eq!(
            tree[0].trim_start(),
            format!(
                "spine_0 ({} verts)",
                tree[0]
                    .split('(')
                    .nth(1)
                    .unwrap()
                    .trim_end_matches(" verts)")
            )
        );
        assert!(tree.iter().any(|l| l.contains("wing_r_1")));
    }

    #[test]
    fn sides_are_read_from_joint_names() {
        for (n, side, base) in [
            ("L_Thigh", 'l', "thigh"),
            ("R_ForearmTwist01", 'r', "forearmtwist01"),
            ("mixamorig:LeftUpLeg", 'l', "upleg"),
            ("RightHand", 'r', "hand"),
            ("thigh.L", 'l', "thigh"),
            ("upper_arm_r", 'r', "upper_arm"),
            ("wing_l_0", 'l', "wing_0"),
            ("Left Arm", 'l', "arm"),
        ] {
            assert_eq!(side_of(n), Some((side, base.to_string())), "{n}");
        }
        for center in ["Spine02", "Hip", "Leftover", "Rightmost", "Root", "NeckTwist01"] {
            assert_eq!(side_of(center), None, "{center}");
        }
        // Tripo's biped: symmetric by name, wherever its bones point (it faces +x,
        // so its sides are ±z, and the positions alone said 38 joints were lopsided).
        let tripo = ["Root", "Hip", "L_Thigh", "L_Calf", "R_Thigh", "R_Calf", "Spine01", "L_Clavicle", "R_Clavicle", "L_Hand", "R_Hand"];
        assert_eq!(mirror_by_name(&tripo), Some(vec![]));
        let w = mirror_by_name(&["Root", "wing_l_0", "wing_l_1", "Spine"]).unwrap();
        assert!(w[0].contains("wing_l_0, wing_l_1 on the left have no twin"), "{w:?}");
        assert_eq!(mirror_by_name(&["Root", "Spine", "Tail"]), None, "nothing named for a side: positions decide");
    }

    #[test]
    fn rigs_over_the_limit_lose_leaves_and_one_sided_rigs_are_flagged() {
        let mut sc = creature(false);
        add_spine(&mut sc, "x", 8, false, true).unwrap();
        // Hang an extra one-sided "wing" chain off the middle.
        let mid = sc.skins[0].joints[3];
        let n = sc.nodes.len();
        sc.nodes.push(glb::Node {
            name: "stub".into(),
            t: [-0.5, 0.0, 0.0],
            ..Default::default()
        });
        sc.nodes[mid].children.push(n);
        sc.skins[0].joints.push(n);
        sc.skins[0].ibm.push(glb::identity());
        assert!(!mirror_check(&sc).is_empty());
        assert_eq!(merge_leaf_joints(&mut sc, 6), 3);
        assert_eq!(sc.skins[0].joints.len(), 6);
        let p = &sc.meshes[0].prims[0];
        assert!(p.joints.as_ref().unwrap().iter().flatten().all(|&j| j < 6));
    }
}
