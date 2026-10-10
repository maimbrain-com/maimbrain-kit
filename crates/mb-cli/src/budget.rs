//! The skill's per-frame budgets (≤ 200 mb3d draws, ≤ 150 k triangles, a
//! handful of skinned draws, a few thousand particles; SKILL.md
//! "Performance budgets") checked against what frames drew, for `mb shot`
//! and `mb measure`. The page decides which frames are over (`BUDGETS` in
//! runtime/src/stats.ts: each over-budget frame has `over`, the budgets it
//! went over, and `top`, the nodes that cost it the most triangles); this
//! says so, with numbers and the nodes to look at.

use serde_json::{Value, json};

/// The budgets in the order they're reported, with how a frame uses each:
/// draws and triangles count the reflector's pass too (stats.ts `budgetUse`).
const KEYS: [&str; 4] = ["draws", "triangles", "skinned", "particles"];

/// Nodes listed under a warning.
const LIST: usize = 3;
/// An instanced node whose bounds are wider than this (m) is probably one
/// list spread over the whole level, which culling can never skip.
const WIDE: f64 = 60.0;

fn num(v: &Value, k: &str) -> f64 {
    v[k].as_f64().unwrap_or(0.0)
}

/// What frame `f` counts against budget `k`.
pub fn usage(f: &Value, k: &str) -> f64 {
    match k {
        "draws" => num(f, "draws") + num(f, "reflected") + num(f, "under"),
        "triangles" => num(f, "triangles") + num(f, "reflectedTriangles"),
        _ => num(f, k),
    }
}

/// mb3d's texture memory against its budget, from `mb.stats()`'s (or a
/// frame's) `textureBytes`, `textureBudget`, `textures` and `textureLimit`:
/// "textures 42.1 of 96 MB (12 of 64)", with a warning near or at a limit
/// (a texture or model over it is refused, −4). None for a page without them.
pub fn textures(bytes: f64, budget: f64, count: f64, limit: f64) -> Option<String> {
    if budget <= 0.0 || limit <= 0.0 {
        return None;
    }
    let mb = |b: f64| b / (1u64 << 20) as f64;
    let mut s = format!("textures {:.1} of {:.0} MB ({count} of {limit})", mb(bytes), mb(budget));
    if bytes >= budget * 0.85 || count >= limit - 4.0 {
        s += " — near mb3d's texture limit: a texture or model that doesn't fit is refused (−4, logged as `runtime: … refused`)";
    }
    Some(s)
}

/// [`textures`] from a stats object's fields.
pub fn textures_of(st: &Value) -> Option<String> {
    st["textureBytes"].as_f64()?;
    textures(num(st, "textureBytes"), num(st, "textureBudget"), num(st, "textures"), num(st, "textureLimit"))
}

/// 750400 → "750.4k"; 150000 → "150k"; 200 → "200".
pub fn count(v: f64) -> String {
    if v >= 1000.0 {
        let k = format!("{:.1}", v / 1000.0);
        format!("{}k", k.strip_suffix(".0").unwrap_or(&k))
    } else {
        format!("{v}")
    }
}

/// The budgets a frame went over, as the page flagged them.
pub fn over(f: &Value) -> Vec<&str> {
    f["over"].as_array().map_or(Vec::new(), |a| a.iter().filter_map(Value::as_str).collect())
}

/// How one budget's use reads: "triangles 750.4k > 150k (738.3k + 12.1k reflected)".
fn use_text(f: &Value, k: &str, budgets: &Value) -> String {
    let split = match k {
        "draws" if num(f, "reflected") + num(f, "under") > 0.0 => {
            let refl = if num(f, "reflected") > 0.0 { format!(" + {} reflected", count(num(f, "reflected"))) } else { String::new() };
            let under = if num(f, "under") > 0.0 { format!(" + {} under water", count(num(f, "under"))) } else { String::new() };
            format!(" ({}{refl}{under})", count(num(f, "draws")))
        }
        "triangles" if num(f, "reflectedTriangles") > 0.0 => format!(" ({} + {} reflected)", count(num(f, "triangles")), count(num(f, "reflectedTriangles"))),
        _ => String::new(),
    };
    format!("{k} {} > {}{split}", count(usage(f, k)), count(num(budgets, k)))
}

