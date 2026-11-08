//! Phase 3c (docs/STUDIO-2.md §9.4): `spring`, `fx.punch`, `fx.slowmo`,
//! the audio buses, placed sound and foley.

use std::rc::Rc;

use crate::input::Held;
use crate::project::{Files, Project};
use crate::tests::with_main;
use crate::vm::{Mode, Vm};

fn vm(files: &Files) -> Vm {
    let p = Rc::new(Project::parse(files).unwrap());
    let im: Vec<_> = p.images.iter().map(crate::scene_image).collect();
    let mut v = Vm::new(&p, 7, Mode::Round, &im, &[], &[], (0.0, 0.0, 360.0, 640.0)).unwrap_or_else(|e| panic!("{e}"));
    // `mb play`'s trace: the sounds a round starts, logged.
    v.host_mut().sound.foley.mixer.trace = true;
    v
}

fn step(v: &mut Vm, n: usize, dt: f32) {
    for _ in 0..n {
        v.step(dt, &[], &Held::default(), false, false).unwrap_or_else(|e| panic!("{e}"));
    }
}

fn printed(v: &Vm) -> Vec<String> {
    v.host().draw.texts().map(str::to_string).collect()
}

fn run(main: &str, n: usize) -> (Vec<String>, Vec<String>) {
    let mut v = vm(&with_main(main, &[]));
    step(&mut v, n, 1.0 / 60.0);
    (printed(&v), v.host().sound.lines().to_vec())
}

fn error_of(main: &str) -> String {
    let f = with_main(main, &[]);
    let p = Rc::new(Project::parse(&f).unwrap());
    match Vm::new(&p, 7, Mode::Round, &[], &[], &[], (0.0, 0.0, 360.0, 640.0)) {
        Err(e) => e,
        Ok(mut v) => v.step(1.0 / 60.0, &[], &Held::default(), false, false).expect_err("an error"),
    }
}

/// A spring heads for its goal, overshooting under damping 1 and not at 1;
/// calling it again retargets from where it is, at its speed; a write
/// stops it; tables, the camera and buses spring too.
#[test]
fn springs_head_for_their_goals() {
    let main = r#"
        local a = Shape{ shape = "rect", w = 10, h = 10, x = 0 }
        local b = Shape{ shape = "rect", w = 10, h = 10, x = 0 }
        local t = { v = 0 }
        spring(a, { x = 100 }, { frequency = 2, damping = 0.3 })
        spring(b, { x = 100, y = 50 }, { frequency = 2, damping = 1 })
        spring(t, { v = 1 })
        spring(audio.bus("music"), { lowpass = 800 }, { frequency = 3, damping = 1 })
        local peakA, peakB, frames = 0, 0, 0
        function love.update(dt)
          frames = frames + 1
          peakA = math.max(peakA, a.x)
          peakB = math.max(peakB, b.x)
          if frames == 200 then spring(a, { x = -50 }) end
          if frames == 230 then b.x = 7 end
        end
        function love.draw()
          love.graphics.print(string.format("%.2f %.2f %.2f %.2f %.3f %.0f", a.x, peakA, b.x, peakB, t.v, audio.bus("music").lowpass))
        end
    "#;
    let mut v = vm(&with_main(main, &[]));
    step(&mut v, 180, 1.0 / 60.0);
    let p = printed(&v);
    let w: Vec<f64> = p[0].split(' ').map(|s| s.parse().unwrap()).collect();
    assert!((w[0] - 100.0).abs() < 0.5, "a settles on its goal: {p:?}");
    assert!(w[1] > 115.0, "damping 0.3 overshoots: {p:?}");
    assert!((w[2] - 100.0).abs() < 0.01 && w[3] <= 100.0 + 1e-3, "damping 1 doesn't: {p:?}");
    assert!((w[4] - 1.0).abs() < 0.01, "a table's number springs: {p:?}");
    assert!((w[5] - 800.0).abs() < 1.0, "a bus's filter springs: {p:?}");
    step(&mut v, 60, 1.0 / 60.0);
    let w: Vec<f64> = printed(&v)[0].split(' ').map(|s| s.parse().unwrap()).collect();
    assert!(w[0] < 0.0, "retargeted to -50: {w:?}");
    assert_eq!(w[2], 7.0, "a write stops the spring");
    // The same at any frame rate (it's integrated exactly).
    let at = |fps: f32| {
        let mut v = vm(&with_main("local a = Shape{ shape = 'rect', w = 1, h = 1 } spring(a, { x = 100 }, { frequency = 3, damping = 0.4 }) function love.draw() love.graphics.print(string.format('%.3f', a.x)) end", &[]));
        step(&mut v, (0.5 * fps) as usize, 1.0 / fps);
        printed(&v)[0].parse::<f64>().unwrap()
    };
    assert!((at(30.0) - at(120.0)).abs() < 0.01, "{} {}", at(30.0), at(120.0));
    assert!(error_of("spring(Shape{ shape = 'rect' }, { colour = 1 })").contains("no number `colour`"));
    assert!(error_of("spring(Shape{ shape = 'rect' }, { color = 0xff0000 })").contains("is a colour: tween it"));
    assert!(error_of("spring(Shape{ shape = 'rect' }, 5)").contains("the values to head for"));
    assert!(error_of("spring(audio.bus('music'), { pitch = 1 })").contains("a bus moves volume, lowpass and highpass"));
}

