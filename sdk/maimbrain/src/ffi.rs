//! Raw host imports (SPEC §5). Signatures must match
//! `crates/mb-format/src/abi.rs`; only the functions a game actually calls end
//! up as imports in its wasm.
//!
//! Off wasm32 (e.g. `cargo test` on the host), every import is a no-op stub
//! that returns zero (or "absent" / "not ready" where zero would mean
//! success, and handle 1 for mb3d's constructors), so game logic can be
//! tested natively (SPEC §9). The clock and debug parameters are the
//! exceptions: `mb_time`, `mb_time_lost` and `mb_param` read per-thread
//! state that tests set (`sys::test`). So, while a test runs one, does
//! `sys::test::Host`: input, the round state, scores, saved data, seeds and
//! logs go to it instead (without one they're the stubs above).

macro_rules! imports {
    ($( pub fn $name:ident($($arg:ident: $ty:ty),* $(,)?) $(-> $ret:ty)? $(= $stub:expr)?; )*) => {
        #[cfg(target_arch = "wasm32")]
        #[link(wasm_import_module = "mb")]
        unsafe extern "C" {
            $( pub fn $name($($arg: $ty),*) $(-> $ret)?; )*
        }

        $(
            #[cfg(not(target_arch = "wasm32"))]
            #[allow(unused_variables, unreachable_code, clippy::missing_safety_doc, clippy::too_many_arguments)]
            pub unsafe fn $name($($arg: $ty),*) $(-> $ret)? {
                $( return $stub; )?
                Default::default()
            }
        )*
    };
}

