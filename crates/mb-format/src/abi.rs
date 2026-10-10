//! The host import table (SPEC §5): every function a guest may import from
//! the `mb` namespace, its exact Wasm signature, and what the manifest must
//! declare to use it. Imports not listed here are rejected, so modules are
//! added as the host implements them.

use crate::manifest::{Capability, Manifest, Sensor};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ty {
    I32,
    I64,
    F32,
    F64,
}

use Ty::*;

#[derive(Clone, Copy, Debug)]
pub enum Requires {
    Always,
    /// A host engine at this version or later (SPEC §2 `stdlib`).
    Stdlib(&'static str, u32),
    Sensor(Sensor),
    Capability(Capability),
}

#[derive(Clone, Copy, Debug)]
pub struct Import {
    pub name: &'static str,
    pub params: &'static [Ty],
    pub results: &'static [Ty],
    pub requires: Requires,
}

const fn f(name: &'static str, params: &'static [Ty], results: &'static [Ty], requires: Requires) -> Import {
    Import { name, params, results, requires }
}

const A: Requires = Requires::Always;
const MB2D: Requires = Requires::Stdlib("mb2d", 1);
/// Added in mb2d 2: game fonts (SPEC §5.3).
const MB2D2: Requires = Requires::Stdlib("mb2d", 2);
const MB3D: Requires = Requires::Stdlib("mb3d", 1);
/// Added in mb3d 2: skinned and animated glTF, instancing, freeing resources, fog, toon and outlines, 3D text, depth of field, speed effects, reflections, sky v2, ripples and wakes, the beauty pass.
const MB3D2: Requires = Requires::Stdlib("mb3d", 2);

pub const IMPORTS: &[Import] = &[
    // 5.1 sys
    f("mb_log", &[I32, I32, I32], &[], A),
    f("mb_mark", &[I32, I32], &[], A),
    f("mb_param", &[I32, I32, I32, I32], &[I32], A),
    f("mb_time", &[], &[F64], A),
    f("mb_time_lost", &[], &[F32], A),
    f("mb_rand_seed", &[], &[I64], A),
    f("mb_daily_seed", &[], &[I64], A),
    f("mb_round", &[I32], &[], A),
    f("mb_player_id", &[I32], &[I32], A),
    f("mb_locale", &[I32, I32], &[I32], A),
    f("mb_screen", &[I32], &[], A),
    f("mb_asset_load", &[I32, I32], &[I32], A),
    f("mb_asset_state", &[I32], &[I32], A),
    f("mb_asset_read", &[I32, I32, I32], &[I32], A),
    // 5.2 input
    f("mb_input_poll", &[I32, I32], &[I32], A),
    // 5.3 mb2d
    f("mb2d_image", &[I32], &[I32], MB2D),
    f("mb2d_font", &[I32], &[I32], MB2D),
    f("mb2d_clear", &[I32], &[], MB2D),
    f("mb2d_push", &[], &[], MB2D),
    f("mb2d_pop", &[], &[], MB2D),
    f("mb2d_translate", &[F32, F32], &[], MB2D),
    f("mb2d_rotate", &[F32], &[], MB2D),
    f("mb2d_scale", &[F32, F32], &[], MB2D),
    f("mb2d_blend", &[I32], &[], MB2D),
    f("mb2d_rect", &[F32, F32, F32, F32, I32], &[], MB2D),
    f("mb2d_rect_gradient", &[F32, F32, F32, F32, I32, I32], &[], MB2D),
    f("mb2d_circle", &[F32, F32, F32, I32], &[], MB2D),
    f("mb2d_line", &[F32, F32, F32, F32, F32, I32], &[], MB2D),
    f("mb2d_poly", &[I32, I32, I32], &[], MB2D),
    f("mb2d_sprite", &[I32, F32, F32, F32, F32, F32, F32, F32, F32, I32], &[], MB2D),
    f("mb2d_text", &[I32, F32, F32, F32, I32, I32, I32], &[], MB2D),
    f("mb2d_measure", &[I32, F32, I32, I32], &[F32], MB2D),
    f("mb2d_rrect", &[F32, F32, F32, F32, F32, F32, F32, I32, I32], &[], MB2D),
    f("mb2d_text_style", &[F32, F32, I32, F32], &[], MB2D),
    f("mb2d_antialias", &[I32], &[], MB2D),
    // 5.3 mb2d 2: game fonts
    f("mb2d_font_load", &[I32], &[I32], MB2D2),
    f("mb2d_font_metrics", &[I32, I32], &[I32], MB2D2),
    // 5.4 mb3d: resources
    f("mb3d_mesh", &[I32, I32], &[I32], MB3D),
    f("mb3d_texture", &[I32], &[I32], MB3D),
    f("mb3d_material", &[I32], &[I32], MB3D),
    f("mb3d_material_set", &[I32, I32], &[I32], MB3D),
    f("mb3d_gltf", &[I32], &[I32], MB3D),
    f("mb3d_model_spawn", &[I32, I32], &[I32], MB3D),
    // 5.4 mb3d: scene graph
    f("mb3d_node", &[], &[I32], MB3D),
    f("mb3d_node_parent", &[I32, I32], &[], MB3D),
    f("mb3d_node_transform", &[I32, I32], &[], MB3D),
    f("mb3d_node_mesh", &[I32, I32, I32], &[], MB3D),
    f("mb3d_node_visible", &[I32, I32], &[], MB3D),
    f("mb3d_node_destroy", &[I32], &[], MB3D),
    f("mb3d_camera", &[I32], &[], MB3D),
    f("mb3d_project", &[F32, F32, F32, I32], &[I32], MB3D),
    // 5.4 mb3d: lighting and sky
    f("mb3d_sun", &[I32], &[], MB3D),
    f("mb3d_light_point", &[I32, F32, F32, F32, F32, F32], &[I32], MB3D),
    f("mb3d_sky", &[I32], &[], MB3D),
    f("mb3d_post", &[I32], &[], MB3D),
    // 5.4 mb3d: effects
    f("mb3d_emitter", &[I32], &[I32], MB3D),
    f("mb3d_emit", &[I32, F32, F32, F32, F32, F32, F32, I32], &[], MB3D),
    f("mb3d_emit_moving", &[I32, F32, F32, F32, F32, F32, F32, I32, F32, F32, F32], &[], MB3D),
    f("mb3d_trail", &[I32], &[I32], MB3D),
    f("mb3d_trail_attach", &[I32, I32], &[], MB3D),
    f("mb3d_trail_detach", &[I32], &[], MB3D),
    f("mb3d_shockwave", &[F32, F32, F32, F32, F32, F32], &[], MB3D),
    f("mb3d_render", &[], &[], MB3D),
    // 5.4 mb3d 2: freeing resources
    f("mb3d_free", &[I32, I32], &[I32], MB3D2),
    // 5.4 mb3d 2: instancing
    f("mb3d_instances", &[I32, I32, I32], &[I32], MB3D2),
    // 5.4 mb3d 2: animated glTF (skins, clips, morph targets) and named nodes
    f("mb3d_clip_count", &[I32], &[I32], MB3D2),
    f("mb3d_clip_find", &[I32, I32, I32], &[I32], MB3D2),
    f("mb3d_clip_duration", &[I32, I32], &[F32], MB3D2),
    f("mb3d_anim", &[I32, I32, F32, F32], &[I32], MB3D2),
    f("mb3d_node_find", &[I32, I32, I32], &[I32], MB3D2),
    f("mb3d_node_morph", &[I32, F32, F32, F32, F32], &[], MB3D2),
    f("mb3d_node_world", &[I32, I32], &[I32], MB3D2),
    f("mb3d_node_material", &[I32], &[I32], MB3D2),
    // 5.4 mb3d 2: distance fade
    f("mb3d_node_fade", &[I32, F32, F32], &[], MB3D2),
    f("mb3d_draw_distance", &[F32, F32], &[], MB3D2),
    // 5.4 mb3d 2: skins built in code
    f("mb3d_skin", &[I32, I32, I32], &[I32], MB3D2),
    // 5.4 mb3d 2: looks
    f("mb3d_fog", &[I32], &[], MB3D2),
    f("mb3d_material_style", &[I32, I32], &[I32], MB3D2),
    f("mb3d_text", &[I32, I32, I32, I32], &[I32], MB3D2),
    f("mb3d_dof", &[I32], &[], MB3D2),
    f("mb3d_speed", &[I32], &[], MB3D2),
    f("mb3d_reflector", &[I32], &[], MB3D2),
    f("mb3d_atmosphere", &[I32], &[], MB3D2),
    f("mb3d_clouds", &[I32], &[], MB3D2),
    f("mb3d_sky_disc", &[I32], &[], MB3D2),
    f("mb3d_sky_texture", &[I32, I32], &[I32], MB3D2),
    f("mb3d_ripple", &[F32, F32, F32, F32, F32], &[], MB3D2),
    f("mb3d_wake", &[I32, F32, F32, F32, F32, F32, F32], &[I32], MB3D2),
    f("mb3d_water", &[I32], &[], MB3D2),
    // 5.4 mb3d 2: the beauty pass
    f("mb3d_grade", &[I32], &[], MB3D2),
    f("mb3d_shafts", &[I32], &[], MB3D2),
    f("mb3d_halos", &[I32], &[], MB3D2),
    f("mb3d_film", &[I32], &[], MB3D2),
    // 5.4 mb3d 2: reading a material back, a model's materials and node names
    f("mb3d_material_get", &[I32, I32], &[I32], MB3D2),
    f("mb3d_model_materials", &[I32, I32, I32], &[I32], MB3D2),
    f("mb3d_model_node_names", &[I32, I32, I32], &[I32], MB3D2),
    // 5.4 mb3d 2: the retro look
    f("mb3d_retro", &[I32], &[], MB3D2),
    f("mb3d_texture_filter", &[I32, I32, I32], &[I32], MB3D2),
    f("mb3d_backdrop", &[I32, I32], &[I32], MB3D2),
    // 5.4 mb3d 2: masked clips, procedural layers over clips, bone parents
    f("mb3d_anim_mask", &[I32, I32, F32, F32, I32, I32], &[I32], MB3D2),
    f("mb3d_anim_take", &[I32, I32], &[I32], MB3D2),
    f("mb3d_node_get_parent", &[I32], &[I32], MB3D2),



    // 5.6 audio (output only, so it never affects determinism)
    f("mb_sound", &[I32], &[I32], A),
    f("mb_play", &[I32, F32, F32, F32, I32], &[I32], A),
    f("mb_play_at", &[I32, F32, F32, F32, I32, F64], &[I32], A),
    f("mb_play_from", &[I32, F32, F32, F32, I32, F64, F64, F32], &[I32], A),
    f("mb_voice_set", &[I32, F32, F32, F32], &[], A),
    f("mb_voice_stop", &[I32], &[], A),
    f("mb_voice_stop_at", &[I32, F64, F32], &[], A),
    f("mb_voice_bus", &[I32, I32], &[], A),
    f("mb_bus_fx", &[I32, I32, I32], &[I32], A),
    // 5.7 sensors
    f("mb_tilt", &[I32], &[I32], Requires::Sensor(Sensor::Tilt)),
    f("mb_motion", &[I32], &[I32], Requires::Sensor(Sensor::Motion)),
    f("mb_attitude", &[I32], &[I32], Requires::Sensor(Sensor::Attitude)),
    f("mb_loudness", &[], &[F32], Requires::Sensor(Sensor::Loudness)),
    f("mb_light", &[], &[F32], Requires::Sensor(Sensor::Light)),
    f("mb_haptic", &[I32], &[], Requires::Sensor(Sensor::Haptics)),
    // 5.8 store / score
    f("mb_store_get", &[I32, I32, I32, I32], &[I32], Requires::Capability(Capability::Store)),
    f("mb_store_set", &[I32, I32, I32, I32], &[I32], Requires::Capability(Capability::Store)),
    f("mb_score_submit", &[I32, I64], &[I32], Requires::Capability(Capability::Score)),
    f("mb_score_show", &[I32], &[], Requires::Capability(Capability::Score)),
    // 5.9 text input / links
    f("mb_text_begin", &[I32, I32, I32], &[], Requires::Capability(Capability::TextInput)),
    f("mb_text_end", &[], &[], Requires::Capability(Capability::TextInput)),
    f("mb_link_open", &[I32, I32], &[I32], Requires::Capability(Capability::Links)),
];

/// Required and optional guest exports (SPEC §3).
pub struct Export {
    pub name: &'static str,
    pub params: &'static [Ty],
    pub results: &'static [Ty],
    pub required: bool,
}

pub const EXPORTS: &[Export] = &[
    Export { name: "mb_init", params: &[], results: &[], required: true },
    Export { name: "mb_update", params: &[F32], results: &[], required: true },
    Export { name: "mb_render", params: &[], results: &[], required: true },
    Export { name: "mb_alloc", params: &[I32], results: &[I32], required: true },
    Export { name: "mb_suspend", params: &[], results: &[], required: false },
    Export { name: "mb_resume", params: &[], results: &[], required: false },
];

pub fn lookup(name: &str) -> Option<&'static Import> {
    IMPORTS.iter().find(|i| i.name == name)
}