/// One node from a frame's `top`: what it is, what it cost, and a hint when
/// its bounds say culling can't help.
pub fn node_text(c: &Value) -> String {
    let (tris, refl, inst) = (num(c, "triangles"), num(c, "reflectedTriangles"), num(c, "instances"));
    let kind = c["kind"].as_str().unwrap_or("mesh");
    let what = if inst > 0.0 { format!("{kind} ×{inst}") } else { kind.to_string() };
    let per = if inst > 0.0 { format!(" ({} each)", count((tris / inst).round())) } else { String::new() };
    let refl = if refl > 0.0 { format!(" + {} reflected", count(refl)) } else { String::new() };
    let r = num(c, "radius");
    let hint = if inst > 1.0 && r > WIDE {
        format!(": its instances span {:.0} m, so it's drawn whenever any part is in view; split it into chunk nodes so culling works", 2.0 * r)
    } else {
        format!(", {:.0} m across", 2.0 * r)
    };
    format!("node {} (mesh {}, {what}): {} tris{per}{refl}{hint}", c["node"], c["mesh"], count(tris))
}

/// The warning for frames since the last check, `last` being the newest (with
/// its `top`): which budgets the last frame went over and by how much, in
/// how many of the frames, and the nodes that cost it the most. None when
/// no frame went over.
pub fn warning(last: &Value, frames: &[Value], budgets: &Value) -> Option<String> {
    let flagged = frames.iter().filter(|f| !over(f).is_empty()).count();
    let now = over(last);
    if flagged == 0 && now.is_empty() {
        return None;
    }
    let mut out = String::from("warning: over the skill's budget (60 fps on an iPhone 13 class phone): ");
    if now.is_empty() {
        // Over earlier in the stretch, not in the frame shot: its worst frame.
        let worst = frames.iter().filter(|f| !over(f).is_empty()).max_by(|a, b| usage(a, "triangles").total_cmp(&usage(b, "triangles")))?;
        out += &format!("{} in {flagged} of the {} frames up to this shot (not this one)", over(worst).iter().map(|k| use_text(worst, k, budgets)).collect::<Vec<_>>().join(", "), frames.len());
        return Some(out + &top_lines(&worst["top"]));
    }
    out += &now.iter().map(|k| use_text(last, k, budgets)).collect::<Vec<_>>().join(", ");
    if frames.len() > 1 {
        out += &format!(" (over in {flagged} of the {} frames up to this shot)", frames.len());
    }
    Some(out + &top_lines(&last["top"]))
}

fn top_lines(top: &Value) -> String {
    let top: Vec<&Value> = top.as_array().into_iter().flatten().collect();
    let mut out: String = top.iter().take(LIST).map(|c| format!("\n  most: {}", node_text(c))).collect();
    out += &shared_meshes(&top).iter().map(|l| format!("\n  {l}")).collect::<String>();
    out
}

/// Meshes several of the listed nodes draw: "mesh 1 in 5 of these nodes: 304k tris".
fn shared_meshes(top: &[&Value]) -> Vec<String> {
    let mut meshes: Vec<(&Value, usize, f64)> = Vec::new();
    for c in top {
        let tris = num(c, "triangles") + num(c, "reflectedTriangles");
        match meshes.iter_mut().find(|m| *m.0 == c["mesh"]) {
            Some(m) => (m.1, m.2) = (m.1 + 1, m.2 + tris),
            None => meshes.push((&c["mesh"], 1, tris)),
        }
    }
    meshes.iter().filter(|m| m.1 > 1).map(|(mesh, n, tris)| format!("mesh {mesh} in {n} of these nodes: {} tris", count(*tris))).collect()
}