/// A punch kicks the size and springs back to exactly where it was, over
/// whatever else moves the node; a place punch throws it out and back.
#[test]
fn punches_spring_back_over_other_motion() {
    let main = r#"
        local a = Shape{ shape = "rect", w = 10, h = 10, x = 0, vx = 60 }
        local b = Shape{ shape = "rect", w = 10, h = 10, x = 0, vx = 60 }
        local c = Shape{ shape = "rect", w = 10, h = 10, x = 200, y = 200, scale = 2 }
        local frames, peak, far = 0, 0, 0
        function love.update(dt)
          frames = frames + 1
          if frames == 2 then fx.punch(a, 0.3) fx.punch(c, { y = -20 }) end
          peak = math.max(peak, a.sx)
          far = math.min(far, c.y - 200)
          if frames == 10 then c.scale = 3 end
        end
        function love.draw()
          love.graphics.print(string.format("%.4f %.4f %.4f %.4f %.3f %.4f %.4f", a.sx, a.sy, a.x - b.x, peak, far, c.y, c.sx))
        end
    "#;
    let mut v = vm(&with_main(main, &[]));
    step(&mut v, 3, 1.0 / 60.0);
    let p = printed(&v);
    let w: Vec<f64> = p[0].split(' ').map(|s| s.parse().unwrap()).collect();
    assert!(w[0] > 1.2 && w[0] == w[1], "kicked 30%: {p:?}");
    step(&mut v, 180, 1.0 / 60.0);
    let w: Vec<f64> = printed(&v)[0].split(' ').map(|s| s.parse().unwrap()).collect();
    assert_eq!((w[0], w[1]), (1.0, 1.0), "back exactly");
    assert_eq!(w[2], 0.0, "its velocity moved it as it moved the unpunched one");
    assert!(w[3] > 1.25, "{w:?}");
    assert!(w[4] < -12.0 && w[4] > -26.0, "thrown about 20 up: {w:?}");
    assert_eq!((w[5], w[6]), (200.0, 3.0), "back to its place; the written scale kept");
    assert!(error_of("fx.punch(5)").contains("bad argument #1 to 'fx.punch' (node expected"));
    assert!(error_of("fx.punch(Shape{ shape = 'rect' }, 'big')").contains("amount is how much bigger"));
}