/// Real host functions an unknown import may have meant, closest first: names
/// a few edits away, or sharing a word with it in the same family
/// (`mb3d_speed` → mb3d functions with "speed"... or a near spelling).
pub fn suggest(name: &str) -> Vec<&'static str> {
    let words = |n: &str| n.split('_').filter(|w| !w.is_empty()).map(str::to_ascii_lowercase).collect::<Vec<_>>();
    let family = |n: &str| n.split('_').next().unwrap_or("").to_string();
    let (want, fam) = (words(name), family(name));
    let mut scored: Vec<(usize, &'static str)> = IMPORTS
        .iter()
        .filter_map(|i| {
            let d = edit_distance(&name.to_ascii_lowercase(), &i.name.to_ascii_lowercase());
            let shared = words(i.name).iter().skip(1).filter(|w| want.iter().skip(1).any(|x| x == *w || (x.len() > 3 && w.starts_with(x.as_str())))).count();
            let close = d <= 2;
            (close || (shared > 0 && family(i.name) == fam)).then_some((d.saturating_sub(shared * 4), i.name))
        })
        .collect();
    scored.sort();
    scored.into_iter().map(|(_, n)| n).take(3).collect()
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut cur = vec![i + 1; b.len() + 1];
        for (j, cb) in b.iter().enumerate() {
            cur[j + 1] = (prev[j] + usize::from(ca != *cb)).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        prev = cur;
    }
    prev[b.len()]
}