/// Over a run's frames: how many went over each budget, the most each used,
/// and the nodes that cost the most across the over-budget frames.
pub struct Summary {
    pub frames: usize,
    /// Some frame rendered mb3d (the budgets are mb3d's).
    pub mb3d: bool,
    pub over: Vec<(&'static str, usize, f64)>,
    /// Per node: its costliest appearance (a `top` entry) and how many over-budget frames it was listed in.
    pub nodes: Vec<(Value, usize)>,
    pub budgets: Value,
}

pub fn summary(frames: &[Value], budgets: &Value) -> Summary {
    let over_frames: Vec<&Value> = frames.iter().filter(|f| !over(f).is_empty()).collect();
    let over_by = KEYS
        .iter()
        .filter_map(|&k| {
            let n = over_frames.iter().filter(|f| over(f).contains(&k)).count();
            (n > 0).then(|| (k, n, frames.iter().map(|f| usage(f, k)).fold(0.0, f64::max)))
        })
        .collect();
    let mut nodes: Vec<(Value, usize)> = Vec::new();
    let cost = |c: &Value| num(c, "triangles") + num(c, "reflectedTriangles");
    for c in over_frames.iter().flat_map(|f| f["top"].as_array().into_iter().flatten()) {
        match nodes.iter_mut().find(|(n, _)| n["node"] == c["node"]) {
            Some((n, k)) => {
                *k += 1;
                if cost(c) > cost(n) {
                    *n = c.clone();
                }
            }
            None => nodes.push((c.clone(), 1)),
        }
    }
    nodes.sort_by(|a, b| cost(&b.0).total_cmp(&cost(&a.0)));
    Summary { frames: frames.len(), mb3d: frames.iter().any(|f| f["triangles"].is_number()), over: over_by, nodes, budgets: budgets.clone() }
}

impl Summary {
    /// `mb measure`'s budget lines, each starting with `label` padding.
    pub fn text(&self) -> String {
        let b = &self.budgets;
        if !self.mb3d {
            return String::new();
        }
        if self.over.is_empty() {
            let list: Vec<String> = KEYS.iter().filter(|k| b[**k].is_number()).map(|k| format!("{k} ≤ {}", count(num(b, k)))).collect();
            return format!("  budget        every frame within the skill's ({}; reflections counted)\n", list.join(", "));
        }
        let what: Vec<String> = self.over.iter().map(|(k, n, max)| format!("{k} in {n} (up to {}, budget {})", count(*max), count(num(b, k)))).collect();
        let mut out = format!("  budget        OVER in {} of {} frames: {} (reflections counted)\n", self.over.iter().map(|o| o.1).max().unwrap_or(0), self.frames, what.join("; "));
        for (c, k) in self.nodes.iter().take(LIST) {
            out += &format!("                most: {}; listed in {k} frames\n", node_text(c));
        }
        for l in shared_meshes(&self.nodes.iter().map(|n| &n.0).collect::<Vec<_>>()) {
            out += &format!("                {l} (each at its worst)\n");
        }
        out
    }