/// A punch on a key a tween or a spring is moving: the motion's writes
/// have no kick in them, so the punch goes on from each, and the value
/// settles exactly where the motion ends. A punch or slow motion given
/// NaN does nothing for good.
#[test]
fn punches_ride_tweens_and_springs_and_ignore_nan() {
    let main = r#"
        local a = Shape{ shape = "rect", w = 10, h = 10 }
        local b = Shape{ shape = "rect", w = 10, h = 10, name = "b" }
        local c = Shape{ shape = "rect", w = 10, h = 10 }
        local frames = 0
        function love.update(dt)
          frames = frames + 1
          if frames == 2 then
            flux.to(a, 0.5, { sx = 2, sy = 2 })
            spring(b, { x = 100 }, { frequency = 3, damping = 1 })
            fx.punch(c, 0 / 0)
            fx.slowmo(0.5, 0 / 0)
          end
          if frames == 10 then fx.punch(a, 0.4) fx.punch(b, { x = 60 }) end
        end
        function love.draw()
          love.graphics.print(string.format("%.6f %.6f %.6f %.6f", a.sx, a.sy, b.x, c.sx))
        end
    "#;
    let mut v = vm(&with_main(main, &[]));
    // The punch shows as drawn (after Lua's frame), over the spring's value.
    let b = v.host().scene.find("b").unwrap();
    let (mut peak, mut spring_peak) = (0.0f32, 0.0f32);
    for _ in 0..60 {
        step(&mut v, 1, 1.0 / 60.0);
        peak = peak.max(v.host().scene.get(b).unwrap().x);
        spring_peak = spring_peak.max(printed(&v)[0].split(' ').nth(2).unwrap().parse().unwrap());
    }
    assert!(peak > spring_peak + 20.0, "the punch threw it out over the spring: {peak} {spring_peak}");
    step(&mut v, 240, 1.0 / 60.0);
    let w: Vec<f64> = printed(&v)[0].split(' ').map(|s| s.parse().unwrap()).collect();
    assert_eq!((w[0], w[1]), (2.0, 2.0), "the tween's end, punched and settled: {w:?}");
    assert_eq!(w[2], 100.0, "the spring's goal, punched and settled: {w:?}");
    assert_eq!(v.host().scene.get(b).unwrap().x, 100.0);
    assert_eq!(w[3], 1.0, "a NaN punch is none");
    let dt = v.host().dt;
    assert!((dt - 1.0 / 60.0).abs() < 1e-6, "a NaN hold is the default second: full speed again ({dt})");
}

/// Slow motion slows the world's dt (love.update's, tweens', velocities')
/// and eases back on real time; round.time keeps real time.
#[test]
fn slow_motion_slows_the_world_and_ends() {
    let main = r#"
        local n = Shape{ shape = "rect", w = 1, h = 1, vx = 60 }
        local worlds, frames, minK = 0, 0, 1
        function love.update(dt)
          frames = frames + 1
          if frames == 2 then fx.slowmo(0.25, 1) end
          worlds = worlds + dt
          if frames > 2 then minK = math.min(minK, dt * 60) end
        end
        function love.draw()
          love.graphics.print(string.format("%.3f %.3f %.3f %.2f", worlds, round.time, minK, n.x))
        end
    "#;
    let mut v = vm(&with_main(main, &[]));
    step(&mut v, 60, 1.0 / 60.0);
    let w: Vec<f64> = printed(&v)[0].split(' ').map(|s| s.parse().unwrap()).collect();
    assert!((w[1] - 1.0).abs() < 1e-3, "round.time is real time: {w:?}");
    assert!(w[0] < 0.45, "the world ran slow: {w:?}");
    assert!((w[2] - 0.25).abs() < 1e-3, "down to a quarter: {w:?}");
    assert!((w[3] - w[0] * 60.0).abs() < 0.5, "velocities by the world's dt: {w:?}");
    // Past its hold and the ease back: full speed.
    step(&mut v, 120, 1.0 / 60.0);
    let before = printed(&v)[0].split(' ').next().unwrap().parse::<f64>().unwrap();
    step(&mut v, 60, 1.0 / 60.0);
    let after = printed(&v)[0].split(' ').next().unwrap().parse::<f64>().unwrap();
    assert!((after - before - 1.0).abs() < 1e-3, "full speed again: {before} {after}");
    assert!(error_of("fx.slowmo('slow')").contains("rate is the world's speed"));
}

/// The numbers a spring and slow motion give a game are the same natively
/// as in wasm: this is what the same Lua printed through `mb play` (V8) when
/// 3c was built.
#[test]
fn juice_numbers_are_the_same_in_lua() {
    let main = r#"
        local a = Shape{ shape = "rect", w = 1, h = 1 }
        spring(a, { x = 100, y = -40 }, { frequency = 1.7, damping = 0.35 })
        local sum, frames = 0, 0
        function love.update(dt)
          frames = frames + 1
          if frames == 30 then fx.slowmo(0.3, 0.5) end
          if frames == 50 then spring(a, { x = 20 }, { frequency = 2.5, damping = 0.6 }) end
          sum = sum + a.x * 0.7 + a.y * 1.3 + dt
        end
        function love.draw() love.graphics.print(string.format("juice sum %.6f", sum)) end
    "#;
    let (p, _) = run(main, 240);
    assert_eq!(p, ["juice sum -5595.691576"]);
}