imports! {
    // 5.1 sys
    pub fn mb_log(level: u32, ptr: *const u8, len: u32) = unsafe { crate::sys::test::host::log(level, ptr, len) };
    pub fn mb_mark(ptr: *const u8, len: u32);
    pub fn mb_param(name: *const u8, name_len: u32, out: *mut u8, cap: u32) -> i32 = unsafe { crate::sys::test::param_raw(name, name_len, out, cap) };
    pub fn mb_time() -> f64 = crate::sys::test::time();
    pub fn mb_time_lost() -> f32 = crate::sys::test::time_lost();
    pub fn mb_rand_seed() -> u64 = crate::sys::test::host::rand_seed();
    pub fn mb_daily_seed() -> u64 = crate::sys::test::host::daily_seed();
    pub fn mb_round(state: u32) = crate::sys::test::host::round(state);
    pub fn mb_player_id(ptr: *mut u8) -> i32;
    pub fn mb_locale(ptr: *mut u8, cap: u32) -> i32;
    pub fn mb_screen(out: *mut u8);
    pub fn mb_asset_load(path: *const u8, len: u32) -> u32;
    pub fn mb_asset_state(h: u32) -> i32;
    pub fn mb_asset_read(h: u32, ptr: *mut u8, cap: u32) -> i32;

    // 5.2 input
    pub fn mb_input_poll(buf: *mut u8, cap: u32) -> i32 = unsafe { crate::sys::test::host::input_poll(buf, cap) };

    // 5.3 mb2d
    pub fn mb2d_image(asset: u32) -> i32;
    pub fn mb2d_font(asset: u32) -> i32;
    pub fn mb2d_clear(rgba: u32);
    pub fn mb2d_push();
    pub fn mb2d_pop();
    pub fn mb2d_translate(x: f32, y: f32);
    pub fn mb2d_rotate(r: f32);
    pub fn mb2d_scale(sx: f32, sy: f32);
    pub fn mb2d_blend(mode: u32);
    pub fn mb2d_rect(x: f32, y: f32, w: f32, h: f32, rgba: u32);
    pub fn mb2d_rect_gradient(x: f32, y: f32, w: f32, h: f32, top: u32, bottom: u32);
    pub fn mb2d_circle(x: f32, y: f32, r: f32, rgba: u32);
    pub fn mb2d_line(x0: f32, y0: f32, x1: f32, y1: f32, w: f32, rgba: u32);
    pub fn mb2d_poly(pts: *const f32, n: u32, rgba: u32);
    pub fn mb2d_sprite(img: u32, sx: f32, sy: f32, sw: f32, sh: f32, dx: f32, dy: f32, dw: f32, dh: f32, tint: u32);
    pub fn mb2d_text(font: u32, size: f32, x: f32, y: f32, rgba: u32, ptr: *const u8, len: u32);
    pub fn mb2d_measure(font: u32, size: f32, ptr: *const u8, len: u32) -> f32;
    pub fn mb2d_rrect(x: f32, y: f32, w: f32, h: f32, radius: f32, stroke: f32, feather: f32, top: u32, bottom: u32);
    pub fn mb2d_text_style(weight: f32, outline: f32, outline_rgba: u32, soft: f32);
    pub fn mb2d_antialias(on: u32);
    // 5.3 mb2d 2: game fonts
    pub fn mb2d_font_load(asset: u32) -> i32 = -7;
    pub fn mb2d_font_metrics(font: u32, out: *mut u8) -> i32 = -1;

    // 5.4 mb3d
    pub fn mb3d_mesh(ptr: *const u8, len: u32) -> i32 = 1;
    pub fn mb3d_texture(asset: u32) -> i32 = 1;
    pub fn mb3d_material(ptr: *const u8) -> i32 = 1;
    pub fn mb3d_material_set(handle: u32, ptr: *const u8) -> i32;
    pub fn mb3d_gltf(asset: u32) -> i32 = 1;
    pub fn mb3d_model_spawn(model: u32, parent: u32) -> i32 = 1;
    pub fn mb3d_node() -> i32 = 1;
    pub fn mb3d_node_parent(node: u32, parent: u32);
    pub fn mb3d_node_transform(node: u32, ptr: *const u8);
    pub fn mb3d_node_mesh(node: u32, mesh: u32, material: u32);
    pub fn mb3d_node_visible(node: u32, visible: u32);
    pub fn mb3d_node_destroy(node: u32);
    pub fn mb3d_camera(ptr: *const u8);
    pub fn mb3d_project(x: f32, y: f32, z: f32, out: *mut u8) -> i32;
    pub fn mb3d_sun(ptr: *const u8);
    pub fn mb3d_light_point(node: u32, r: f32, g: f32, b: f32, intensity: f32, range: f32) -> i32;
    pub fn mb3d_sky(ptr: *const u8);
    pub fn mb3d_post(ptr: *const u8);
    pub fn mb3d_emitter(ptr: *const u8) -> i32 = 1;
    pub fn mb3d_emit(emitter: u32, x: f32, y: f32, z: f32, dx: f32, dy: f32, dz: f32, count: u32);
    pub fn mb3d_emit_moving(emitter: u32, x: f32, y: f32, z: f32, dx: f32, dy: f32, dz: f32, count: u32, vx: f32, vy: f32, vz: f32);
    pub fn mb3d_trail(ptr: *const u8) -> i32 = 1;
    pub fn mb3d_trail_attach(trail: u32, node: u32);
    pub fn mb3d_trail_detach(trail: u32);
    pub fn mb3d_shockwave(x: f32, y: f32, z: f32, radius: f32, strength: f32, seconds: f32);
    pub fn mb3d_render();
    // 5.4 mb3d 2
    pub fn mb3d_free(kind: u32, handle: u32) -> i32;
    pub fn mb3d_instances(node: u32, ptr: *const u8, count: u32) -> i32;
    pub fn mb3d_clip_count(model: u32) -> i32;
    pub fn mb3d_clip_find(model: u32, name: *const u8, len: u32) -> i32;
    pub fn mb3d_clip_duration(model: u32, clip: u32) -> f32 = 1.0;
    pub fn mb3d_anim(node: u32, clip: u32, time: f32, weight: f32) -> i32;
    pub fn mb3d_node_find(node: u32, name: *const u8, len: u32) -> i32 = 1;
    pub fn mb3d_node_morph(node: u32, w0: f32, w1: f32, w2: f32, w3: f32);
    pub fn mb3d_node_world(node: u32, out: *mut u8) -> i32;
    pub fn mb3d_node_material(node: u32) -> i32 = 1;
    pub fn mb3d_node_fade(node: u32, start: f32, end: f32);
    pub fn mb3d_draw_distance(start: f32, end: f32);
    pub fn mb3d_skin(node: u32, ptr: *const u8, count: u32) -> i32;
    pub fn mb3d_fog(ptr: *const u8);
    pub fn mb3d_material_style(material: u32, ptr: *const u8) -> i32;
    pub fn mb3d_text(node: u32, desc: *const u8, text: *const u8, len: u32) -> i32;
    pub fn mb3d_dof(ptr: *const u8);
    pub fn mb3d_speed(ptr: *const u8);
    pub fn mb3d_reflector(ptr: *const u8);
    pub fn mb3d_atmosphere(ptr: *const u8);
    pub fn mb3d_clouds(ptr: *const u8);
    pub fn mb3d_sky_disc(ptr: *const u8);
    pub fn mb3d_sky_texture(texture: u32, ptr: *const u8) -> i32;
    pub fn mb3d_ripple(x: f32, z: f32, strength: f32, speed: f32, seconds: f32);
    pub fn mb3d_wake(wake: u32, x: f32, z: f32, vx: f32, vz: f32, size: f32, strength: f32) -> i32;
    pub fn mb3d_water(ptr: *const u8);
    pub fn mb3d_grade(ptr: *const u8);
    pub fn mb3d_shafts(ptr: *const u8);
    pub fn mb3d_halos(ptr: *const u8);
    pub fn mb3d_film(ptr: *const u8);
    pub fn mb3d_material_get(material: u32, out: *mut u8) -> i32 = -1;
    pub fn mb3d_model_materials(model: u32, out: *mut u8, cap: u32) -> i32;
    pub fn mb3d_model_node_names(model: u32, out: *mut u8, cap: u32) -> i32;
    pub fn mb3d_retro(ptr: *const u8);
    pub fn mb3d_texture_filter(kind: u32, handle: u32, mode: u32) -> i32;
    pub fn mb3d_backdrop(ptr: *const u8, count: u32) -> i32;
    pub fn mb3d_anim_mask(node: u32, clip: u32, time: f32, weight: f32, ptr: *const u8, count: u32) -> i32;
    pub fn mb3d_anim_take(node: u32, out: *mut u8) -> i32;
    pub fn mb3d_node_get_parent(node: u32) -> i32;

    // 5.6 audio
    pub fn mb_sound(asset: u32) -> i32;
    pub fn mb_play(sound: u32, vol: f32, pan: f32, pitch: f32, looped: u32) -> u32;
    pub fn mb_play_at(sound: u32, vol: f32, pan: f32, pitch: f32, looped: u32, at: f64) -> u32;
    pub fn mb_play_from(sound: u32, vol: f32, pan: f32, pitch: f32, looped: u32, at: f64, from: f64, fade: f32) -> u32;
    pub fn mb_voice_set(voice: u32, vol: f32, pan: f32, pitch: f32);
    pub fn mb_voice_stop(voice: u32);
    pub fn mb_voice_stop_at(voice: u32, at: f64, fade: f32);
    pub fn mb_voice_bus(voice: u32, bus: u32);
    pub fn mb_bus_fx(bus: u32, kind: u32, ptr: *const u8) -> i32;

    // 5.7 sensors
    pub fn mb_tilt(out: *mut u8) -> i32 = -6;
    pub fn mb_motion(out: *mut u8) -> i32 = -6;
    pub fn mb_attitude(out: *mut u8) -> i32 = -6;
    pub fn mb_loudness() -> f32 = -1.0;
    pub fn mb_haptic(kind: u32);

    // 5.8 store / score
    pub fn mb_store_get(key: *const u8, key_len: u32, buf: *mut u8, cap: u32) -> i32 = unsafe { crate::sys::test::host::store_get(key, key_len, buf, cap) };
    pub fn mb_store_set(key: *const u8, key_len: u32, val: *const u8, val_len: u32) -> i32 = unsafe { crate::sys::test::host::store_set(key, key_len, val, val_len) };
    pub fn mb_score_submit(board: u32, value: i64) -> i32 = crate::sys::test::host::score_submit(board, value);
    pub fn mb_score_show(board: u32) = crate::sys::test::host::score_show(board);

    // 5.9 links
    pub fn mb_link_open(ptr: *const u8, len: u32) -> i32;
}