    pub fn json(&self) -> Value {
        let over: serde_json::Map<String, Value> = self.over.iter().map(|(k, n, max)| (k.to_string(), json!({ "frames": n, "max": max }))).collect();
        let nodes: Vec<Value> = self.nodes.iter().map(|(c, k)| json!({ "worst": c, "frames": k })).collect();
        json!({ "budgets": self.budgets, "over": over, "nodes": nodes })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn budgets() -> Value {
        json!({ "draws": 200, "triangles": 150000, "skinned": 16, "particles": 5000 })
    }

    fn props(tris: f64) -> Value {
        json!({ "node": 65537, "mesh": 3, "kind": "instanced", "triangles": tris, "instances": 2400, "reflectedTriangles": 12100, "center": [0, 0, -130], "radius": 260 })
    }

    fn frame(tris: f64, over: bool) -> Value {
        let mut f = json!({ "draws": 36, "reflected": 1, "triangles": tris, "reflectedTriangles": 12100, "skinned": 1, "particles": 40 });
        if over {
            f["over"] = json!(["triangles"]);
            f["top"] = json!([props(tris - 1600.0), { "node": 4, "mesh": 9, "kind": "skinned", "triangles": 1600, "instances": 0, "reflectedTriangles": 0, "center": [0, 1, 0], "radius": 0.8 }]);
        }
        f
    }

    #[test]
    fn a_frame_over_budget_names_the_budget_and_the_node() {
        let frames = [frame(100_000.0, false), frame(738_300.0, true), frame(738_300.0, true)];
        let w = warning(&frames[2], &frames, &budgets()).unwrap();
        assert!(w.starts_with("warning: over the skill's budget (60 fps on an iPhone 13 class phone): triangles 750.4k > 150k (738.3k + 12.1k reflected) (over in 2 of the 3 frames up to this shot)"), "{w}");
        assert!(
            w.contains("\n  most: node 65537 (mesh 3, instanced ×2400): 736.7k tris (307 each) + 12.1k reflected: its instances span 520 m, so it's drawn whenever any part is in view; split it into chunk nodes"),
            "{w}"
        );
        assert!(w.contains("\n  most: node 4 (mesh 9, skinned): 1.6k tris, 2 m across"), "{w}");
        assert!(!w.contains("of these nodes"), "no mesh is shared: {w}");
        let shared = [props(100_000.0), json!({ "node": 7, "mesh": 3, "triangles": 50_000, "instances": 900, "radius": 40 })];
        assert_eq!(shared_meshes(&shared.iter().collect::<Vec<_>>()), ["mesh 3 in 2 of these nodes: 162.1k tris"]);
        assert!(warning(&frames[0], &frames[..1], &budgets()).is_none(), "within budget: quiet");
        let earlier = warning(&frames[0], &[frames[1].clone(), frames[0].clone()], &budgets()).unwrap();
        assert!(earlier.contains("in 1 of the 2 frames up to this shot (not this one)"), "{earlier}");
    }

    #[test]
    fn a_run_sums_up_frames_over_and_the_costliest_nodes() {
        let frames = [frame(100_000.0, false), frame(500_000.0, true), frame(738_300.0, true)];
        let s = summary(&frames, &budgets());
        assert_eq!(s.over, vec![("triangles", 2, 750_400.0)]);
        assert_eq!(s.nodes[0].1, 2);
        assert_eq!(s.nodes[0].0["triangles"], 736_700.0, "its worst frame");
        let text = s.text();
        assert!(text.starts_with("  budget        OVER in 2 of 3 frames: triangles in 2 (up to 750.4k, budget 150k) (reflections counted)\n"), "{text}");
        assert!(text.contains("                most: node 65537 (mesh 3, instanced ×2400): 736.7k tris"), "{text}");
        assert_eq!(s.json()["over"]["triangles"]["frames"], 2);
        assert_eq!(summary(&[json!({ "draws": 0, "draws2d": 3 })], &budgets()).text(), "", "a 2D game: nothing to say");
        let calm = summary(&frames[..1], &budgets()).text();
        assert_eq!(calm, "  budget        every frame within the skill's (draws ≤ 200, triangles ≤ 150k, skinned ≤ 16, particles ≤ 5k; reflections counted)\n");
    }

    #[test]
    fn usage_counts_the_reflection_and_the_view_under_the_water() {
        let f = json!({ "draws": 30, "reflected": 28, "under": 12, "triangles": 4000, "reflectedTriangles": 3000, "particles": 7 });
        assert_eq!((usage(&f, "draws"), usage(&f, "triangles"), usage(&f, "particles"), usage(&f, "skinned")), (70.0, 7000.0, 7.0, 0.0));
        let budgets = json!({ "draws": 60 });
        assert_eq!(use_text(&f, "draws", &budgets), "draws 70 > 60 (30 + 28 reflected + 12 under water)");
        assert_eq!((count(750_400.0), count(150_000.0), count(200.0)), ("750.4k".into(), "150k".into(), "200".into()));
    }
}
