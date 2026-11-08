//! 3D games, natively (docs/STUDIO-2.md §9.2 row 1b): engine 1.0's 3D names
//! through the scene's 3D side, the batched flush, the tunnel starter
//! playing a round and replaying, and 1.0's tunnel running on the 3D build.
//! Natively every mb3d call answers handle 1 and the test host records node
//! transforms and the camera with the frame's draws (SPEC §9.8).

use std::path::Path;
use std::rc::Rc;

use maimbrain::scene::Cmd;
use maimbrain::sys::Round;
use maimbrain::sys::test::{Conformance, Host};

use crate::input::{Held, LoveEvent};
use crate::project::{Files, Project};
use crate::vm::{Mode, Vm};
use crate::{Studio2, use_project};

fn read(root: &Path) -> Files {
    let mut out = Files::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if let Ok(b) = std::fs::read(&p) {
                out.insert(p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"), b);
            }
        }
    }
    compile(&mut out);
    out
}

fn compile(files: &mut Files) {
    let lua: Vec<(String, Vec<u8>)> = files.iter().filter(|(p, _)| p.starts_with("game/") && p.ends_with(".lua")).map(|(p, b)| (p.clone(), b.clone())).collect();
    for (p, src) in lua {
        let bc = luau::compile(&src).unwrap_or_else(|e| panic!("{p}:{}: {}", e.line, e.message));
        files.insert(format!("{p}c"), bc);
    }
}

fn dir(rel: &str) -> Files {
    read(&Path::new(env!("CARGO_MANIFEST_DIR")).join(rel))
}

/// A 3D project with this main.lua (and a model, `fox`).
fn three(main: &str) -> Files {
    let mut f = Files::new();
    f.insert("studio.toml".into(), b"title = \"T\"\nengine = \"2.0\"\nkind = \"3d\"\nlives = 3\n".to_vec());
    f.insert("models/fox.glb".into(), b"glTF".to_vec());
    f.insert("sounds/hit.ogg".into(), Vec::new());
    f.insert("game/main.lua".into(), main.as_bytes().to_vec());
    compile(&mut f);
    f
}

fn vm(files: &Files) -> Result<Vm, String> {
    let p = Rc::new(Project::parse(files)?);
    Vm::new(&p, 7, Mode::Round, &[], &[], &[], (0.0, 0.0, 360.0, 640.0))
}

fn step(v: &mut Vm, n: usize) {
    for _ in 0..n {
        v.step(1.0 / 60.0, &[], &Held::default(), false, false).unwrap_or_else(|e| panic!("{e}"));
    }
}

/// What the game printed (love.draw, or 1.0's `text`) last frame.
fn printed(v: &Vm) -> Vec<String> {
    v.host().draw.texts().map(str::to_string).collect()
}

fn error_of(main: &str) -> String {
    match vm(&three(main)) {
        Err(e) => e,
        Ok(mut v) => v.step(1.0 / 60.0, &[], &Held::default(), false, false).expect_err("an error"),
    }
}