/// Buses take their volume, filters, reverb and delay, read them back, and
/// say what they have; love.audio.setVolume is the sfx bus's volume.
#[test]
fn buses_take_effects() {
    let main = r#"
        local m = audio.bus("music")
        m.lowpass = 600
        m.reverb = "hall"
        m.delay = { time = 0.25, feedback = 0.4 }
        love.audio.setVolume(0.5)
        local s = audio.bus("sfx")
        function love.draw()
          love.graphics.print(string.format("%s %.0f %s %.2f %.2f %.1f %s", m.name, m.lowpass, m.reverb, m.delay.time, m.delay.feedback, s.volume, tostring(audio.bus("master").delay)))
        end
    "#;
    let (p, _) = run(main, 1);
    assert_eq!(p, ["music 600 hall 0.25 0.40 0.5 nil"]);
    assert!(error_of("audio.bus('musik')").contains("\"master\", \"music\" or \"sfx\""));
    assert!(error_of("audio.bus('music').lowpas = 3").contains("did you mean `lowpass`?"));
    assert!(error_of("audio.bus('music').reverb = 'church'").contains("\"room\", \"hall\", \"cave\" or \"off\""));
    assert!(error_of("audio.bus('music').volume = 'loud'").contains("takes a number"));
    assert!(error_of("love.audio.setEffect('myEffect', { type = 'reverb' })").contains("effects are a bus's in 2.0"));
}

