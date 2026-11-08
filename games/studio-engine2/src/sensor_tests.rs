//! `sensors` (api/sensors.rs, docs/STUDIO-2.md §9.4 3e) natively: what Lua
//! reads, the declarations, the scope, `player.tilt`, the smoothness
//! check's poses, and a sensor scene that prints what `mb play` prints in
//! V8 (crates/mb-cli/tests/replay.rs `mb_play_scripts_the_sensors`).

use std::rc::Rc;

use maimbrain::sensors::{Attitude, Tilt};
use maimbrain::sys::test::{Call, Host, Sensed as HostSensed};

use crate::api::sensors::Sensed;
use crate::input::Held;
use crate::project::{Files, Project};
use crate::tests::{compile, with_main};
use crate::vm::{Mode, Vm};
use crate::{Studio2, use_project};

/// `with_main` with studio.toml's `sensors` set.
fn with_sensors(main: &str, sensors: &str) -> Files {
    let mut f = with_main(main, &[]);
    let toml = String::from_utf8(f["studio.toml"].clone()).unwrap();
    let toml = toml.replacen("title", &format!("sensors = {sensors}\ntitle"), 1);
    f.insert("studio.toml".into(), toml.into_bytes());
    compile(&mut f);
    f
}

fn vm(files: &Files) -> Vm {
    let p = Rc::new(Project::parse(files).unwrap_or_else(|e| panic!("{e}")));
    let im: Vec<_> = p.images.iter().map(crate::scene_image).collect();
    Vm::new(&p, 7, Mode::Round, &im, &[], &[], (0.0, 0.0, 360.0, 640.0)).unwrap_or_else(|e| panic!("{e}"))
}

fn step(v: &mut Vm, s: Sensed) {
    v.step_sensed(1.0 / 60.0, &[], &Held::default(), false, false, s).unwrap_or_else(|e| panic!("{e}"));
}

fn printed(v: &Vm) -> Vec<String> {
    v.host().draw.texts().map(str::to_string).collect()
}

fn tilted(roll_deg: f32) -> Sensed {
    let r = roll_deg.to_radians();
    Sensed { tilt: Some(Tilt { x: r.sin(), y: r.cos(), z: 0.0 }), ..Sensed::default() }
}