/// A node's properties write through to the scene and read back, 1.0's
/// methods are the same properties, velocity moves it in Rust, and many
/// writes in a frame are one host call at the flush.
#[test]
fn nodes_write_through_and_flush_once_a_frame() {
    let mut v = vm(&three(
        r#"
        local box = mesh.box(1, 2, 1):color(0xff0000ff)
        crate = node(box, material{ color = 0xb07a45ff, rough = 0.8 })
        crate:place(1, 2, 3, 0.5, 2)
        lid = node()
        lid:parent(crate)
        lid.y = 1
        crate.vx = 6
        function love.update()
          for i = 1, 10 do lid.x = i end
          local x, y, z = crate:pos()
          out = { x, y, z, crate.ry, crate.sz, lid.parent == crate, #crate.children, crate.alive, lid:world() }
        end
        function love.draw() for _, s in ipairs(out) do love.graphics.print(tostring(s)) end end
        "#,
    ))
    .unwrap();
    step(&mut v, 30);
    let out = printed(&v);
    // 29 frames of 6 m/s seen by the 30th update (motion runs after Lua).
    assert_eq!(out[1..8], ["2", "3", "0.5", "2", "true", "1", "true"], "{out:?}");
    let x = out[0].parse::<f64>().unwrap();
    assert!((x - (1.0 + 6.0 * 29.0 / 60.0)).abs() < 1e-4, "{out:?}");
    // The lid's world place: 10 m along the crate's turned, doubled x, 1 m up doubled.
    let (wx, wy) = (out[8].parse::<f64>().unwrap(), out[9].parse::<f64>().unwrap());
    assert!((wx - (x + 20.0 * 0.5f64.cos())).abs() < 1e-3 && (wy - 4.0).abs() < 1e-4, "{out:?}");
    let w = v.host().three.as_ref().unwrap();
    assert_eq!(w.flushed, 2, "the crate (moving) and the lid (written ten times): one call each");
    assert_eq!(w.scene.len(), 2);
}

/// `vrx`, `vry`, `vrz` spin a node in Rust after the update, as `vx` moves
/// it: a spinning node is one batched transform a frame, its angles stay
/// in −π..π, a written angle spins on from there, and a replay spins the
/// same, bit for bit.
#[test]
fn spin_turns_nodes_in_rust() {
    let src = r#"
        coin = Mesh{ geometry = geometry.box(1, 1, 1), vry = 6 }
        fan = node()
        fan.vrz, fan.vrx = -1, 0.5
        still = node()
        n = 0
        function love.update(dt)
          n = n + 1
          if n == 40 then at40 = coin.ry end
          if n == 45 then coin.ry = 1 end
          out = { at40 or 0, coin.ry, coin.vry, fan.rz, fan.rx, fan.vrz, still.ry }
        end
        function love.draw() for _, s in ipairs(out) do love.graphics.print(tostring(s)) end end
    "#;
    let run = || {
        let mut v = vm(&three(src)).unwrap();
        let mut flushed = Vec::new();
        for _ in 0..60 {
            step(&mut v, 1);
            flushed.push(v.host().three.as_ref().unwrap().flushed);
        }
        (printed(&v), flushed)
    };
    let (out, flushed) = run();
    let num = |i: usize| out[i].parse::<f64>().unwrap();
    // 39 steps of 0.1 by the 40th update: 3.9, past π, so 3.9 − 2π.
    assert!((num(0) - (3.9 - std::f64::consts::TAU)).abs() < 1e-4, "{out:?}");
    // Written 1 in the 45th update, then 15 steps by the 60th.
    assert!((num(1) - 2.5).abs() < 1e-4, "{out:?}");
    assert!((num(3) + 59.0 / 60.0).abs() < 1e-4 && (num(4) - 29.5 / 60.0).abs() < 1e-4, "{out:?}");
    assert_eq!((out[2].as_str(), out[5].as_str(), out[6].as_str()), ("6", "-1", "0"));
    assert!(flushed[1..].iter().all(|f| *f == 2), "the coin and the fan, one transform each: {flushed:?}");
    assert_eq!((out, flushed), run(), "a replay spins the same");
}

/// Bad values and unknown names are errors at their line, with what to do.
#[test]
fn writes_are_checked_at_their_line() {
    assert_eq!(error_of("local n = node()\nn.y = 'up'\n"), "game/main.lua:2: node.y takes a number, got string");
    assert_eq!(error_of("local n = node()\nn.rotation = 1\n"), "game/main.lua:2: a 3D node turns with ry (yaw), rx (pitch) and rz (roll), or node:faceDirection(dx, dz); `rotation` is 2D's");
    assert_eq!(error_of("local n = node()\nn.vr = 1\n"), "game/main.lua:2: a 3D node spins with vry (yaw), vrx (pitch) and vrz (roll), radians a second; `vr` is 2D's");
    assert_eq!(error_of("local n = node()\nn:spin(1)\n"), "game/main.lua:2: no method `spin` on Node3D");
    assert_eq!(error_of("local n = node()\nn:plce(1)\n"), "game/main.lua:2: no method `plce` on Node3D; did you mean `place`?");
    assert!(error_of("local n = node()\nn:place(1, 2)\n").contains("bad argument #3 to 'place' (number expected, got nil)"));
    assert!(error_of("local n = node()\nn.parent = Sprite{}\n").contains("a 3D node's parent is a 3D node"));
    assert!(error_of("node():play('Run')\n").contains("play: not a model"));
    assert!(error_of("model('cat')\n").contains("there's no models/cat.glb"));
    assert!(error_of("sfx('boom')\n").contains("no sound \"boom\""));
    assert!(error_of("local n = node()\nn:destroy()\nn.x = 1\n").contains("this node was destroyed"));
    assert!(error_of("spawn('coin')\n").contains("quick path's, which is 2D"));
    assert!(error_of("kind('coin', {})\n").contains("quick path's, which is 2D"));
    assert!(error_of("on('overlap', 'a', 'b', function() end)\n").starts_with("game/main.lua:1: on(\"overlap\") is the quick path's"));
}

/// A 2D game hears that the 3D names are 3D's.
#[test]
fn a_2d_game_is_told_the_3d_names_are_3ds() {
    let mut f = three("local n = node()\n");
    f.insert("studio.toml".into(), b"title = \"T\"\nengine = \"2.0\"\n".to_vec());
    f.remove("models/fox.glb");
    let e = vm(&f).err().unwrap();
    assert!(e.contains("`node` is 3D's"), "{e}");
    f.insert("game/main.lua".into(), b"camera(0, 1, 5)\n".to_vec());
    compile(&mut f);
    assert!(vm(&f).err().unwrap().contains("a 2D game moves it with camera.x"));
    f.insert("models/fox.glb".into(), b"glTF".to_vec());
    assert!(vm(&f).err().unwrap().contains("models are for 3D games"));
}

/// Models spawn and play; a part found by name is the same node each time
/// and stays the model's; flux tweens a 3D node and a write wins.
#[test]
fn models_parts_and_tweens() {
    let mut v = vm(&three(
        r#"
        local fox = spawn(model("fox"))
        fox:play("Run", { fade = 0.1 })
        local tail = fox:find("Tail")
        same = tail == fox:find("Tail")
        tail:destroy()
        local b = node()
        flux.to(b, 1, { z = -10, ry = 1 }):ease("linear")
        function love.update() if round.time > 0.49 and round.time < 0.51 then b.ry = 3 end end
        function love.draw() love.graphics.print(tostring(same) .. " " .. tostring(tail.alive) .. " " .. fox:clip() .. " " .. b.z .. " " .. b.ry) end
        "#,
    ))
    .unwrap();
    for _ in 0..4 {
        v.step(0.25, &[], &Held::default(), false, false).unwrap();
    }
    assert_eq!(printed(&v), ["true true Run -10 3"]);
}

/// 1.0's round: `score` and `lives` are round's, `title` and `over` read
/// it, `game_over` ends the round with its heading, `start()` runs, and
/// 1.0's timers, saves and maths work.
#[test]
fn engine_1_0s_round_and_timers() {
    let _host = Host::start(1); // saves
    let mut v = vm(&three(
        r#"
        ticks, waves, every_n, after_n = 0, 0, 0, 0
        function start() score = 5 end
        timer("wave", 0.5, true)
        on("timer", "wave", function() waves = waves + 1 end)
        after(0.25, function() after_n = after_n + 1 end)
        every(0.5, function() every_n = every_n + 1 return false end)
        save("best", 12)
        on("tick", function(dt)
          ticks = ticks + 1
          score = score + 1
          lives = lives - 0
          if time() >= 1 and not over then game_over("WIPEOUT!") end
          text(score, 180, 30, { size = 40 })
          rect(0, 0, 10, 10, 0xff000080)
        end)
        function love.draw()
          love.graphics.print(table.concat({ round.score, lives, tostring(title), tostring(over), waves, every_n, after_n, load("best"), tostring(rnd(2) < 2) }, " "))
        end
        "#,
    ))
    .unwrap();
    step(&mut v, 62);
    let h = v.host();
    assert_eq!(h.over.as_ref().unwrap().heading.as_deref(), Some("WIPEOUT!"));
    let texts = printed(&v);
    // Ticks 1–62 each add one to start()'s 5; the timers at 0.5 s and 1 s.
    assert_eq!(texts[2], "67 3 false true 2 2 1 12 true", "{texts:?}");
    assert_eq!(texts[..2], ["67", "67"], "text(): its shadow, then it");
    assert!(h.draw.cmds().contains(&Cmd::Rect { x: 0.0, y: 0.0, w: 10.0, h: 10.0, r: 0.0, line: 0.0, color: 0xff000080 }));
}

/// 1.0's events from LÖVE's: a finger's tap, hold and release, and buttons
/// pressed by their 1.0 names.
#[test]
fn engine_1_0s_events() {
    let mut v = vm(&three(
        r#"
        log = {}
        on("tap", function(x, y) log[#log + 1] = "tap " .. x end)
        on("hold", function(x, y) log[#log + 1] = "hold " .. x end)
        on("release", function(x, y) log[#log + 1] = "release " .. x end)
        on("press", "a", function() log[#log + 1] = "a" end)
        on("press", "jump", function() log[#log + 1] = "jump " .. tostring(input.jump) end)
        function love.draw() love.graphics.print(table.concat(log, ", ") .. " | " .. input.x .. " " .. tostring(input.down)) end
        "#,
    ))
    .unwrap();
    let down = Held::default();
    let frames: [&[LoveEvent]; 4] = [
        &[LoveEvent::MousePressed { x: 10.0, y: 5.0, button: 1, istouch: true }],
        &[LoveEvent::MouseMoved { x: 30.0, y: 5.0, dx: 20.0, dy: 0.0, istouch: true }],
        &[LoveEvent::MouseReleased { x: 40.0, y: 5.0, button: 1, istouch: true }],
        &[LoveEvent::KeyPressed("space")],
    ];
    for f in frames {
        v.step(1.0 / 60.0, f, &down, false, false).unwrap();
    }
    assert_eq!(printed(&v), ["tap 10, hold 10, hold 30, release 40, a, jump true | 40 false"]);
}

/// project() answers in canvas units, through the camera, as mb3d would.
#[test]
fn project_uses_the_camera() {
    let mut v = vm(&three(
        r#"
        camera(0, 0, 5, 0, 0, 0, 1.0)
        function love.draw()
          local x, y, on = project(0, 0, 0)
          local _, _, behind = project(0, 0, 10)
          love.graphics.print(string.format("%.1f %.1f %s %s", x, y, tostring(on), tostring(behind)))
        end
        "#,
    ))
    .unwrap();
    step(&mut v, 1);
    assert_eq!(printed(&v), ["180.0 320.0 true false"]);
}

/// The 3D build's manifest and a round on a fake host keep the platform's rules.
#[test]
fn the_tunnel_keeps_the_platform_rules() {
    const MANIFEST: &str = "abi = 0\nid = \"dev.maimbrain.studio2.tunnel\"\nname = \"Tunnel\"\nversion = \"0.1.0\"\ncreator = \"@maimbrain\"\norientation = \"portrait\"\nlogical_size = [360, 640]\ninputs = [\"touch\"]\nperf_tier = \"full\"\nstdlib = { mb3d = 2, mb2d = 3 }\nsensors = [\"haptics\"]\ncapabilities = [\"store\", \"score\"]\n[[scores]]\nboard = 0\nlabel = \"Rings\"\norder = \"higher\"\n";
    use_project(dir("starters/tunnel"));
    Conformance::new().max_round(60.0).manifest(MANIFEST).check::<Studio2>();
}

/// The tunnel plays itself on the title card, a tap starts a round, an
/// unsteered round crashes into a ring wall, and the wreck drifts on under
/// the card. `project` picks the starter: 2.0's, or 1.0's on the 3D build.
fn plays_a_round(project: &str) -> Vec<u64> {
    let host = Host::start(5);
    use_project(dir(project));
    let mut g: Studio2 = host.init();
    host.run(&mut g, 2.0, 1.0 / 60.0);
    assert!(g.error.is_none(), "{:?}", g.error);
    let w = g.title.as_ref().expect("the attract mode").host().three.as_ref().unwrap();
    assert!(w.scene.len() >= 29 && w.cam_set, "the rings, the player and the camera");
    host.tap(180.0, 320.0);
    host.run(&mut g, 20.0, 1.0 / 60.0);
    assert!(g.error.is_none(), "{:?}", g.error);
    let mut rounds = host.rounds();
    rounds.dedup();
    assert_eq!(rounds, [Round::Idle, Round::Playing, Round::Over]);
    assert!(g.dump().contains("\"mode\":\"over\""), "{}", g.dump());
    host.drawn()
}

#[test]
fn the_tunnel_plays_a_round_and_replays() {
    let a = plays_a_round("starters/tunnel");
    assert_eq!(a, plays_a_round("starters/tunnel"), "the same seed and input draw the same frames, 3D included");
    assert!(a.windows(2).any(|w| w[0] != w[1]), "frames change");
}

/// Engine 1.0's tunnel, its Lua 1.0's but for bitwise operators, plays on
/// the 3D build: the compatibility test for 1.0's 3D Lua.
#[test]
fn engine_1_0s_tunnel_runs_on_the_3d_build() {
    let a = plays_a_round("compat/tunnel-1.0");
    assert_eq!(a, plays_a_round("compat/tunnel-1.0"));
}

/// The autopilot (the debug menu's scenario global) threads rings: the
/// score counts them, through 1.0's `score` and 2.0's `round.score`.
#[test]
fn the_autopilot_scores() {
    for project in ["starters/tunnel", "compat/tunnel-1.0"] {
        let mut f = dir(project);
        let main = String::from_utf8(f["game/main.lua"].clone()).unwrap().replace("AUTO = AUTO or false", "AUTO = true").replace("AUTO = false", "AUTO = true");
        f.insert("game/main.lua".into(), main.into_bytes());
        compile(&mut f);
        let p = Rc::new(Project::parse(&f).unwrap());
        let mut v = Vm::new(&p, 9, Mode::Round, &[], &[], &[], (0.0, 0.0, 360.0, 640.0)).unwrap();
        step(&mut v, 60 * 8);
        let score = v.host().last_score;
        assert!(score >= 4.0 && v.host().over.is_none(), "{project}: {score}");
    }
}

// ------------------------------------------------------- 2f: the racer kit's starter

/// The racer starter's manifest and a round on a fake host keep the platform's rules.
#[test]
fn the_racer_keeps_the_platform_rules() {
    const MANIFEST: &str = "abi = 0\nid = \"dev.maimbrain.studio2.racer\"\nname = \"Swerve\"\nversion = \"0.1.0\"\ncreator = \"@maimbrain\"\norientation = \"portrait\"\nlogical_size = [360, 640]\ninputs = [\"touch\"]\nperf_tier = \"full\"\nstdlib = { mb3d = 2, mb2d = 3 }\nsensors = [\"haptics\"]\ncapabilities = [\"store\", \"score\"]\n[[scores]]\nboard = 0\nlabel = \"Score\"\norder = \"higher\"\n";
    use_project(dir("starters/racer"));
    Conformance::new().max_round(60.0).manifest(MANIFEST).check::<Studio2>();
}

/// The racer drives itself on the title card over its terrain, a tap
/// starts a round (and is its first lane change), a car left to itself
/// crashes into traffic, the wreck plays out and the card comes; the same
/// seed and input draw the same frames.
fn racer_round() -> Vec<u64> {
    let host = Host::start(5);
    use_project(dir("starters/racer"));
    let mut g: Studio2 = host.init();
    host.run(&mut g, 3.0, 1.0 / 60.0);
    assert!(g.error.is_none(), "{:?}", g.error);
    let w = g.title.as_ref().expect("the attract mode").host().three.as_ref().unwrap();
    assert!(w.cam_set, "the chase camera");
    host.tap(90.0, 400.0);
    host.run(&mut g, 30.0, 1.0 / 60.0);
    assert!(g.error.is_none(), "{:?}", g.error);
    let mut rounds = host.rounds();
    rounds.dedup();
    assert_eq!(rounds, [Round::Idle, Round::Playing, Round::Over]);
    assert!(g.dump().contains("\"card\":\"WRECKED!\""), "{}", g.dump());
    host.drawn()
}

#[test]
fn the_racer_plays_a_round_and_replays() {
    let a = racer_round();
    assert_eq!(a, racer_round(), "the same seed and input draw the same frames");
    assert!(a.windows(2).any(|w| w[0] != w[1]), "frames change");
}

/// The racer's autopilot (the Autopilot scenario's global) drives a minute
/// through traffic, trucks and roadworks; and nothing reaches a car left
/// alone before the warm-up's 3.5 s.
#[test]
fn the_racer_autopilot_drives_and_the_start_is_safe() {
    let mut f = dir("starters/racer");
    let mut alone = vm(&f).unwrap_or_else(|e| panic!("{e}"));
    step(&mut alone, 60 * 3 + 30);
    assert!(alone.host().over.is_none() && alone.host().last_score > 0.0, "the warm-up can't be failed");
    let main = String::from_utf8(f["game/main.lua"].clone()).unwrap().replace("AUTO = false", "AUTO = true");
    f.insert("game/main.lua".into(), main.into_bytes());
    compile(&mut f);
    let mut crashed = 0;
    for seed in [3, 9, 11, 21] {
        let p = Rc::new(Project::parse(&f).unwrap());
        let mut v = Vm::new(&p, seed, Mode::Round, &[], &[], &[], (0.0, 0.0, 360.0, 640.0)).unwrap();
        step(&mut v, 60 * 60);
        if v.host().over.is_some() {
            crashed += 1;
        } else {
            assert!(v.host().last_score > 2000.0, "seed {seed}: {}", v.host().last_score);
        }
    }
    assert!(crashed <= 1, "{crashed} of 4 one-minute autopilot runs crashed");
}

/// The racer's bot.lua reads the road through game/shared.lua and taps the
/// halves of the screen: it changes lanes, every round ends in traffic or
/// at the cap, and a good player outlasts a first-timer over the same
/// roads. (Natively, as `mb difficulty --runner native` plays it; here, not
/// bot_tests.rs, which is part of the engine's published fingerprint.)
#[test]
fn the_racers_bot_drives_it() {
    let f = dir("starters/racer");
    // A field of the round's `studio-bot` line.
    let field = |j: &str, k: &str| -> String {
        let i = j.find(&format!("\"{k}\":")).unwrap_or_else(|| panic!("no {k} in {j}")) + k.len() + 3;
        let rest = j[i..].trim_start_matches('"');
        rest[..rest.find(['"', ',', '}']).unwrap()].to_string()
    };
    let play = |preset: &str, seed: u64, max: f64| -> (f64, u64, String) {
        let (line, _) = crate::bot::native_round(&f, preset, seed, max, &[]).unwrap();
        assert!(!line.contains("\"error\""), "{line}");
        let (secs, presses): (f64, u64) = (field(&line, "seconds").parse().unwrap(), field(&line, "presses").parse().unwrap());
        assert!(secs < 15.0 || presses > 3, "{line}");
        assert!(field(&line, "capped") == "true" || ["car", "truck", "cones"].contains(&field(&line, "cause").as_str()), "{line}");
        (secs, presses, line)
    };
    let median = |preset: &str, max: f64| {
        let mut s: Vec<f64> = (1..=4).map(|seed| play(preset, seed, max).0).collect();
        s.sort_by(f64::total_cmp);
        s[2]
    };
    let (good, first) = (median("good", 90.0), median("first-timer", 45.0));
    assert!(good >= first && good > 30.0, "good {good} s, first-timer {first} s");
}

/// An instance's colour keeps all 32 bits (a Lua number is a double; a
/// float would round 0xffffffff to 0, drawing white things black).
#[test]
fn instance_colours_keep_every_bit() {
    let list = [1.0, 2.0, 3.0, 0.5, 2.0, 0xffffffffu32 as f64, 0.0, 0.0, 0.0, 0.0, 1.0, 0x9a9a9aff_u32 as f64, 7.0];
    let out = crate::api::three::instances(&list, 6);
    assert_eq!(out.len(), 2, "a row short of a stride is left out");
    assert_eq!((out[0].pos.x, out[0].scale.y, out[0].color), (1.0, 2.0, 0xffffffff));
    assert_eq!(out[1].color, 0xff9a9a9a, "0xRRGGBBAA, byte-swapped for the host");
}

/// Hiding, showing or giving a model's part a velocity of 0 sends mb3d
/// its visibility only, never a transform: the part keeps the pose the
/// model has for it (as 1.0's `show` did) until the game writes its
/// place, turn or size.
#[test]
fn a_models_part_keeps_its_pose_when_shown_or_hidden() {
    let mut v = vm(&three(
        r#"
        local fox = spawn(model("fox"))
        local hat = fox:find("Hat")
        hat:show(false)
        hat.visible = true
        hat.visible = false
        hat.vx, hat.gravity = 0, 0
        hat.data.worn = true
        function love.update() if round.time > 0.05 then hat.y = 1 end end
        "#,
    ))
    .unwrap();
    step(&mut v, 1);
    assert_eq!(v.host().three.as_ref().unwrap().flushed, 1, "the hat's visibility, no transform");
    step(&mut v, 3);
    assert_eq!(v.host().three.as_ref().unwrap().flushed, 1, "its y written: now its transform");
}

/// Comet Lanes, engine 1.0's 3D example moved up (examples/comet-lanes):
/// the platform's rules on the 3D build, and a round that plays and ends.
#[test]
fn comet_lanes_plays_on_the_3d_build() {
    const MANIFEST: &str = "abi = 0\nid = \"dev.maimbrain.studio2.cometlanes\"\nname = \"Comet Lanes\"\nversion = \"0.1.0\"\ncreator = \"@maimbrain\"\norientation = \"portrait\"\nlogical_size = [360, 640]\ninputs = [\"touch\"]\nperf_tier = \"full\"\nstdlib = { mb3d = 2, mb2d = 3 }\nsensors = [\"haptics\"]\ncapabilities = [\"store\", \"score\"]\n[[scores]]\nboard = 0\nlabel = \"Rings\"\norder = \"higher\"\n";
    use_project(dir("examples/comet-lanes"));
    Conformance::new().max_round(60.0).manifest(MANIFEST).check::<Studio2>();
    plays_a_round("examples/comet-lanes");
}

// ------------------------------------------------------------------ 2a

/// 2.0's 3D nodes (§5 [3d]): each constructor's table sets its
/// properties, they read back, `parent` keeps a node where it is, and
/// `Group`, `Emitter` and a billboard `Text` are 3D in a 3D game.
#[test]
fn three_js_nodes_take_their_properties() {
    let mut v = vm(&three(
        r#"
        local box = geometry.box(1, 2, 1):paint(0xff0000)
        local mat = material{ color = 0xb07a45, roughness = 0.8, metalness = 0.1, toon = 3, outline = 1 }
        crate = Mesh{ geometry = box, material = mat, x = 1, y = 0.5, name = "crate", tag = "loot" }
        lamp = Light{ parent = crate, color = 0xffc070, intensity = 3, range = 6, y = 2 }
        g = Group{ x = 5, ry = math.pi / 2 }
        sparks = Emitter{ preset = "sparks", rate = 10, parent = g, colors = { 0xffd060, 0xff4020 }, life = { 0.2, 0.5 } }
        label = Text{ text = "hi", billboard = true, size = 0.4, parent = crate }
        trail = Trail{ parent = g, color = 0x9fdcff, width = 0.2, length = 0.8 }
        hud = Text{ text = "score" }
        g2 = Group{ parent = Sprite{} }
        crate.fade = { 40, 60 }
        function love.update()
          lamp.parent = g
          out = { crate.name, crate.tag, crate.geometry == box, crate.material == mat, lamp.color, lamp.intensity, lamp.range,
            sparks.preset, sparks.rate, label.text, label.size, trail.width, trail.length, crate.fade[2], tostring(hud), tostring(g2), typeof(g) }
          local x, y, z = lamp:worldPosition()
          out[#out + 1] = string.format("%.2f %.2f %.2f", x, y, math.abs(z))
        end
        function love.draw() for _, s in ipairs(out) do love.graphics.print(tostring(s)) end end
        "#,
    ))
    .unwrap();
    step(&mut v, 2);
    let out = printed(&v);
    assert_eq!(out[..17], ["crate", "loot", "true", "true", "16760944", "3", "6", "sparks", "10", "hi", "0.4", "0.2", "0.8", "60", "Text", "Group", "Node3D"], "{out:?}");
    assert_eq!(out[17], "1.00 2.50 0.00", "reparented, the lamp stays where it was");
    let w = v.host().three.as_ref().unwrap();
    assert_eq!(w.scene.len(), 6, "crate, lamp, g, sparks, label, trail; the 2D group and text aren't 3D");
}

/// `scene.find` and `scene.all` see 3D nodes too (they saw only 2D ones,
/// so a 3D game's bot.lua couldn't find a body by its name): the 2D nodes
/// first, then the 3D ones, each as made.
#[test]
fn scene_find_and_all_see_3d_nodes() {
    let mut v = vm(&three(
        r#"
        hud = Text{ text = "score", tag = "ui", name = "hud" }
        a = Mesh{ geometry = geometry.box(1, 1, 1), name = "crate1", tag = "crate" }
        b = RigidBody3D{ shape = BoxShape3D{ w = 1, h = 1, d = 1 }, name = "crate2", tag = "crate", x = 3 }
        g = Group{ name = "lamps", tag = "ui" }
        function love.draw()
          local crates, ui = scene.all("crate"), scene.all("ui")
          love.graphics.print(string.format("%s %s %s %s %d %s %s %d %s %s %s", tostring(scene.find("crate2") == b), tostring(scene.find("crate1") == a),
            tostring(scene.find("hud") == hud), tostring(scene.find("nope")), #crates, tostring(crates[1] == a), tostring(crates[2] == b),
            #ui, tostring(ui[1] == hud), tostring(ui[2] == g), tostring(#scene.all("*") >= 4)))
        end
        "#,
    ))
    .unwrap();
    step(&mut v, 2);
    assert_eq!(printed(&v), ["true true true nil 2 true true 2 true true true"]);
}

/// 3D nodes come in the order they were made, though a new one takes an
/// old one's freed slot.
#[test]
fn scene_find_and_all_keep_3d_nodes_in_the_order_made() {
    let mut v = vm(&three(
        r#"
        a = Mesh{ geometry = geometry.box(1, 1, 1), name = "enemy", tag = "enemy" }
        b = Mesh{ geometry = geometry.box(1, 1, 1), name = "enemy", tag = "enemy" }
        a:destroy()
        c = Mesh{ geometry = geometry.box(1, 1, 1), name = "enemy", tag = "enemy" }
        function love.draw()
          local all = scene.all("enemy")
          love.graphics.print(string.format("%s %d %s %s", tostring(scene.find("enemy") == b), #all, tostring(all[1] == b), tostring(all[2] == c)))
        end
        "#,
    ))
    .unwrap();
    step(&mut v, 2);
    assert_eq!(printed(&v), ["true 2 true true"]);
}

/// Teaching errors for the new names, at their line.
#[test]
fn three_js_names_say_what_to_do() {
    assert!(error_of("Mesh{}\n").contains("Mesh{…} needs geometry = geometry.box(1, 1, 1)"));
    assert!(error_of("Model{ model = 'cat' }\n").contains("there's no models/cat.glb"));
    assert!(error_of("local l = Light{}\nl.colour = 1\n").contains("no property `colour` on Light; did you mean `color`?"));
    assert!(error_of("local m = Mesh{ geometry = geometry.box() }\nm.intensity = 2\n").contains("Mesh has no property `intensity`") || error_of("local m = Mesh{ geometry = geometry.box() }\nm.intensity = 2\n").contains("no property `intensity` on Mesh"));
    assert!(error_of("world.grade('sepia')\n").contains("the presets are moonlit, goldenHour, neon, off"));
    assert!(error_of("world.sky{ atmosphere = 'noon' }\n").contains("day, dusk, night"));
    assert!(error_of("material{ shader = 'x.wgsl' }\n").contains("there's no shaders/x.wgsl"));
    assert!(error_of("local m = Mesh{ geometry = geometry.box() }\nm:bone('hand_r')\n").contains("no method `bone` on Mesh"));
    assert!(error_of("camera.fov = 0.9\n").contains("degrees"));
    assert!(error_of("local f = Model{ model = 'fox' }\nf:play('Run', { layer = 'upper' })\n").contains("a new layer needs a mask"));
    assert!(error_of("AnimationTree{ model = Model{ model = 'fox' }, states = { a = { clip = 'Run' } }, transitions = { { from = 'a', to = 'b' } } }\n").contains("there's no state \"b\""));
    assert!(error_of("scatter(Group{}, { count = 1e6 })\n").contains("0 to 16384"));
    assert!(error_of("Mesh{ geometry = geometry.box(), spin = 1 }\n").contains("Mesh{…}: no property `spin` on Mesh"));
}

/// `material{}` takes three.js's keys or 1.0's, told apart by the table.
#[test]
fn material_tells_1_0s_keys_from_three_js() {
    let mut v = vm(&three(
        r#"
        a = material{ color = 0xff0000, roughness = 0.2 }
        b = material{ color = 0xff0000ff, rough = 0.2 }
        c = material{ color = 0xff0000ff }
        d = material{ color = { 1, 0, 0 }, emissive = 0xff8000, emissiveIntensity = 2, transparent = true, opacity = 0.5 }
        "#,
    ))
    .unwrap();
    step(&mut v, 1);
    assert_eq!(v.host().three.as_ref().unwrap().materials.len(), 4);
}

/// Models: clips with a layer on a mask, an AnimationTree (states,
/// transitions, a 1D blend space) and its state, rigs and IK calls (which
/// natively find no rig and say so in the log, never an error).
#[test]
fn models_trees_layers_and_rigs() {
    let mut v = vm(&three(
        r#"
        fox = Model{ model = "fox", ry = math.pi }
        tree = AnimationTree{ model = fox, start = "move",
          states = { move = { blend = { Idle = 0, Walk = 2, Run = 6 }, by = "speed" }, jump = { clip = "Jump", once = true, next = "move" } },
          transitions = { { from = "move", to = "jump", when = "airborne", fade = 0.1 } } }
        fox.plantFeet = true
        fox.gait = "mech"
        fox:headLook(0, 1, 5, { limit = 0.8 })
        fox:reach("hand_r", 1, 1, 1)
        fox:reach("hand_r", nil)
        fox:restyle{ toon = 3, outline = 1 }
        hand = fox:bone("hand_r")
        fox.flight = { flap = 2 }
        fox.swim = { length = 0.5 }
        states = {}
        function love.update()
          tree:set("speed", 3)
          tree:set("airborne", round.time > 0.2 and round.time < 0.3)
          states[#states + 1] = tree.state
        end
        function love.draw() love.graphics.print(table.concat({ tostring(states[2]), tostring(states[#states]), tostring(fox.plantFeet), tostring(tree:get("speed")), typeof(tree), tostring(fox.clip ~= nil) }, " ")) end
        "#,
    ))
    .unwrap();
    for _ in 0..30 {
        v.step(1.0 / 30.0, &[], &Held::default(), false, false).unwrap();
    }
    let out = printed(&v);
    assert_eq!(out, ["move jump true 3 AnimationTree true"], "{out:?}");
}

/// The 3D camera: degrees, roll, lookAt and project; follow runs after
/// everything moved and stops when the game writes the camera.
#[test]
fn the_3d_camera_follows_and_rolls() {
    let mut v = vm(&three(
        r#"
        ship = Group{ z = 0 }
        camera.fov = 60
        camera.roll = 0.1
        camera:follow(ship, { behind = 5, above = 2, lag = 0 })
        function love.update(dt)
          ship.z = ship.z - 10 * dt
          if round.time > 0.5 then camera.x = 3 end
          local sx, sy, on = camera:project(ship.x, ship.y, ship.z)
          out = string.format("%.1f %.2f %.1f %.1f %.1f %s", camera.fov, camera.roll, camera.x, camera.y, camera.z - ship.z, tostring(on))
        end
        function love.draw() love.graphics.print(out) end
        "#,
    ))
    .unwrap();
    step(&mut v, 12);
    // Locked behind (lag 0): 5 m behind its front (−z), 2 m up, as of the last frame.
    let out = printed(&v)[0].clone();
    assert!(out.starts_with("60.0 0.10 0.0 2.0 5.2"), "{out}");
    step(&mut v, 30);
    assert!(printed(&v)[0].starts_with("60.0 0.10 3.0"), "writing camera.x stopped the follow: {:?}", printed(&v));
}

/// faceDirection and lookAt turn a node's front (−z) along a direction or
/// at a point; scatter lays out the same copies for the same seed.
#[test]
fn facing_and_scatter() {
    let mut v = vm(&three(
        r#"
        a = Group{}
        a:faceDirection(1, 0)
        b = Group{ x = 0, y = 0, z = 0 }
        b:lookAt(0, 1, -1)
        grass = Mesh{ geometry = geometry.cone(0.1, 0.5, 4) }
        scatter(grass, { count = 500, area = 20, seed = 7, scale = { 0.5, 1.5 }, tint = { 0x406020, 0x80a040 } })
        trees = Mesh{ geometry = geometry.cylinder(0.2, 2):merge(geometry.cone(1, 2):paint(0x2f5a2a), 0, 2, 0) }
        trees:instance({ 0, 0, 0, 0, 1, 0xff0000,  4, 0, 0, 1, 2, 0x00ff00 }, 6)
        function love.draw() love.graphics.print(string.format("%.3f %.3f %.3f", a.ry, b.rx, math.abs(b.ry))) end
        "#,
    ))
    .unwrap();
    step(&mut v, 1);
    assert_eq!(printed(&v), ["-1.571 0.785 0.000"], "−z turned toward +x is a quarter turn right; up 45°");
    let w = v.host().three.as_ref().unwrap();
    let counts: Vec<usize> = w.info.values().map(|i| i.instances).filter(|n| *n > 0).collect();
    assert_eq!(counts, [500, 2]);
}

/// The world's looks take their presets and tables; splash, shockwave and
/// fx in 3D run; a 3D round replays: the same input, the same frames.
#[test]
fn the_world_and_a_replayed_3d_round() {
    let src = r#"
        world.sky{ atmosphere = "night", clouds = { cover = 0.3 }, disc = "moon", stars = 0.8 }
        world.sun{ dir = { -0.3, -1, 0.4 }, color = 0x9fb4ff, intensity = 0.6, shadows = true }
        world.lighting("hero")
        world.fog{ color = 0x1a2340, mode = "exp2", density = 0.03 }
        world.post{ exposure = 1.1, bloom = 0.4 }
        world.dof{ focus = 6, range = 2, blur = 3 }
        world.speedBlur{ amount = 0.2 }
        world.grade("moonlit", 0.8) world.shafts("moonbeams") world.halos("mist") world.film("subtle")
        world.water{ preset = "lake", height = 0 }
        world.retro("ps1") world.retro("off")
        fox = Model{ model = "fox" }
        fox:play("Run")
        fox:play("Wave", { layer = "upper", mask = "Spine", once = true })
        boat = Mesh{ geometry = geometry.lathe({ 0, 0, 0.5, 0.1, 0.6, 0.4 }, 10), wake = true }
        fish = Mesh{ geometry = geometry.fish{ length = 0.6 } }
        tube = Mesh{ geometry = geometry.loft{ points = { 0, 0, 0, 0, 1, 0, 1, 2, 0 }, radius = function(t) return 0.2 * (1 - t) end } }
        function love.update(dt)
          boat.vx = math.sin(round.time) * 2
          fox.ry = fox.ry + dt
          if love.mouse.isDown(1) then fx.burst("sparkles", 0, 1, 0, { count = 10 }) fx.popup("+1", 0, 2, 0) fx.shake(0.2, 0.2) splash(0, 0, 0, 0.5) shockwave(0, 0, 0, 3, 0.5, 0.4) end
        end
    "#;
    let run = || {
        let _host = Host::start(3);
        let mut v = vm(&three(src)).unwrap();
        let down = [LoveEvent::MousePressed { x: 180.0, y: 300.0, button: 1, istouch: true }];
        let mut flushed = Vec::new();
        for i in 0..90 {
            let mut held = Held::default();
            held.mouse[0] = i % 20 < 5;
            v.step(1.0 / 60.0, if i % 20 == 0 { &down } else { &[] }, &held, false, false).unwrap_or_else(|e| panic!("{e}"));
            flushed.push(v.host().three.as_ref().unwrap().flushed);
        }
        flushed
    };
    assert_eq!(run(), run());
}

/// A destroyed `Trail` fades, then its trail is freed in mb3d, which holds
/// 64 at most a session: a shot with a trail every frame, each destroyed
/// soon after, never runs out, and the frees come at the same frames
/// every run (they follow the round's clock).
#[test]
fn destroyed_trails_are_freed_once_faded() {
    let src = r#"
        shots, made = {}, 0
        function love.update(dt)
          local s = Group{ z = -made }
          Trail{ parent = s, color = 0xffc070, width = 0.1, length = 0.3 }
          made = made + 1
          shots[#shots + 1] = s
          if #shots > 3 then table.remove(shots, 1):destroy() end
        end
        function love.draw() love.graphics.print(tostring(made)) end
    "#;
    let run = || {
        let mut v = vm(&three(src)).unwrap();
        let mut fading = Vec::new();
        for _ in 0..200 {
            v.step(1.0 / 60.0, &[], &Held::default(), false, false).unwrap_or_else(|e| panic!("{e}"));
            fading.push(v.host().three.as_ref().unwrap().fading.len());
        }
        assert_eq!(printed(&v), ["200"], "200 trails made, 197 destroyed");
        fading
    };
    let fading = run();
    // A destroyed trail waits out its 0.3 s fade and a 0.1 s margin, about
    // 24 frames, so about two dozen wait at once, never all 197.
    assert_eq!(*fading.iter().max().unwrap(), 23, "{fading:?}");
    assert_eq!(fading[199], 23, "the ones still fading");
    assert_eq!(fading, run(), "freed at the same frames every run");
}

// ---------------------------------------------------------------- Terrain (2c)

/// `Terrain{}` from Lua: the true ground (heightAt, normalAt) as the SDK has
/// it, `raise` at once, `progress` to 1 and the `built` signal once.
#[test]
fn terrain_from_lua() {
    let mut v = vm(&three(
        r#"
        ground = Terrain{ noise = { seed = 7, ridged = 0.3 }, height = 80,
          stamps = { { "hill", 20, -10, radius = 18, height = 6 }, { "crater", 80, 40, radius = 30, depth = 12, rim = 3 },
                     { "river", { -60, 0, 0, 10, 60, 5 }, width = 6, bank = 20 } },
          paint = { { color = 0x3c5a2e }, { above = 60, color = 0xd8dee6 }, { slope = 0.2, color = 0x6b6256 } } }
        built, frames = 0, 0
        ground.built:connect(function() built = built + 1 end)
        before = ground:heightAt(5, 5)
        function love.update(dt)
          frames = frames + 1
          if frames == 30 then ground:raise(5, 5, 4, 2) end
        end
        function love.draw()
          local nx, ny, nz = ground:normalAt(5, 5)
          love.graphics.print(string.format("%d %.2f %.3f %.3f %s", built, ground.progress, ground:heightAt(5, 5) - before, nx * nx + ny * ny + nz * nz, tostring(ny > 0)))
        end
        "#,
    ))
    .unwrap();
    step(&mut v, 1);
    let first: Vec<String> = printed(&v);
    assert!(first[0].starts_with("0 0."), "building: {first:?}");
    step(&mut v, 20);
    assert_eq!(printed(&v), ["1 1.00 0.000 1.000 true"], "built once, the true ground");
    step(&mut v, 10);
    assert_eq!(printed(&v), ["1 1.00 2.000 1.000 true"], "raised 2 m at once");
    let w = v.host().three.as_ref().unwrap();
    let g = w.ground.as_ref().unwrap();
    assert!(g.terrain.counts().0 > 40, "its tiles are on the host");
    assert_eq!(w.cam.far, 500.0, "the camera's own far plane is past the ground's 400 m");
}

/// What a Terrain refuses, in a creator's words.
#[test]
fn terrain_says_what_to_do() {
    assert!(error_of("Terrain{ reach = 1600 }\n").contains("reach 1600 at detail 1 is about 390k triangles of the phone's 150k; use detail = 2 or reach = 800"));
    assert!(error_of("Terrain{ detail = 3 }\n").contains("detail is 0.5, 1, 2 or 4"));
    assert!(error_of("Terrain{ heightmap = \"valley\" }\n").contains("heightmap isn't in engine 2.0 yet"));
    assert!(error_of("Terrain{ hieght = 30 }\n").contains("did you mean `height`?"));
    assert!(error_of("Terrain{ stamps = { { \"mesa\", 0, 0 } } }\n").contains("\"mesa\" isn't a stamp"));
    assert!(error_of("Terrain{ stamps = { { \"hill\", 0, 0, radius = 5 } } }\n").contains("needs height"));
    assert!(error_of("Terrain{ paint = { { above = 3 } } }\n").contains("paint[1] needs a surface"));
    assert!(error_of("Terrain{ paint = { { surface = \"lava\" } } }\n").contains("\"lava\" isn't a ground surface: \"grass\", \"meadow\", \"dirt\", \"rock\" or \"snow\""));
    assert!(error_of("Terrain{ surfaces = { sand = \"x\" } }\n").contains("\"sand\" isn't a ground surface"));
    assert!(error_of("Terrain{ surfaces = { rock = \"basalt\" } }\n").contains("surfaces.rock: there's no images/basalt"));
    assert!(error_of("Terrain{ paint = { { color = 0x808080, slope = 2 } } }\n").contains("0.29 is 45°"));
    assert!(error_of("Terrain{}\nTerrain{}\n").contains("a round has one Terrain"));
    assert!(error_of("local g = Terrain{}\ng.x = 3\n").contains("a Terrain stays where it is"));
    assert!(error_of("local g = Terrain{}\ng:place(1, 2, 3)\n").contains("a Terrain stays where it is"));
    assert!(error_of("local g = Terrain{}\ng.progress = 1\n").contains("read-only"));
    assert!(error_of("scatter(Group{}, { on = Group{} })\n").contains("on is the round's Terrain"));
    // collision = true, and a big reach, only warn; destroyed, another may be made.
    let mut v = vm(&three("g = Terrain{ collision = true, reach = 600 }\ng:destroy()\nTerrain{ fog = false, size = 256 }\n")).unwrap();
    step(&mut v, 2);
}

/// `scatter(node, { on = ground })` stands every copy on the true ground and
/// fades them out before the coarsest level; `plantFeet` on a model over a
/// terrain does nothing natively (no rig) but doesn't fail.
#[test]
fn terrain_scatter_and_feet() {
    let mut v = vm(&three(
        r#"
        ground = Terrain{ height = 40 }
        grass = Mesh{ geometry = geometry.cone(0.1, 0.5, 4), x = 30, y = 100, z = -20 }
        scatter(grass, { count = 300, area = 40, seed = 3, on = ground })
        fox = Model{ model = "fox", plantFeet = true }
        function love.update() fox.y = ground:heightAt(fox.x, fox.z) end
        "#,
    ))
    .unwrap();
    step(&mut v, 5);
    let w = v.host().three.as_ref().unwrap();
    let grass = w.info.values().find(|i| i.instances == 300).expect("the copies");
    assert_eq!(grass.fade, Some((48.0, 64.0)), "gone before level 2 (64 m at detail 1)");
}

/// A terrain game on the title card holds the tap that starts a round until
/// its first view is built; the round takes that ground over built (the
/// `built` signal on its first frame); and the session replays.
#[test]
fn terrain_holds_the_title_and_the_round_takes_it_over() {
    let src = r#"
        ground = Terrain{ noise = { seed = 3 }, height = 50 }
        firstBuilt = nil
        ground.built:connect(function() firstBuilt = firstBuilt or round.time end)
        camera.y = ground:heightAt(0, 0) + 2
        function love.update(dt)
          camera.z = camera.z - 6 * dt
          camera.y = ground:heightAt(camera.x, camera.z) + 1.7
        end
        function love.draw() love.graphics.print(string.format("%s %.2f", tostring(firstBuilt ~= nil and firstBuilt < 0.05), ground.progress)) end
    "#;
    let run = || {
        let host = Host::start(9);
        use_project(three(src));
        let mut g: Studio2 = host.init();
        host.run(&mut g, 1.0 / 60.0, 1.0 / 60.0);
        host.tap(180.0, 320.0);
        host.run(&mut g, 2.0 / 60.0, 1.0 / 60.0);
        assert!(g.round.is_none(), "held while the attract mode builds");
        host.run(&mut g, 0.5, 1.0 / 60.0);
        assert!(g.error.is_none(), "{:?}", g.error);
        let r = g.round.as_ref().expect("the round started once built");
        assert_eq!(printed(r), ["true 1.00"], "the round's ground was built from its first frame (heard on the next)");
        let mut rounds = host.rounds();
        rounds.dedup();
        assert_eq!(rounds, [Round::Idle, Round::Playing]);
        host.run(&mut g, 5.0, 1.0 / 60.0);
        host.drawn()
    };
    let a = run();
    assert_eq!(a, run(), "the same session draws the same frames, tiles built across frames included");
}

/// The heights Lua reads are the same natively as in wasm: this sum is what
/// the same Lua printed in headless Chrome (V8) through `mb shot` when 2c
/// was built. A change here changes every terrain's land.
#[test]
fn terrain_heights_are_the_same_in_lua() {
    let mut v = vm(&three(
        r#"
        local ground = Terrain{ noise = { seed = 7, ridged = 0.4 }, height = 90,
          stamps = { { "hill", 120, -80, radius = 60, height = 18 }, { "crater", -200, 150, radius = 30, depth = 12, rim = 3 },
                     { "river", { -600, 0, 0, 40, 600, -20 }, width = 12, bank = 20 } } }
        local sum = 0
        for j = 0, 127 do
          for i = 0, 127 do
            sum = sum + ground:heightAt(-2000 + i * 31.4, -2000 + j * 31.4)
          end
        end
        local text = string.format("terrain sum %.6f", sum)
        function love.draw() love.graphics.print(text, 8, 8) end
        "#,
    ))
    .unwrap();
    step(&mut v, 1);
    assert_eq!(printed(&v), ["terrain sum 450055.478967"]);
}

/// The follow camera over a terrain stays above the ground, and the line
/// from what it follows to it stays clear of the ground, frame after frame,
/// as the node walks over hills (`clear_of`).
#[test]
fn terrain_keeps_the_follow_camera_out_of_the_ground() {
    let mut v = vm(&three(
        r#"
        ground = Terrain{ noise = { seed = 5 }, height = 90 }
        walker = Group{}
        camera:follow(walker, { behind = 6, above = 1.5, lag = 0.25 })
        function love.update(dt)
          walker.z = walker.z - 4 * dt
          walker.x = 20 * math.sin(walker.z * 0.02)
          walker.y = ground:heightAt(walker.x, walker.z)
          walker:faceDirection(0, -1)
        end
        "#,
    ))
    .unwrap();
    for f in 0..1200 {
        step(&mut v, 1);
        let w = v.host().three.as_ref().unwrap();
        let t = &w.ground.as_ref().unwrap().terrain;
        let c = w.cam.pos;
        let floor = t.height_at(c.x, c.z);
        assert!(c.y >= floor + crate::three::CAMERA_CLEARANCE - 1e-3, "frame {f}: the camera is {:.2} m over the ground", c.y - floor);
        let n = w.scene.get(w.scene.ids().into_iter().find(|id| w.kind(*id) == crate::three::Kind3::Group).unwrap()).unwrap();
        // From its middle and from its shins: the legs stay in view too.
        for up in [0.75, 0.3] {
            let from = maimbrain::gfx3d::vec3(n.x, n.y + up, n.z);
            for k in 1..=10 {
                let p = from + (c - from) * (k as f32 / 10.0);
                assert!(p.y >= t.height_at(p.x, p.z) - 0.35, "frame {f}: the ground is between the walker ({up} m up) and the camera");
            }
        }
    }
    // A hill behind the node: the camera rises over it; a wall too high to
    // rise over brings it in, in front of the hill.
    use maimbrain::gfx3d::terrain::{Stamp, Terrain, TerrainDesc};
    use maimbrain::gfx3d::vec3;
    let hill = |height: f32| Terrain::new(TerrainDesc { noise: None, stamps: vec![Stamp::Hill { x: 0.0, z: 8.0, radius: 6.0, height }], ..TerrainDesc::default() });
    let (from, to) = (vec3(0.0, 1.0, 0.0), vec3(0.0, 2.5, 6.0));
    let t = hill(3.0);
    let c = crate::three::clear_of(&t, from, from, to, 9.0);
    assert!(c.y > to.y && (c.x, c.z) == (to.x, to.z), "lifted over a 3 m hill: {c:?}");
    for k in 1..=16 {
        let p = from + (c - from) * (k as f32 / 16.0);
        assert!(p.y >= t.height_at(p.x, p.z) - 1e-3);
    }
    let c = crate::three::clear_of(&hill(40.0), from, from, to, 9.0);
    assert!(c.z < to.z && c.z > 0.0, "brought in before a 40 m wall: {c:?}");
    assert_eq!(crate::three::clear_of(&hill(0.0), from, from, to, 9.0), to, "nothing in the way: where it was");
}

/// Shader materials (docs/STUDIO-2-API.md [shaders]): `shader` names a
/// project's `shaders/<name>.wgsl`, its `uniforms` table is read again each
/// frame (a write, or a tween, changes the shader), its keys are the shader's
/// `struct Uniforms`, and mistakes say what there is.
#[test]
fn shader_materials_follow_their_uniforms_table() {
    let mut f = three(
        r#"
        burn = { t = 0, edge = 0xff6a10 }
        m = material{ shader = "dissolve", uniforms = burn, color = 0xb98a52 }
        flux.to(burn, 0.5, { t = 1 }):ease("linear")
        "#,
    );
    f.insert("shaders/dissolve.wgsl".into(), b"struct Uniforms { t: f32, edge: vec3<f32> }\nfn surface(in: Surface) -> SurfaceOut { var o: SurfaceOut; o.albedo = uniforms.edge * uniforms.t; o.normal = in.normal; return o; }\n".to_vec());
    f.insert("packed.toml".into(), b"[shaders.dissolve]\nkind = \"surface\"\ncost = 140\nuniforms = [[\"t\", 0, 1], [\"edge\", 16, 3]]\n".to_vec());
    let mut v = vm(&f).unwrap();
    step(&mut v, 1);
    let first = v.host().shaders.materials[0].params.uniforms;
    assert!(first[0] < 0.05, "{}", first[0]);
    assert!((first[4] - 1.0).abs() < 1e-6 && first[5] > 0.1 && first[5] < 0.2, "0xff6a10 in linear light: {:?}", &first[4..8]);
    step(&mut v, 15);
    let t = v.host().shaders.materials[0].params.uniforms[0];
    assert!(t > 0.4 && t < 0.6, "the tween moved t to about half: {t}");

    let mut f = three("m = material{ shader = \"glass\" }");
    f.insert("shaders/dissolve.wgsl".into(), b"fn surface(in: Surface) -> SurfaceOut { var o: SurfaceOut; return o; }\n".to_vec());
    let e = match vm(&f) {
        Err(e) => e,
        Ok(mut v) => v.step(1.0 / 60.0, &[], &Held::default(), false, false).expect_err("an error"),
    };
    assert!(e.contains("there's no shaders/glass.wgsl (there's dissolve)"), "{e}");
    let mut f = three("m = material{ shader = \"dissolve\", uniforms = { speed = 2 } }");
    f.insert("shaders/dissolve.wgsl".into(), b"struct Uniforms { t: f32 }\nfn surface(in: Surface) -> SurfaceOut { var o: SurfaceOut; return o; }\n".to_vec());
    f.insert("packed.toml".into(), b"[shaders.dissolve]\nkind = \"surface\"\nuniforms = [[\"t\", 0, 1]]\n".to_vec());
    let e = match vm(&f) {
        Err(e) => e,
        Ok(mut v) => v.step(1.0 / 60.0, &[], &Held::default(), false, false).expect_err("an error"),
    };
    assert!(e.contains("uniforms.speed: shaders/dissolve.wgsl's struct Uniforms has t"), "{e}");
}

/// Inline WGSL in `newShader` can't have a `struct Uniforms` (nothing lays it
/// out), and only `shaders/<name>.wgsl` is a project's shader (mb build
/// refuses a subfolder or `.WGSL`; the engine doesn't see one either).
#[test]
fn inline_shaders_have_no_named_uniforms_and_shaders_are_top_level() {
    let e = error_of("s = love.graphics.newShader(\"struct Uniforms { t: f32 }\\nfn sprite(in: Sprite) -> vec4<f32> { return in.color * uniforms.t; }\")\n");
    assert!(e.contains("a shader with struct Uniforms goes in shaders/<name>.wgsl"), "{e}");
    assert!(error_of("s = love.graphics.newShader(\"fn sprite(in: Sprite) -> vec4<f32> { return in.color; }\")\nerror('made')\n").contains("made"));
    let mut f = three("m = material{ shader = \"glow\" }");
    f.insert("shaders/fx/glow.wgsl".into(), b"fn surface(in: Surface) -> SurfaceOut { var o: SurfaceOut; return o; }\n".to_vec());
    f.insert("shaders/glow.WGSL".into(), b"fn surface(in: Surface) -> SurfaceOut { var o: SurfaceOut; return o; }\n".to_vec());
    let p = Project::parse(&f).unwrap();
    assert!(p.shaders.is_empty(), "{:?}", p.shaders.iter().map(|s| &s.name).collect::<Vec<_>>());
}

/// A sprite shader: love.graphics.newShader, setShader around images (as
/// draw-list commands, with the uniforms `send` set), and a fresh frame
/// starting without one.
#[test]
fn sprite_shaders_are_draw_list_commands() {
    let mut f = three(
        r#"
        img = love.graphics.newImage("gem")
        s = love.graphics.newShader("shimmer")
        function love.draw()
          love.graphics.setShader(s)
          s:send("strength", 0.25)
          love.graphics.draw(img, 10, 10)
          love.graphics.setShader()
          ok = love.graphics.getShader() == nil and s:hasUniform("strength") and not s:hasUniform("nope")
          love.graphics.print(tostring(ok))
        end
        "#,
    );
    f.insert("images/gem.png".into(), Vec::new());
    f.insert("shaders/shimmer.wgsl".into(), b"struct Uniforms { strength: f32 }\nfn sprite(in: Sprite) -> vec4<f32> { return in.color * uniforms.strength; }\n".to_vec());
    f.insert("packed.toml".into(), b"[images.gem]\nsize = [16, 16]\nfile = \"images/gem.png\"\n[shaders.shimmer]\nkind = \"sprite\"\nuniforms = [[\"strength\", 0, 1]]\n".to_vec());
    let mut v = vm(&f).unwrap();
    step(&mut v, 2);
    let shaders: Vec<f32> = v
        .host()
        .draw
        .cmds()
        .iter()
        .filter_map(|c| match c {
            Cmd::Shader { shader, params } => Some(if *shader == 0 { -1.0 } else { f32::from_le_bytes(params[16..20].try_into().unwrap()) }),
            _ => None,
        })
        .collect();
    assert_eq!(shaders, [0.25, 0.25, -1.0], "set (keeping last frame's send), sent again as it's in use, then none");
    assert_eq!(printed(&v).last().map(String::as_str), Some("true"));
}