/// Footsteps on a Walker sound at its footfalls (as `stepped` fires); a
/// hum follows its speed, rising and falling silent; an impact is louder
/// the harder; a placed source pans to its side. The same run logs the
/// same sounds.
#[test]
fn foley_follows_the_walker() {
    let main = r#"
        local w = Walker{ x = 180, y = 300, height = 60, footsteps = "catch", hum = "bonus" }
        local steps = {}
        w.stepped:connect(function() steps[#steps + 1] = string.format("%.3f", round.time) end)
        local miss = love.audio.newSource("miss", "static")
        local right = miss:clone()
        right:setPosition(500, 300)
        local frames = 0
        function love.update(dt)
          frames = frames + 1
          if frames == 10 then w.vx = 150 end
          if frames == 100 then w.vx = 0 end
          if frames == 150 then audio.impact(miss, 2, 12) right:play() end
          if frames == 160 then audio.impact(miss, 12, 12) end
        end
        function love.draw() love.graphics.print(table.concat(steps, " ") .. "|" .. tostring(w.footsteps) .. " " .. tostring(w.hum)) end
    "#;
    let (p, lines) = run(main, 200);
    let (steps, props) = p[0].split_once('|').unwrap();
    assert_eq!(props, "catch bonus");
    let steps: Vec<f64> = steps.split(' ').map(|s| s.parse().unwrap()).collect();
    let plays: Vec<f64> = lines.iter().filter(|l| l.contains(" catch ") && l.contains(" play ")).map(|l| l.split(' ').next().unwrap().parse().unwrap()).collect();
    assert!(steps.len() >= 3, "{p:?}");
    // `stepped` is heard the frame after the foot lands; its sound plays as it lands.
    assert_eq!(plays.len(), steps.len(), "a footstep each footfall: {lines:#?}");
    for (s, f) in plays.iter().zip(&steps) {
        assert!((f - s - 1.0 / 60.0).abs() < 1e-3, "{s} {f}");
    }
    let hum: Vec<&String> = lines.iter().filter(|l| l.contains(" bonus ")).collect();
    assert!(hum[0].contains(" loop "), "{hum:?}");
    assert!(hum.iter().any(|l| l.contains(" set ")), "its level follows the speed: {hum:?}");
    assert!(hum.last().unwrap().contains(" stop "), "and stops when it does: {hum:?}");
    let miss: Vec<Vec<&str>> = lines.iter().filter(|l| l.contains(" miss ")).map(|l| l.split(' ').collect()).collect();
    assert_eq!(miss.len(), 3, "{miss:?}");
    let vol = |i: usize| miss[i][2].parse::<f64>().unwrap();
    assert!(vol(2) > vol(0) * 1.5, "harder is louder: {miss:?}");
    assert!(miss[1][6].starts_with("pan=0.") && miss[1][5] == "id=s2", "placed to the right: {miss:?}");
    assert_eq!(run(main, 200).1, lines, "the same sounds every run");
    assert!(error_of("Walker{ footsteps = 'thud' }").contains("footsteps: no sound \"thud\""));
    assert!(error_of("Shape{ shape = 'rect', footsteps = 'catch' }").contains("footsteps"));
}

/// A 3D project with this main.lua (and the catch starter's sounds' names).
fn three(main: &str) -> Files {
    let mut f = Files::new();
    f.insert("studio.toml".into(), b"title = \"T\"\nengine = \"2.0\"\nkind = \"3d\"\nlives = 3\n".to_vec());
    f.insert("sounds/hum.ogg".into(), Vec::new());
    f.insert("sounds/clang.ogg".into(), Vec::new());
    f.insert("game/main.lua".into(), main.as_bytes().to_vec());
    crate::tests::compile(&mut f);
    f
}

/// In 3D: a body's size punches but its place doesn't (the physics own
/// it); a crate falls the same distance over the same steps, slowed, as
/// its world's dt says; a hum follows a moving node from the camera, and
/// a placed clang to the camera's left pans left.
#[test]
fn juice_in_3d_leaves_physics_to_the_physics() {
    let main = r#"
        local floor = StaticBody3D{ shape = BoxShape3D{ w = 40, h = 1, d = 40 }, y = -0.5 }
        local crate = RigidBody3D{ shape = BoxShape3D{ w = 1, h = 1, d = 1 }, y = 10 }
        local cart = Group{ z = -8, hum = "hum" }
        local clang = love.audio.newSource("clang", "static")
        camera.x, camera.y, camera.z = 0, 2, 10
        camera:lookAt(0, 2, 0)
        local frames, worlds = 0, 0
        function love.update(dt)
          frames = frames + 1
          worlds = worlds + dt
          if frames == 2 then fx.punch(crate, 0.3) fx.slowmo(0.5, 0.5) end
          if frames > 20 and frames < 80 then cart.x = cart.x + 6 * dt end
          if frames == 100 then clang:setPosition(-10, 2, 0) clang:play() end
        end
        function love.draw() love.graphics.print(string.format("%.4f %.4f %.4f", crate.y, crate.sx, worlds)) end
    "#;
    let mut v = vm(&three(main));
    step(&mut v, 30, 1.0 / 60.0);
    let w: Vec<f64> = printed(&v)[0].split(' ').map(|s| s.parse().unwrap()).collect();
    // Under gravity from rest, by its world's time (fixed steps, so near it).
    let fell = 10.0 - w[0];
    assert!((fell - 0.5 * 9.8 * w[2] * w[2]).abs() < 0.25, "the crate fell by the slowed time: {w:?}");
    assert!(w[2] < 0.4, "slowed: {w:?}");
    step(&mut v, 90, 1.0 / 60.0);
    let w: Vec<f64> = printed(&v)[0].split(' ').map(|s| s.parse().unwrap()).collect();
    assert_eq!(w[1], 1.0, "the punch settled");
    let lines = v.host().sound.lines().to_vec();
    assert!(lines.iter().any(|l| l.contains(" hum ") && l.contains(" loop ")), "{lines:?}");
    assert!(lines.iter().any(|l| l.contains(" hum ") && l.contains(" set ")), "{lines:?}");
    let clang: Vec<&String> = lines.iter().filter(|l| l.contains(" clang ")).collect();
    assert!(clang.len() == 1 && clang[0].contains("pan=-0.42"), "placed left of the camera: {clang:?}");
    let e = error_of3("local b = RigidBody3D{ shape = SphereShape3D{ radius = 1 } }\nfx.punch(b, { y = 1 })");
    assert!(e.contains("a physics body's place is the physics'"), "{e}");
}

fn error_of3(main: &str) -> String {
    let p = Rc::new(Project::parse(&three(main)).unwrap());
    match Vm::new(&p, 7, Mode::Round, &[], &[], &[], (0.0, 0.0, 360.0, 640.0)) {
        Err(e) => e,
        Ok(mut v) => v.step(1.0 / 60.0, &[], &Held::default(), false, false).expect_err("an error"),
    }
}

/// The title card's attract mode makes no sound: nothing is logged.
#[test]
fn attract_mode_foley_is_silent() {
    let f = with_main("local w = Walker{ x = 0, y = 300, vx = 150, height = 60, footsteps = 'catch' }", &[]);
    let p = Rc::new(Project::parse(&f).unwrap());
    let mut v = Vm::new(&p, 7, Mode::Title, &[], &[], &[], (0.0, 0.0, 360.0, 640.0)).unwrap();
    v.host_mut().sound.foley.mixer.trace = true;
    for _ in 0..90 {
        v.step(1.0 / 60.0, &[], &Held::default(), true, false).unwrap();
    }
    assert!(v.host().sound.lines().is_empty());
}