/// Each reads its sensor in radians and g, nil without a reading.
#[test]
fn lua_reads_the_sensors() {
    let main = r#"
        local t = ""
        function love.update()
          local r, p = sensors.tilt()
          local y, ap, ar = sensors.attitude()
          local x = sensors.motion()
          local l = sensors.loudness()
          t = string.format("%s %s %s %s", r and string.format("%.3f %.3f", r, p) or "nil", y and string.format("%.3f %.3f %.3f", y, ap, ar) or "nil",
            x and string.format("%.2f", x) or "nil", l and string.format("%.2f", l) or "nil")
        end
        function love.draw() love.graphics.print(t, 0, 0) end
    "#;
    let mut v = vm(&with_sensors(main, r#"["haptics", "tilt", "motion", "attitude", "loudness"]"#));
    step(&mut v, Sensed::default());
    assert_eq!(printed(&v), ["nil nil nil nil"]);
    let a = |yaw: f32| Attitude::from_angles(yaw.to_radians(), 10f32.to_radians(), 0.0);
    let s = Sensed { attitude: Some(a(40.0)), loudness: Some(0.85), motion: Some(maimbrain::sensors::Motion { x: 2.0, y: 0.0, z: 0.0 }), ..tilted(30.0) };
    step(&mut v, s);
    assert_eq!(printed(&v), ["0.524 -0.000 0.000 0.175 0.000 2.00 0.85"], "yaw from the round's first reading");
    step(&mut v, Sensed { attitude: Some(a(70.0)), ..s });
    assert_eq!(printed(&v), ["0.524 -0.000 0.524 0.175 0.000 2.00 0.85"], "turned 30° right since");
}

/// A sensor studio.toml doesn't list is an error that says what to add;
/// an unknown one is studio.toml's error.
#[test]
fn sensors_are_declared() {
    let mut v = vm(&with_main("function love.update() sensors.loudness() end", &[]));
    let e = v.step(1.0 / 60.0, &[], &Held::default(), false, false).unwrap_err();
    assert!(e.contains("sensors.loudness() needs sensors = [\"loudness\"] in studio.toml"), "{e}");
    let f = with_sensors("function love.load() sensors.scope() end", r#"["haptics"]"#);
    let p = Rc::new(Project::parse(&f).unwrap());
    let e = Vm::new(&p, 7, Mode::Round, &[], &[], &[], (0.0, 0.0, 360.0, 640.0)).err().expect("an error in love.load");
    assert!(e.contains("sensors.scope needs sensors = [\"attitude\"]"), "{e}");
    let f = with_sensors("", r#"["haptics", "light"]"#);
    assert_eq!(Project::parse(&f).err().unwrap(), "studio.toml: sensors can be haptics, tilt, motion, attitude and loudness, not \"light\"");
}

/// A scope follows the phone (8° of a 32° sweep is a quarter of the width),
/// recenters, and a finger moves it too.
#[test]
fn a_scope_aims_by_pointing_and_by_dragging() {
    let main = r#"
        local s, t, n = nil, nil, 0
        function love.load() s = sensors.scope() end
        function love.update()
          n = n + 1
          if n == 20 then s:recenter() end
          if n == 31 then s:drag(10, -20) end
          t = string.format("%.0f %.0f %s", s.x, s.y, s.source)
        end
        function love.draw() love.graphics.print(t, 0, 0) end
    "#;
    let mut v = vm(&with_sensors(main, r#"["haptics", "attitude"]"#));
    let at = |yaw: f32| Sensed { attitude: Some(Attitude::from_angles(yaw.to_radians(), 0.0, 0.0)), ..Sensed::default() };
    step(&mut v, at(0.0));
    for _ in 0..10 {
        step(&mut v, at(8.0));
    }
    assert_eq!(printed(&v), ["269 320 attitude"]);
    for _ in 0..10 {
        step(&mut v, at(8.0));
    }
    assert_eq!(printed(&v), ["180 320 attitude"], "recentred where it points");
    for _ in 0..12 {
        step(&mut v, at(8.0));
    }
    assert_eq!(printed(&v), ["190 300 attitude"], "dragged");
    // Without a reading, the finger alone aims.
    step(&mut v, Sensed::default());
    assert_eq!(printed(&v), ["190 300 drag"]);
}

/// `player.tilt` rolls by tilt while no finger drags it.
#[test]
fn player_tilt_steers_by_tilt() {
    let main = r#"
        local ball
        function love.load()
          kind("ball", { w = 16, h = 16, behaviour = "player.tilt" })
          ball = spawn("ball", { x = 180, y = 500 })
        end
        function love.draw() love.graphics.print(string.format("%.0f", ball.x), 0, 0) end
    "#;
    let mut v = vm(&with_main(main, &[]));
    let x = |v: &Vm| printed(v)[0].parse::<f32>().unwrap();
    for _ in 0..12 {
        step(&mut v, tilted(10.0));
    }
    let right = x(&v);
    assert!(right > 200.0 && right < 340.0, "tipped right, it rolls right: {right}");
    for _ in 0..12 {
        step(&mut v, tilted(-10.0));
    }
    assert!((x(&v) - 180.0).abs() < 2.0, "and back as far tipped left: {}", x(&v));
    for _ in 0..12 {
        step(&mut v, Sensed::default());
    }
    assert!((x(&v) - 180.0).abs() < 2.0, "without a reading it stays: {}", x(&v));
}

/// The smoothness check's poses: the camera without its shake, the
/// subject, the things on the view, and the marks (a hitstop, debug.mark).
#[test]
fn poses_say_where_the_camera_and_the_subject_are() {
    let main = r#"
        local fox
        function love.load()
          fox = Shape{ shape = "circle", radius = 10, name = "fox", x = 100, y = 300 }
          fox.vx = 120
          round.subject = fox
          Shape{ name = "coin", x = 200, y = 200 }
        end
        local n = 0
        function love.update()
          camera.x = 200
          n = n + 1
          if n == 6 then fx.shake(20, 1); fx.hitstop(0.05); debug.mark("dash") end
        end
    "#;
    let host = Host::start(5);
    maimbrain::sys::test::set_param("studio_smooth", "1");
    use_project(with_main(main, &[]));
    let mut g: Studio2 = host.init();
    host.run(&mut g, 0.2, 1.0 / 60.0);
    host.tap(180.0, 320.0);
    host.run(&mut g, 0.3, 1.0 / 60.0);
    let poses: Vec<String> = host.calls().into_iter().filter_map(|c| if let Call::Log { message, .. } = c { message.strip_prefix("studio-pose ").map(str::to_string) } else { None }).collect();
    assert!(poses.len() >= 15, "a pose a frame of the round: {}", poses.len());
    let marked: Vec<&String> = poses.iter().filter(|p| p.contains("\"dash\"")).collect();
    assert_eq!(marked.len(), 1, "{poses:#?}");
    assert!(marked[0].contains("\"cam\":[200.000,320.000,1.0000,0.0000]"), "no shake: {}", marked[0]);
    assert!(poses.iter().any(|p| p.contains("\"hitstop\"")), "a hitstop marks itself");
    assert!(marked[0].contains("\"subject\":[\"fox\",\"2d\""), "{}", marked[0]);
    assert!(marked[0].contains("\"coin\",") && !marked[0].contains("[\"fox\"]"), "the things on the view: {}", marked[0]);
}

// ------------------------------------------- the same in V8 (mb play)

/// The sensor scene `mb play` plays in crates/mb-cli/tests/replay.rs.
pub const SENSED_MAIN: &str = r#"
local scope, sum, n
function love.load() scope = sensors.scope{ precise = true }; sum, n = 0, 0 end
function love.update(dt)
  if round.title then return end
  n = n + 1
  local r, p = sensors.tilt()
  local y, ap, ar = sensors.attitude()
  local l = sensors.loudness()
  if r then sum = sum + r * 1.5 + p * 0.7 end
  if y then sum = sum + y * 2.1 + ap * 0.3 + ar end
  if l then sum = sum + l * 0.05 end
  sum = sum + scope.x * 0.001 + scope.y * 0.002
  if n % 30 == 0 then print(string.format("sensed %d %.5f", n, sum)) end
end
"#;
pub const SENSED_TOML: &str = "title = \"Sensed\"\nengine = \"2.0\"\nkind = \"2d\"\nsensors = [\"haptics\", \"tilt\", \"attitude\", \"loudness\"]\nid = \"dev.maimbrain.studio2.sensed\"\nversion = \"0.1.0\"\ncreator = \"@maimbrain\"\n";
/// What it prints over 2.5 s with replay.rs's script (a tap at 0.05 s, tilted
/// 20° right and 10° back at 0.5 s, aimed 30° right and 10° down at 1 s, a
/// blow of 0.85 at 1.5 s), natively and in V8 alike.
pub const SENSED_PRINTS: &[&str] = &["sensed 30 28.61886", "sensed 60 91.19677", "sensed 90 188.63394", "sensed 120 286.61032"];

/// The readings `mb play` sends at game time `t` for that script (its
/// sense.rs, with the same quantizing), as the SDK's test host takes them.
fn scripted(t: f64) -> HostSensed {
    type Q = [f64; 4];
    fn qmul(a: Q, b: Q) -> Q {
        [
            a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
            a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
            a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
            a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
        ]
    }
    let axis = |x: f64, y: f64, z: f64, a: f64| [x * (a / 2.0).sin(), y * (a / 2.0).sin(), z * (a / 2.0).sin(), (a / 2.0).cos()];
    let (yaw, up, roll) = if t >= 1.0 - 1e-9 {
        (30.0, -10.0, 20.0)
    } else if t >= 0.5 - 1e-9 {
        (0.0, -10.0, 20.0)
    } else {
        (0.0f64, 0.0f64, 0.0f64)
    };
    let q = qmul(qmul(axis(0.0, -1.0, 0.0, yaw.to_radians()), axis(1.0, 0.0, 0.0, -up.to_radians())), axis(0.0, 0.0, 1.0, roll.to_radians()));
    let [x, y, z, w] = q;
    let g = [2.0 * (x * y + w * z), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - w * x)];
    let quant = |v: f64, k: f64| ((v * k).round() / k) as f32;
    let loud = if t >= 1.5 - 1e-9 { 0.85 } else { 0.45 };
    HostSensed {
        tilt: Some(g.map(|v| quant(v, 4096.0))),
        motion: None,
        attitude: Some([quant(q[0], 32768.0), quant(q[1], 32768.0), quant(q[2], 32768.0), quant(q[3], 32768.0), 0.0, 0.0, 0.0]),
        loudness: Some(quant(loud, 1024.0)),
    }
}

/// The scene natively, on the SDK's test host, read frame by frame as the
/// replay log sends it: frame f's reading is the one at f / 60 s.
#[test]
fn the_sensor_scene_prints_what_v8_prints() {
    let host = Host::start(1);
    let mut f = Files::new();
    f.insert("studio.toml".into(), SENSED_TOML.as_bytes().to_vec());
    f.insert("game/main.lua".into(), SENSED_MAIN.as_bytes().to_vec());
    compile(&mut f);
    use_project(f);
    let mut g: Studio2 = host.init();
    for frame in 0..151 {
        host.sense(scripted(frame as f64 / 60.0));
        if frame == 3 {
            host.tap(180.0, 384.0);
        }
        host.frame(&mut g, 1.0 / 60.0);
    }
    let lines: Vec<String> = host.calls().into_iter().filter_map(|c| if let Call::Log { message, .. } = c { message.starts_with("sensed ").then_some(message) } else { None }).collect();
    assert_eq!(lines, SENSED_PRINTS);
}