impl Requires {
    /// Whether `m` declares what this import needs; Err names what's missing.
    pub fn satisfied_by(self, m: &Manifest) -> Result<(), String> {
        match self {
            Requires::Always => Ok(()),
            Requires::Stdlib(s, v) if m.stdlib_version(s) >= v => Ok(()),
            Requires::Stdlib(s, v) => Err(format!("stdlib {{ {s} = {v} }}")),
            Requires::Sensor(s) if m.sensors.contains(&s) => Ok(()),
            Requires::Sensor(s) => Err(format!("sensors = [\"{}\"]", s.as_str())),
            Requires::Capability(c) if m.capabilities.contains(&c) => Ok(()),
            Requires::Capability(c) => Err(format!("capabilities = [\"{}\"]", c.as_str())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The SDK's raw imports (sdk/maimbrain/src/ffi.rs) agree with this table,
    /// name for name and type for type.
    #[test]
    fn sdk_imports_match_the_table() {
        let src = include_str!("../../../sdk/maimbrain/src/ffi.rs");
        let ty = |t: &str| match t.trim() {
            "f32" => F32,
            "f64" => F64,
            "u64" | "i64" => I64,
            _ => I32, // u32, i32, pointers
        };
        let mut seen = 0;
        for line in src.lines().map(str::trim).filter(|l| l.starts_with("pub fn mb")) {
            let open = line.find('(').unwrap();
            let name = &line[7..open];
            let args = &line[open + 1..line.find(')').unwrap()];
            let params: Vec<Ty> = args.split(',').filter(|a| !a.trim().is_empty()).map(|a| ty(a.split(':').nth(1).unwrap())).collect();
            let results: Vec<Ty> = line.split("->").nth(1).map(|r| ty(r.split(['=', ';']).next().unwrap())).into_iter().collect();
            let i = lookup(name).unwrap_or_else(|| panic!("{name} is in the SDK but not the ABI table"));
            assert_eq!(i.params, &params[..], "{name} params");
            assert_eq!(i.results, &results[..], "{name} results");
            seen += 1;
        }
        assert!(seen > 60, "parsed only {seen} SDK imports");
    }
}

#[cfg(test)]
mod suggest_tests {
    use super::*;

    #[test]
    fn near_misspellings_and_shared_words_are_suggested() {
        assert_eq!(suggest("mb2d_rect_")[0], "mb2d_rect");
        assert!(suggest("mb3d_node_destory").contains(&"mb3d_node_destroy"));
        assert!(suggest("totally_made_up_thing").is_empty());
        for n in suggest("mb3d_speed") {
            assert!(lookup(n).is_some());
        }
    }
}
