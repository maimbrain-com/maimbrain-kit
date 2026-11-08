//! The engine and its starters, natively: the platform's rules
//! (`Conformance`), the property sync (§4.3), the immediate layer, errors
//! with their lines, the budget, and replays.

use std::path::Path;
use std::rc::Rc;

use maimbrain::scene::{Cmd, Kind, SceneImage};
use maimbrain::sys::test::{Conformance, Host};

use crate::input::{Held, LoveEvent};
use crate::project::{Files, Project};
use crate::vm::{Mode, Vm};
use crate::{Studio2, use_project};

/// A project directory's files as `mb build` ships them: each `game/*.lua`
/// with its bytecode beside it.
pub(crate) fn read(root: &Path) -> Files {
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

pub(crate) fn compile(files: &mut Files) {
    let lua: Vec<(String, Vec<u8>)> = files.iter().filter(|(p, _)| p.starts_with("game/") && p.ends_with(".lua")).map(|(p, b)| (p.clone(), b.clone())).collect();
    for (p, src) in lua {
        let bc = luau::compile(&src).unwrap_or_else(|e| panic!("{p}:{}: {}", e.line, e.message));
        files.insert(format!("{p}c"), bc);
    }
}

pub(crate) fn starter(name: &str) -> Files {
    let mut f = read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("starters").join(name));
    // The library fonts the starters name, as mb build bakes them (natively a font
    // file that isn't there draws as the sans).
    let fonts = "[fonts.press-start-2p]\nfamily = \"Press Start 2P\"\nfile = \"fonts/press-start-2p.mbf\"\n[fonts.lilita-one]\nfamily = \"Lilita One\"\nfile = \"fonts/lilita-one.mbf\"\n[fonts.rye]\nfamily = \"Rye\"\nfile = \"fonts/rye.mbf\"\n[fonts.titan-one]\nfamily = \"Titan One\"\nfile = \"fonts/titan-one.mbf\"\n[fonts.orbitron]\nfamily = \"Orbitron\"\nfile = \"fonts/orbitron.mbf\"\n[fonts.exo-2]\nfamily = \"Exo 2\"\nfile = \"fonts/exo-2.mbf\"\n[fonts.fredoka]\nfamily = \"Fredoka\"\nfile = \"fonts/fredoka.mbf\"\n[fonts.luckiest-guy]\nfamily = \"Luckiest Guy\"\nfile = \"fonts/luckiest-guy.mbf\"\n";


    f.insert(crate::project::PACKED.into(), fonts.as_bytes().to_vec());
    f
}

/// A studio.toml without its `[identity]` table.
fn without_identity(toml: &str) -> String {
    let mut out = String::new();
    let mut skipping = false;
    for line in toml.lines() {
        if line.trim_start().starts_with('[') {
            skipping = line.trim() == "[identity]";
        }
        if !skipping {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// The catch starter with main.lua replaced (and other chunks added), and
/// without the starter's identity: the engine's defaults (greybox).
pub(crate) fn with_main(main: &str, more: &[(&str, &str)]) -> Files {
    let mut f = starter("catch");
    let toml = without_identity(&String::from_utf8(f["studio.toml"].clone()).unwrap());
    f.insert("studio.toml".into(), toml.into_bytes());

    f.insert("game/main.lua".into(), main.as_bytes().to_vec());
    for (p, s) in more {
        f.insert(p.to_string(), s.as_bytes().to_vec());
    }
    compile(&mut f);
    f
}

fn images(p: &Project) -> Vec<SceneImage> {
    p.images.iter().map(crate::scene_image).collect()
}

/// A round's VM for these files.
fn vm(files: &Files) -> Result<Vm, String> {
    let p = Rc::new(Project::parse(files).unwrap());
    let im = images(&p);
    Vm::new(&p, 7, Mode::Round, &im, &[], &[], (0.0, 0.0, 360.0, 640.0))
}

/// Runs a game's main.lua (its `love.update` once) and returns the error.
fn error_of(main: &str) -> String {
    match vm(&with_main(main, &[])) {
        Err(e) => e,
        Ok(mut v) => v.step(1.0 / 60.0, &[], &Held::default(), false, false).expect_err("an error"),
    }
}

/// The manifest `mb build` would write (for Conformance's daily-board check).
const MANIFEST: &str = "abi = 0\nid = \"dev.maimbrain.studio2.test\"\nname = \"Test\"\nversion = \"0.1.0\"\ncreator = \"@maimbrain\"\norientation = \"portrait\"\nlogical_size = [360, 640]\ninputs = [\"touch\"]\nperf_tier = \"lite\"\ncapabilities = [\"store\", \"score\"]\n[[scores]]\nboard = 0\nlabel = \"Score\"\norder = \"higher\"\n";

/// The platform's round rules (SPEC §5.1–§5.2) on a fake host: a Studio
/// game can't break them, because `Rounds` owns them.
#[test]
fn the_catch_starter_keeps_the_platform_rules() {
    use_project(starter("catch"));
    Conformance::new().max_round(60.0).manifest(MANIFEST).check::<Studio2>();
}

/// The title card plays the game by itself; a tap starts a round in a
/// fresh VM; a miss ends it with the game's heading; the world goes on
/// under the card.
#[test]
fn catch_plays_a_round() {
    let host = Host::start(3);
    use_project(starter("catch"));
    let mut g: Studio2 = host.init();
    host.run(&mut g, 3.0, 1.0 / 60.0);
    assert!(g.error.is_none(), "{:?}", g.error);
    let title = g.title.as_ref().expect("the attract mode");
    assert!(title.host().scene.all("item").len() + title.host().scene.all("bonus").len() > 0, "things fall on the title card");
    host.tap(180.0, 320.0);
    host.run(&mut g, 0.5, 1.0 / 60.0);
    assert!(g.title.is_none() && g.round.is_some());
    // Nobody steers: the first ball is missed.
    host.run(&mut g, 15.0, 1.0 / 60.0);
    assert!(g.error.is_none(), "{:?}", g.error);
    use maimbrain::sys::Round;
    let mut rounds = host.rounds();
    rounds.dedup(); // the title card is reported as the engine starts and again once the project has loaded
    assert_eq!(rounds, [Round::Idle, Round::Playing, Round::Over]);
    assert!(g.dump().contains("\"mode\":\"over\""), "{}", g.dump());
}

/// The same seed and input draw the same frames (replays).
#[test]
fn rounds_replay() {
    let run = || {
        let host = Host::start(11);
        use_project(starter("catch"));
        let mut g: Studio2 = host.init();
        host.run(&mut g, 1.0, 1.0 / 60.0);
        host.tap(100.0, 500.0);
        host.run(&mut g, 4.0, 1.0 / 60.0);
        host.drawn()
    };
    let a = run();
    assert_eq!(a, run());
    assert!(a.windows(2).any(|w| w[0] != w[1]), "frames change");
}

// ------------------------------------------------------- property sync

/// A node's properties write through to the scene and read back from it,
/// including what Rust moves (velocities).
#[test]
fn properties_write_through_and_read_back() {
    let mut v = vm(&with_main(
        r#"
        function love.load()
          hero = Sprite{ image = "player", x = 40, y = 50, tag = "hero", name = "Hero" }
          hero.vx = 60
          hat = Shape{ shape = "circle", radius = 8, parent = hero, y = -20, color = 0xff0000 }
          label = Text{ text = "hi", x = 10, y = 10, align = "center", fixed = true }
          hero.data.lives = 3
        end
        function love.update(dt)
          seen = { math.floor(hero.x + 0.5), hero.w, hero.frame, hat.parent == hero, #hero.children, hero.data.lives, hero.image, hat.color, scene.find("Hero") == hero }
        end
        function love.draw() for _, x in ipairs(seen) do love.graphics.print(tostring(x)) end end
        "#,
        &[],
    ))
    .unwrap();
    for _ in 0..60 {
        v.step(1.0 / 60.0, &[], &Held::default(), false, false).unwrap();
    }
    let h = v.host();
    let hero = h.scene.find("Hero").unwrap();
    let n = h.scene.get(hero).unwrap();
    assert_eq!((n.kind, n.y), (Kind::Sprite, 50.0));
    assert!((n.x - 100.0).abs() < 0.01, "moved by vx in Rust: {}", n.x);
    let hat = h.scene.get(hero).unwrap().children()[0];
    assert_eq!(h.scene.get(hat).unwrap().color, 0xff0000ff);
    // A constructor's x and y are under its parent (it rides on the hero).
    let (hx, hy) = h.scene.world_position(hat);
    assert!((hx - n.x).abs() < 0.01 && hy == 30.0, "{hx}, {hy}");
    let seen = printed(&v);
    assert_eq!(seen, ["99", "64", "1", "true", "1", "3", "player", "16711680", "true"], "the reads Lua saw, a frame before the last move");
}

/// `vr` spins a node in Rust, after `love.update` as `vx` moves it: the
/// angle stays in −π..π, a written `rotation` spins on from where it was
/// put, and the same frames give the same angles bit for bit.
#[test]
fn vr_spins_a_node_and_a_write_carries_on() {
    let main = r#"
        function love.load()
          coin = Sprite{ image = "player", x = 40, y = 50, vr = 6 }
          n = 0
        end
        function love.update(dt)
          n = n + 1
          if n == 40 then at40 = coin.rotation end
          if n == 45 then coin.rotation = 1 end
        end
        function love.draw() love.graphics.print(tostring(at40)) love.graphics.print(tostring(coin.rotation)) love.graphics.print(tostring(coin.vr)) end
        "#;
    let run = || {
        let mut v = vm(&with_main(main, &[])).unwrap();
        for _ in 0..60 {
            v.step(1.0 / 60.0, &[], &Held::default(), false, false).unwrap();
        }
        printed(&v)
    };
    let seen = run();
    let num = |i: usize| seen[i].parse::<f64>().unwrap();
    // 39 steps of 0.1 by the 40th update: 3.9, past π, so 3.9 − 2π.
    assert!((num(0) - (3.9 - std::f64::consts::TAU)).abs() < 1e-4, "{seen:?}");
    // Written 1 in the 45th update, then 15 steps by the 60th draw.
    assert!((num(1) - 2.5).abs() < 1e-4, "{seen:?}");
    assert_eq!(seen[2], "6");
    assert_eq!(seen, run(), "a replay spins the same");
}

/// `round.title` is true in the title card's VM from the start (its
/// main.lua and love.load), as [rounds] says, and false in a round's.
#[test]
fn round_title_holds_from_main_lua_on() {
    let main = r#"
        local seen = { tostring(round.title) }
        function love.load() seen[2] = tostring(round.title) end
        function love.draw() love.graphics.print(seen[1] .. " " .. seen[2]) end
    "#;
    let f = with_main(main, &[]);
    let p = Rc::new(Project::parse(&f).unwrap());
    let im = images(&p);
    for (mode, want) in [(Mode::Title, "true true"), (Mode::Round, "false false")] {
        let mut v = Vm::new(&p, 3, mode, &im, &[], &[], (0.0, 0.0, 360.0, 640.0)).unwrap();
        v.step(1.0 / 60.0, &[], &Held::default(), mode == Mode::Title, false).unwrap();
        assert_eq!(printed(&v), [want], "{mode:?}");
    }
}

/// `mb play --debug`'s picks (`studio_debug`), by label, before the first
/// round: a scenario's score and Invincible reach the round.
#[test]
fn headless_debug_picks_reach_the_round() {
    use maimbrain::sys::test::set_param;
    let mut f = with_main("function love.update() end", &[]);
    f.insert("debug/scenarios.toml".into(), b"[[scenario]]\nname = \"Rich\"\nscore = 38\n".to_vec());
    let host = Host::start(9);
    set_param("studio_debug", "Rich;Invincible=1");
    use_project(f);
    let mut g: Studio2 = host.init();
    host.run(&mut g, 0.5, 1.0 / 60.0);
    host.tap(180.0, 320.0);
    host.run(&mut g, 2.0, 1.0 / 60.0);
    assert!(g.error.is_none(), "{:?}", g.error);
    let d = g.dump();
    assert!(d.contains("\"score\":38"), "{d}");
}

/// Past 256 random generators a round, the error says how to need fewer.
#[test]
fn too_many_generators_says_to_reseed_one() {
    let e = error_of("for i = 1, 257 do love.math.newRandomGenerator(i) end");
    assert!(e.contains("more than 256 in a round") && e.contains("rng:setSeed"), "{e}");
}

/// `data = { … }` in a constructor or `spawn`'s props is the node's data
/// (it was the engine's own table of every node's data).
#[test]
fn data_in_props_is_the_nodes() {
    let main = r#"
        local s = Sprite{ image = "player", data = { hp = 3 } }
        kind("rock", { w = 8, h = 8 })
        local r = spawn("rock", { x = 10, data = { mid = 42 } })
        function love.draw() love.graphics.print(tostring(s.data.hp) .. " " .. tostring(r.data.mid)) end
    "#;
    let v = play_frames(&with_main(main, &[]), 1, &none);
    assert_eq!(printed(&v), ["3 42"]);
}

/// What the game's love.draw printed last frame (how these tests read Lua).
fn printed(v: &Vm) -> Vec<String> {
    v.host().draw.texts().map(str::to_string).collect()
}

/// Bad values and unknown names are errors at the line that wrote them,
/// and say what to do.
#[test]
fn writes_are_checked_at_their_line() {
    let e = error_of("function love.load()\n  local s = Sprite{ image = \"player\" }\n  s.alpha = 30\nend\n");
    assert_eq!(e, "game/main.lua:3: node.alpha takes 0–1, got 30. Use 0.3 for 30%.");
    let e = error_of("local s = Shape{}\ns.colour = 0xff0000\n");
    assert_eq!(e, "game/main.lua:2: no property `colour` on Shape; did you mean `color`?");
    let e = error_of("local s = Sprite{ image = \"hero\" }\n");
    assert!(e.starts_with("game/main.lua:1: Sprite{…}: no image \"hero\": add images/hero.png"), "{e}");
    let e = error_of("local t = Text{ text = 'x' }\nt.image = 'player'\n");
    assert!(e.contains("Text has no property `image`"), "{e}");
    let e = error_of("local s = Sprite{ image = 'player' }\ns.frame = 3\n");
    assert!(e.contains("frame takes 1 to 2 (frames count from 1), got 3"), "{e}");
    let e = error_of("local s = Shape{}\ns.score = 1\n");
    assert!(e.contains("node.data"), "{e}");
    let e = error_of("camera:shake(4)\n");
    assert!(e.contains("fx.shake"), "{e}");
    let e = error_of("function love.update() love.graphics.rectangle('filled', 0, 0, 1, 1) end\n");
    assert!(e.starts_with("game/main.lua:1: bad argument #1 to 'rectangle' (invalid draw mode \"filled\""), "{e}");
    let e = error_of("function love.update() return nil + 1 end\n");
    assert!(e.starts_with("game/main.lua:1:"), "{e}");
}

/// One userdata per node: `==` and table keys work; a destroyed node reads
/// as it was, says it's gone, and refuses writes.
#[test]
fn nodes_keep_their_identity_and_their_last_values() {
    let mut v = vm(&with_main(
        r#"
        function love.load()
          local s = Sprite{ image = "item", x = 70 }
          local keyed = { [s] = "found" }
          s.tag = "ball"
          out = { keyed[scene.all("ball")[1]] }
          s:destroy()
          out[2], out[3] = tostring(s.alive), tostring(s.x)
          ok, msg = pcall(function() s.x = 1 end)
          out[#out + 1] = msg
        end
        function love.draw() for _, x in ipairs(out) do love.graphics.print(x) end end
        "#,
        &[],
    ))
    .unwrap();
    v.step(1.0 / 60.0, &[], &Held::default(), false, false).unwrap();
    let out = printed(&v);
    assert_eq!(out[..3], ["found", "false", "70"]);
    assert!(out[3].contains("this node was destroyed"), "{out:?}");
}

/// love.draw is recorded (Lua never runs in mb_render), in the canvas's
/// coordinates, with LÖVE's colour state.
#[test]
fn love_draw_is_recorded() {
    let mut v = vm(&with_main(
        r#"
        function love.draw()
          love.graphics.setColor(1, 0, 0)
          love.graphics.rectangle("fill", 10, 20, 30, 40, 4)
          love.graphics.setColor(0x00ff00)
          love.graphics.circle("line", 5, 6, 7)
          love.graphics.print("SCORE", 1, 2)
        end
        "#,
        &[],
    ))
    .unwrap();
    v.step(1.0 / 60.0, &[], &Held::default(), false, false).unwrap();
    let cmds = v.host().draw.cmds().to_vec();
    assert_eq!(cmds[0], Cmd::Rect { x: 10.0, y: 20.0, w: 30.0, h: 40.0, r: 4.0, line: 0.0, color: 0xff0000ff });
    assert_eq!(cmds[1], Cmd::Circle { x: 5.0, y: 6.0, r: 7.0, line: 1.0, color: 0x00ff00ff });
    assert!(matches!(&cmds[2], Cmd::Text { s, x: 1.0, y: 2.0, .. } if s == "SCORE"));
    // A new frame starts a new list.
    v.step(1.0 / 60.0, &[], &Held::default(), false, false).unwrap();
    assert_eq!(v.host().draw.len(), 3);
}

/// Images, quads, fonts, text objects and sprite batches as LÖVE has
/// them, and the filter pixel art needs.
#[test]
fn images_quads_fonts_and_batches() {
    let mut v = vm(&with_main(
        r#"
        love.graphics.setDefaultFilter("nearest", "nearest")
        local img = love.graphics.newImage("images/player.png")
        local ball = love.graphics.newImage("item")
        ball:setFilter("linear", "linear")
        local q = love.graphics.newQuad(64, 0, 64, 40, img)
        local batch = love.graphics.newSpriteBatch(ball, 10)
        batch:add(1, 2)
        batch:add(q, 3, 4)
        local label = love.graphics.newText(love.graphics.newFont(20), "hi")
        function love.draw()
          love.graphics.draw(img, q, 10, 20, 0, 2, 2, 32, 20)
          love.graphics.draw(img, 5, 6)
          love.graphics.draw(batch, 100, 0)
          love.graphics.draw(label, 7, 8)
          love.graphics.setFont(love.graphics.newFont("pixel", 16))
          love.graphics.print(img:getWidth() .. "x" .. img:getHeight() .. " " .. tostring(img:typeOf("Drawable")) .. " " .. batch:getCount())
          local _, _, w = q:getViewport()
          love.graphics.print(w .. " " .. select(1, img:getFilter()) .. " " .. ball:getFilter())
        end
        "#,
        &[],
    ))
    .unwrap();
    v.step(1.0 / 60.0, &[], &Held::default(), false, false).unwrap();
    let h = v.host();
    let player = h.project.image("player").unwrap() as u16;
    let item = h.project.image("item").unwrap() as u16;
    let cmds = h.draw.cmds();
    assert_eq!(cmds[0], Cmd::Image { image: player, frame: 0, quad: Some([64.0, 0.0, 64.0, 40.0]), x: 10.0, y: 20.0, r: 0.0, sx: 2.0, sy: 2.0, ox: 32.0, oy: 20.0, color: 0xffffffff });
    assert!(matches!(cmds[1], Cmd::Image { quad: None, x: 5.0, y: 6.0, .. }));
    assert_eq!(cmds[2], Cmd::Push);
    assert!(matches!(cmds[7], Cmd::Image { image, quad: None, x: 1.0, y: 2.0, .. } if image == item));
    assert!(matches!(cmds[8], Cmd::Image { quad: Some(_), x: 3.0, y: 4.0, .. }));
    assert_eq!(cmds[9], Cmd::Pop);
    assert!(cmds.iter().any(|c| matches!(c, Cmd::Text { s, size: 20.0, .. } if s == "hi")));
    assert_eq!(printed(&v)[1..], ["128x40 true 2", "64 nearest linear"]);
    let h = v.host();
    assert!(h.nearest[player as usize] && !h.nearest[item as usize]);
    // An unknown image or font teaches.
    assert!(error_of("love.graphics.newImage('hero')").contains("no image \"hero\": add images/hero.png"));
    assert!(error_of("love.graphics.newFont('Comic Neue Sans', 20)").contains("no font \"Comic Neue Sans\""));
}

/// Sources play the project's sounds by name, with LÖVE's state; music
/// is asked of the engine; a controller is LÖVE's Joystick.
#[test]
fn sources_music_and_joysticks() {
    let mut f = with_main(
        r#"
        local jump = love.audio.newSource("jump", "static")
        local copy
        function love.update()
          if not copy then
            jump:setVolume(0.5)
            jump:setPitch(1.5)
            jump:play()
            copy = jump:clone()
            music.play("theme")
          end
        end
        function love.draw()
          local pad = love.joystick.getJoysticks()[1]
          love.graphics.print(tostring(jump:isPlaying()) .. " " .. tostring(copy:isPlaying()) .. " " .. jump:getVolume() .. " " .. jump:getDuration())
          love.graphics.print(tostring(pad and pad:isGamepadDown("a", "b")) .. " " .. input.ax .. " " .. string.format("%.2f", love.math.noise(0.5)))
        end
        "#,
        &[],
    );
    f.insert("sounds/jump.sfx.toml".into(), b"[[note]]
".to_vec());
    f.insert("music/theme.ogg".into(), b"OggS".to_vec());
    f.insert("packed.toml".into(), b"[sounds.jump]
secs = 0.25
".to_vec());
    let mut v = vm(&f).unwrap();
    let mut held = Held::default();
    let mut m = crate::input::Mapper::default();
    m.feed(&maimbrain::input::Event { kind: maimbrain::input::Kind::PadButton as u8, code: maimbrain::input::pad::A, a: 1.0, ..Default::default() }, (0.0, 0.0, 1.0));
    m.feed(&maimbrain::input::Event { kind: maimbrain::input::Kind::KeyDown as u8, code: maimbrain::input::key::RIGHT, ..Default::default() }, (0.0, 0.0, 1.0));
    held.clone_from(&m.held);
    v.step(1.0 / 60.0, &[], &held, false, false).unwrap();
    assert_eq!(printed(&v), ["true false 0.5 0.25", "true 1 0.38"]);
    assert_eq!(v.host().music, Some(crate::api::audio::Music::Play { name: "theme".into(), fade: 0.5, volume: 0.7 }));
    // At 1.5× a quarter-second sound is over in a sixth of a second.
    for _ in 0..12 {
        v.step(1.0 / 60.0, &[], &held, false, false).unwrap();
    }
    assert!(printed(&v)[0].starts_with("false"), "{:?}", printed(&v));
    assert!(error_of("love.audio.newSource('boom', 'static')").contains("no sound \"boom\": add sounds/boom.ogg"));
}

/// LÖVE's callbacks get the frame's input, with a finger as the mouse.
#[test]
fn input_reaches_loves_callbacks() {
    let mut v = vm(&with_main(
        r#"
        log = {}
        function love.keypressed(k) log[#log + 1] = "key " .. k end
        function love.mousepressed(x, y, b, touch) log[#log + 1] = "mouse " .. x .. " " .. y .. " " .. b .. " " .. tostring(touch) end
        function love.touchpressed(id, x, y) log[#log + 1] = "touch " .. id end
        function love.draw()
          for _, l in ipairs(log) do love.graphics.print(l) end
          love.graphics.print(tostring(love.keyboard.isDown("space")) .. " " .. tostring(love.mouse.isDown(1)))
        end
        "#,
        &[],
    ))
    .unwrap();
    let mut held = Held::default();
    held.mouse[0] = true;
    let ev = [LoveEvent::TouchPressed { id: 2, x: 5.0, y: 6.0, pressure: 1.0 }, LoveEvent::MousePressed { x: 5.0, y: 6.0, button: 1, istouch: true }, LoveEvent::KeyPressed("space")];
    v.step(1.0 / 60.0, &ev, &held, false, false).unwrap();
    assert_eq!(printed(&v), ["touch 2", "mouse 5 6 1 true", "key space", "false true"]);
}

/// Globals stay live across chunks and frames: Luau's load-time imports
/// would otherwise freeze `round.score`, `camera.x` or a table one file
/// changes and another reads (vm.rs `load_game_chunk`).
#[test]
fn globals_read_what_they_are_now() {
    let files = with_main(
        r#"
        Game = { state = "title" }
        local ui = require("ui")
        function love.update(dt)
          round.score = round.score + 1
          camera.x = camera.x + 1
          Game.state = "play"
        end
        function love.draw()
          love.graphics.print(round.score .. " " .. camera.x .. " " .. ui.state())
        end
        "#,
        &[("game/ui.lua", "return { state = function() return Game.state end }
")],
    );
    let mut v = vm(&files).unwrap();
    v.step(1.0 / 60.0, &[], &Held::default(), false, false).unwrap();
    v.step(1.0 / 60.0, &[], &Held::default(), false, false).unwrap();
    assert_eq!(printed(&v), ["2 182 play"]);
}

/// `require` loads another chunk once; its errors name its own file.
#[test]
fn require_loads_game_modules() {
    let files = with_main(
        "local enemies = require(\"enemies\")\nlocal again = require(\"enemies\")\nfunction love.draw() love.graphics.print(tostring(enemies == again) .. \" \" .. enemies.n) end\n",
        &[("game/enemies.lua", "local M = { n = 3 }\nreturn M\n")],
    );
    let mut v = vm(&files).unwrap();
    v.step(1.0 / 60.0, &[], &Held::default(), false, false).unwrap();
    assert_eq!(printed(&v), ["true 3"]);
    let files = with_main("require(\"broken\")\n", &[("game/broken.lua", "\nlocal x = nil\nreturn x.y\n")]);
    assert!(vm(&files).err().unwrap().starts_with("game/broken.lua:3:"));
    assert!(error_of("require(\"missing\")\n").contains("there's no game/missing.lua"));
}

/// A loop that never ends trips the budget at its own line; the frame's
/// error stops the round, not the app.
#[test]
fn a_runaway_loop_trips_the_budget() {
    let e = error_of("function love.update()\n  local n = 0\n  while true do n = n + 1 end\nend\n");
    assert_eq!(e, "game/main.lua:3: sandbox: instruction budget exhausted (a loop that never ends?)");
}

/// The round's random numbers are the platform's seed's: the same in a
/// replay, different in another round; math.random is the same generator.
#[test]
fn random_numbers_follow_the_seed() {
    let main = "function love.draw() love.graphics.print(love.math.random(1, 1000) .. ' ' .. math.random(1000) .. ' ' .. love.math.random()) end\n";
    let files = with_main(main, &[]);
    let p = Rc::new(Project::parse(&files).unwrap());
    let draw = |seed| {
        let mut v = Vm::new(&p, seed, Mode::Round, &images(&p), &[], &[], (0.0, 0.0, 360.0, 640.0)).unwrap();
        v.step(1.0 / 60.0, &[], &Held::default(), false, false).unwrap();
        printed(&v)
    };
    assert_eq!(draw(1), draw(1));
    assert_ne!(draw(1), draw(2));
}

/// The sandbox: no way out, and no weak tables.
#[test]
fn the_sandbox_holds() {
    // os is the round's: its clock, and exit ends the round; no files or processes.
    assert!(error_of("os.execute('ls')").contains("attempt to call a nil value"));
    assert!(error_of("os.time({ year = 2020 })").contains("no calendar in the sandbox"));
    assert!(error_of("string.foo = 1").contains("readonly"));
    assert!(error_of("setmetatable({}, { __mode = 'k' })").contains("weak tables and __gc are off"));
    let e = error_of("return require(\"x\") or love.filesystem.read(\"a\")");
    assert!(e.contains("there's no game/x.lua"), "{e}");
}

/// Project errors name the file.
#[test]
fn project_errors_name_the_file() {
    let mut f = starter("catch");
    f.insert("studio.toml".into(), b"title = \"X\"\nengine = \"1.0\"\n".to_vec());
    assert!(Project::parse(&f).unwrap_err().contains("1.0 projects play on engine 1.0"));
    let mut f = starter("catch");
    f.remove("game/main.luac");
    assert!(Project::parse(&f).unwrap_err().contains("hasn't been compiled"));
    let mut f = starter("catch");
    f.insert("images/player.toml".into(), b"frame = [200, 40]\n".to_vec());
    assert!(Project::parse(&f).unwrap_err().contains("bigger than the 128×40 image"));
}

// ------------------------------------------------- the libraries (§5 [libraries])

/// Steps a round's VM `n` frames of `dt` and returns what love.draw printed last.
fn run_frames(main: &str, more: &[(&str, &str)], n: usize, dt: f32) -> Vec<String> {
    let mut v = vm(&with_main(main, more)).unwrap_or_else(|e| panic!("{e}"));
    for _ in 0..n {
        v.step(dt, &[], &Held::default(), false, false).unwrap_or_else(|e| panic!("{e}"));
    }
    printed(&v)
}

/// flux's README, run: linear moves, delays, oncomplete once, after, a
/// later tween taking a variable over, stop, and onstart/onupdate.
#[test]
fn flux_behaves_as_flux() {
    let main = r#"
        local a, b, c, d = { x = 0, y = 0 }, { x = 0 }, { x = 0 }, { x = 0 }
        local done, started, updates = 0, 0, 0
        flux.to(a, 4, { x = 10, y = 20 }):ease("linear")
        flux.to(b, 2, { x = 10 }):ease("linear"):delay(1)
        flux.to(c, 1, { x = 1 }):ease("linear"):oncomplete(function() done = done + 1 end)
          :onstart(function() started = started + 1 end):onupdate(function() updates = updates + 1 end)
          :after(1, { x = 0 }):ease("linear")
        local e = { x = 0 }
        flux.to(e, 2, { x = 100 }):ease("linear")
        function love.update() if round.time >= 0.49 and round.time < 0.51 then flux.to(e, 1, { x = 0 }):ease("linear") end end
        local s = flux.to(d, 1, { x = 10 }):ease("linear")
        function love.draw()
          if round.time >= 0.74 then s:stop() end
          love.graphics.print(a.x .. " " .. a.y .. " " .. b.x .. " " .. c.x .. " " .. done .. " " .. started .. " " .. updates .. " " .. e.x .. " " .. d.x)
          flux.update(1) -- does nothing: the engine updates flux
        end
    "#;
    // 2 s in quarter seconds: a halfway; b a second into its two after its
    // delay; c out and back; e taken over at 0.5 (from 25) and done by 1.5 s;
    // d stopped at 0.75 s (7.5). Then 1.25 s.
    assert_eq!(run_frames(main, &[], 8, 0.25), ["5 10 5 0 1 1 4 0 7.5"]);
    assert_eq!(run_frames(main, &[], 5, 0.25), ["3.125 6.25 1.25 0.75 1 1 4 6.25 7.5"]);
}

/// Easing names: flux's and easings.net's, flux's quadout by default; and
/// teaching errors.
#[test]
fn flux_eases_and_teaches() {
    let main = r#"
        local a, b, c = { x = 0 }, { x = 0 }, { x = 0 }
        flux.to(a, 1, { x = 1 })
        flux.to(b, 1, { x = 1 }):ease("backout")
        flux.to(c, 1, { x = 1 }):ease("outBack")
        function love.draw() love.graphics.print(string.format("%.4f %.4f %.4f", a.x, b.x, c.x)) end
    "#;
    // quadout at 0.5: 0.75; flux's backout at 0.5: 1 - 0.25 * (2.7 * 0.5 - 1.7).
    assert_eq!(run_frames(main, &[], 2, 0.25), ["0.7500 1.0875 1.0875"]);
    assert!(error_of("flux.to({ x = 0 }, 1, { x = 1 }):ease('wiggle')").contains("bad easing type 'wiggle'"));
    assert!(error_of("flux.to({ x = 'a' }, 1, { x = 1 })").contains("bad value for key 'x'"));
    assert!(error_of("flux.to(Sprite{ image = 'item' }, 1, { text = 1 })").contains("has no number `text`"));
}

/// Tweens move nodes (colours channel by channel), a write beats the
/// tween, and :yoyo() goes there and back for ever.
#[test]
fn flux_moves_nodes_and_writes_win() {
    let main = r#"
        local s = Shape{ x = 0, color = 0x000000 }
        local t = Shape{ x = 0 }
        local y = Shape{ y = 0 }
        flux.to(s, 1, { x = 100, color = 0xff8000 }):ease("linear")
        flux.to(t, 1, { x = 100 }):ease("linear")
        flux.to(y, 1, { y = 10 }):ease("linear"):yoyo()
        function love.update() if round.time >= 0.49 and round.time < 0.51 then t.x = -5 end end
        function love.draw() love.graphics.print(s.x .. " " .. string.format("%06x", s.color) .. " " .. t.x .. " " .. y.y) end
    "#;
    assert_eq!(run_frames(main, &[], 2, 0.25), ["50 804000 -5 5"]);
    assert_eq!(run_frames(main, &[], 6, 0.25), ["100 ff8000 -5 5"], "y is on its way back");
}

/// anim8's README: a grid's frames, durations by frame, onLoop, flips, draw.
#[test]
fn anim8_behaves_as_anim8() {
    let main = r#"
        local img = love.graphics.newImage("player")
        local g = anim8.newGrid(32, 20, 128, 40)
        local walk = anim8.newAnimation(g('1-4', 1, '1-2', 2), 0.1)
        local loops = 0
        local jump = anim8.newAnimation(g(1, 1, 2, 1, 3, 1), { ['1-2'] = 0.1, [3] = 0.3 }, function(a, n) loops = loops + n end)
        local once = anim8.newAnimation(g('1-2', 1), 0.1, 'pauseAtEnd')
        walk:flipH()
        function love.draw()
          walk:draw(img, 10, 20)
          love.graphics.print(#walk.frames .. " " .. walk.position .. " " .. jump.position .. " " .. loops .. " " .. once.position .. " " .. once.status .. " " .. tostring(walk.flippedH))
          walk:update(1) -- does nothing: the engine updates animations
        end
    "#;
    let mut v = vm(&with_main(main, &[])).unwrap();
    for _ in 0..5 {
        v.step(0.1, &[], &Held::default(), false, false).unwrap();
    }
    // 0.5 s: walk is on frame 6 of 6 (two rows), jump looped once (0.5 = 0.5 + 0), once paused at its end.
    assert_eq!(printed(&v), ["6 6 1 1 2 paused true"]);
    match &v.host().draw.cmds()[0] {
        Cmd::Image { quad, sx, ox, .. } => assert_eq!((*quad, *sx, *ox), (Some([32.0, 20.0, 32.0, 20.0]), -1.0, 32.0)),
        c => panic!("{c:?}"),
    }
    assert!(error_of("local g = anim8.newGrid(32, 20, 128, 40) g('1-5', 1)").contains("There is no frame for x=5, y=1"));
    assert!(error_of("local g = anim8.newGrid(32, 20, 128, 40) anim8.newAnimation(g('1-3', 1), { [1] = 0.1 })").contains("The durations table has length of 1, but it should be >= 3"));
}

/// hump.timer's README: after, every (with a count, and stopped by
/// returning false), during, cancel, script with wait, and tween.
#[test]
fn hump_timer_behaves_as_hump() {
    let main = r#"
        local log = {}
        local function add(s) log[#log + 1] = s end
        timer.after(0.5, function() add("after") end)
        timer.every(0.25, function() add("every") end, 3)
        local n = 0
        timer.every(0.25, function() n = n + 1 add("stop" .. n) if n == 2 then return false end end)
        local dur = 0
        timer.during(0.5, function(dt) dur = dur + dt end, function() add("during") end)
        local h = timer.after(0.25, function() add("cancelled") end)
        timer.cancel(h)
        timer.script(function(wait) add("s1") wait(0.5) add("s2") end)
        local t = { x = 0 }
        timer.tween(1, t, { x = 10 }, "linear")
        function love.draw() love.graphics.print(table.concat(log, ",") .. " " .. dur .. " " .. t.x) end
    "#;
    assert_eq!(run_frames(main, &[], 4, 0.25), ["s1,every,stop1,after,every,stop2,during,s2,every 0.5 10"]);
}

/// bump's README: moving into a wall slides along it, filters choose the
/// response, queries find items.
#[test]
fn bump_behaves_as_bump() {
    let main = r#"
        local world = bump.newWorld(50)
        local player, wall, coin = { name = "player" }, { name = "wall" }, { name = "coin" }
        world:add(player, 0, 0, 10, 10)
        world:add(wall, 14, 0, 10, 10)
        world:add(coin, 0, 30, 5, 5)
        local x, y, cols, len = world:move(player, 10, 10)
        local out = { x, y, len, cols[1].other.name, cols[1].normal.x, cols[1].type }
        local function filter(item, other) if other == coin then return "cross" end return "slide" end
        x, y, cols, len = world:move(player, 4, 40, filter)
        out[#out + 1] = len .. ":" .. cols[1].other.name .. ":" .. cols[1].type .. ":" .. y
        local items, n = world:queryRect(0, 0, 30, 30)
        out[#out + 1] = n
        out[#out + 1] = tostring(world:hasItem(coin)) .. " " .. world:countItems()
        world:remove(coin)
        out[#out + 1] = tostring(world:hasItem(coin))
        function love.draw() love.graphics.print(table.concat(out, " ")) end
    "#;
    assert_eq!(run_frames(main, &[], 1, 0.25), ["4 10 1 wall -1 slide 1:coin:cross:40 1 true 3 false"]);
    assert!(error_of("local w = bump.newWorld() local a = {} w:add(a, 0, 0, 1, 1) w:add(a, 0, 0, 1, 1)").contains("Item already added"));
    let e = error_of("local w = bump.newWorld() local a, b = {}, {} w:add(a, 0, 0, 1, 1) w:add(b, 2, 0, 1, 1)\nw:move(a, 5, 0, function() return nil + 1 end)");
    assert!(e.starts_with("game/main.lua:2:") && !e.contains("game/main.lua:2: game/main.lua"), "{e}");
}

/// classic, the real library: classes, inheritance, is, mixins; and
/// require of the libraries by their usual names.
#[test]
fn classic_and_require_by_name() {
    let main = r#"
        local Object = require("classic")
        local Point = Object:extend()
        function Point:new(x, y) self.x, self.y = x or 0, y or 0 end
        function Point:__tostring() return "Point(" .. self.x .. ", " .. self.y .. ")" end
        local Rect = Point:extend()
        function Rect:new(x, y, w, h) Rect.super.new(self, x, y) self.w, self.h = w, h end
        local PairPrinter = Object:extend()
        function PairPrinter:printPairs() return "pairs" end
        Rect:implement(PairPrinter)
        local r = Rect(1, 2, 3, 4)
        local out = { tostring(Point(5, 6)), tostring(r:is(Point)), tostring(r:is(Rect)), tostring(Point(1, 1):is(Rect)), r.w, r:printPairs(),
          tostring(require("lib.flux") == flux), tostring(require("hump.timer") == timer), tostring(require("hump.camera")(10, 20) == camera), camera.x }
        function love.draw() love.graphics.print(table.concat(out, " ")) end
    "#;
    assert_eq!(run_frames(main, &[], 1, 0.25), ["Point(5, 6) true true false 3 pairs true true true 10"]);
}

/// Godot's Tween over flux: steps one after another, `finished` at the end.
#[test]
fn godot_tween_alias() {
    let main = r#"
        local s = Shape{ x = 0, y = 0 }
        local finished = 0
        local tw = s:createTween()
        tw:tweenProperty("x", 100, 1)
        tw:tweenProperty("y", 50, 1):setTrans("sine"):setEase("inOut")
        tw.finished:connect(function() finished = finished + 1 end)
        function love.draw() love.graphics.print(s.x .. " " .. string.format("%.2f", s.y) .. " " .. finished) end
    "#;
    // x linear (Godot's default), then y: sine in-out, halfway is half.
    assert_eq!(run_frames(main, &[], 2, 0.5), ["100 0.00 0"]);
    assert_eq!(run_frames(main, &[], 3, 0.5), ["100 25.00 0"]);
    assert_eq!(run_frames(main, &[], 4, 0.5), ["100 50.00 1"]);
    assert!(error_of("Shape{}:createTween():tweenProperty('x', 1, 1):setTrans('wobbly')").contains("setTrans: \"wobbly\""));
}

/// A Sprite plays its sheet's animations, once or looping, and tells its
/// signal when a once is done; the camera follows a node.
#[test]
fn sprites_play_and_the_camera_follows() {
    let main = r#"
        local hero = Sprite{ image = "player", x = 0, y = 300, vx = 100 }
        local ended = 0
        hero.animationFinished:connect(function() ended = ended + 1 end)
        hero:play("catch", { once = true })
        camera:follow(hero, { lag = 0 })
        function love.draw() love.graphics.print(hero.frame .. " " .. ended .. " " .. math.floor(camera.x + 0.5) .. " " .. tostring(hero.animationFinished == hero.animationFinished)) end
    "#;
    let toml = "frame = [64, 40]\n[anims]\nidle = { frames = \"1\", fps = 4 }\ncatch = { frames = \"2-1\", fps = 10 }\n";
    // catch is frames 2 then 1, at 10 a second: done after 0.2 s. love.draw
    // sees the camera as the last frame left it (it follows after motion).
    assert_eq!(run_frames(main, &[("images/player.toml", toml)], 1, 0.05), ["2 0 180 true"]);
    assert_eq!(run_frames(main, &[("images/player.toml", toml)], 6, 0.05), ["1 1 25 true"]);
    assert!(error_of("Sprite{ image = 'player' }:play('run')").contains("has no [anims]"));
}

/// A PNG's header, all the project reads of it natively.
fn png_header(w: u32, h: u32) -> Vec<u8> {
    let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
    b.extend(w.to_be_bytes());
    b.extend(h.to_be_bytes());
    b.extend([8, 6, 0, 0, 0]);
    b
}

/// The catch starter with a tile set and a map.
fn with_level(main: &str, map: &str) -> Files {
    let mut f = with_main(main, &[("tiles/ground.toml", "frame = [32, 32]\n[tiles.top]\nframe = 1\nsolid = true\n[tiles.ledge]\nframe = 2\noneway = true\n"), ("maps/level.map", map)]);
    f.insert("tiles/ground.png".into(), png_header(64, 32));
    f
}

const LEVEL: &str = "[legend]\n\"#\" = \"ground.top\"\n\"=\" = \"ground.ledge\"\n\"c\" = { thing = \"item\", vx = 5, worth = 3 }\n\"@\" = { marker = \"start\" }\n\n[map]\n.....\n.@.c.\n..==.\n#####\n";

#[test]
fn tilemaps_place_their_things_and_answer_where_tiles_are() {
    let main = r#"
        local m = Tilemap{ map = "level", x = 100 }
        local sx, sy = m:marker("start")
        local item = scene.all("item")[1]
        function love.draw()
          love.graphics.print(m.w .. " " .. m.h .. " " .. tostring(m:solidAt(110, 101)) .. " " .. tostring(m:solidAt(110, 5)) .. " " .. m:tileAt(165, 65) .. " " .. tostring(m:tileAt(0, 0)))
          love.graphics.print(sx .. " " .. sy .. " " .. #scene.all("item") .. " " .. item.y .. " " .. item.data.worth .. " " .. m.map .. " " .. m.tiles)
        end
    "#;
    let mut v = vm(&with_level(main, LEVEL)).unwrap_or_else(|e| panic!("{e}"));
    v.step(0.5, &[], &Held::default(), false, false).unwrap();
    assert_eq!(printed(&v), ["160 128 true false ledge nil", "148 48 1 48 3 level ground"]);
    let h = v.host();
    let item = h.scene.all("item")[0];
    // Made at x 212 (the cell's middle), then vx 5 for half a second.
    assert_eq!(h.scene.get(item).unwrap().x, 214.5);
    let map = h.scene.ids().into_iter().find(|i| h.scene.get(*i).unwrap().kind == Kind::Tilemap).unwrap();
    let tm = h.scene.get(map).unwrap().tilemap.as_ref().unwrap();
    assert_eq!((tm.cols, tm.rows, tm.tiles.len(), tm.image), (5, 4, 2, h.project.image("tiles/ground").map(|i| i as u16)));
    assert!(h.nearest[h.project.image("tiles/ground").unwrap()], "tiles are pixel art by default");

    let err = |main: &str, map: &str| match Project::parse(&with_level(main, map)).and_then(|_| vm(&with_level(main, map))) {
        Err(e) => e,
        Ok(mut v) => v.step(1.0 / 60.0, &[], &Held::default(), false, false).expect_err("an error"),
    };
    assert!(err("Tilemap{}", LEVEL).contains("a Tilemap needs its map"));
    assert!(err("Tilemap{ map = 'lvl' }", LEVEL).contains("no map \"lvl\" (maps: level)"));
    assert!(err("Tilemap{ map = 'level' }.map = 'other'", LEVEL).contains("set when it's made"));
    assert!(err("Tilemap{ map = 'level' }:marker('end')", LEVEL).contains("no marker \"end\" (start)"));
    assert!(err("", "[legend]\n\"#\" = \"ground.rock\"\n[map]\n#\n").contains("maps/level.map:4: '#' is tile \"ground.rock\", but tiles/ground.toml has no tile \"rock\" (top, ledge)"));
    assert!(err("", "[legend]\n[map]\n.x\n").contains("maps/level.map:3: 'x' isn't in the legend"));
}

/// What `mb play` reads for its budgets and the grounding check: each
/// frame description counts the Lua's interrupts, and with `mb.ground` it
/// has every Walker on the view standing on the floor under it.
#[test]
fn frame_descriptions_count_lua_and_stand_walkers_on_their_floors() {
    let host = Host::start(5);
    maimbrain::sys::test::set_param("mb.ground", "1");
    let main = r#"
        Tilemap{ map = "level" }
        Walker{ name = "fox", x = 48, y = 96, height = 40 }
        function love.update(dt)
          local n = 0
          for i = 1, 500 do n = n + i end
        end
    "#;
    let mut f = with_level(main, LEVEL);
    f.insert(crate::project::PACKED.into(), Vec::new());
    use_project(f);
    let mut g: Studio2 = host.init();
    host.run(&mut g, 0.3, 1.0 / 60.0);
    maimbrain::sys::test::clear_params();
    let d = g.dump();
    let lua: i64 = d.split("\"lua\":").nth(1).and_then(|r| r.split([',', '}']).next()).and_then(|n| n.parse().ok()).unwrap_or_else(|| panic!("{d}"));
    assert!(lua >= 500, "a loop of 500 counts: {d}");
    assert!(d.contains("\"ground\":\"ffox,"), "{d}");
    assert!(d.contains(",96.0,1,48.0,") && d.contains("F96.0,32.0,64.0;"), "standing on the ground's top at y 96: {d}");
}

/// The readability check (`studio_readability`): the frame description
/// names the subject, `round.subject` over the node the camera follows,
/// with its box and what's behind it; and the frame draws its variants
/// (`studio_isolate`) without touching the run.
#[test]
fn the_readability_check_names_its_subject_and_draws_it_alone() {
    use maimbrain::sys::test::{clear_params, set_param};
    let host = Host::start(5);
    set_param("studio_readability", "1");
    let main = r#"
        Tilemap{ map = "level", name = "moss" }
        local fox = Walker{ name = "fox", x = 48, y = 96, height = 40 }
        local hero = Shape{ name = "hero", x = 100, y = 300, w = 40, h = 40 }
        camera:follow(fox, { lag = 0 })
        function love.update()
          if round.time > 0.2 then round.subject = hero end
        end
    "#;
    let mut f = with_level(main, LEVEL);
    f.insert(crate::project::PACKED.into(), Vec::new());
    use_project(f);
    let mut g: Studio2 = host.init();
    host.run(&mut g, 0.1, 1.0 / 60.0);
    let d = g.dump();
    assert!(d.contains(r#""subject":{"name":"fox","why":"the camera follows it","kind":"2d","box":["#), "{d}");
    assert!(d.contains(r#""behind":["moss"]"#), "the tiles under its feet: {d}");
    host.run(&mut g, 0.3, 1.0 / 60.0);
    let d = g.dump();
    assert!(d.contains(r#""subject":{"name":"hero","why":"round.subject""#), "{d}");
    assert_eq!(crate::headless::Isolate::asked(&g.hooks), None);
    set_param("studio_isolate", "subject");
    assert_eq!(crate::headless::Isolate::asked(&g.hooks), Some(crate::headless::Isolate::Subject));
    set_param("studio_isolate", "clean");
    assert_eq!(crate::headless::Isolate::asked(&g.hooks), Some(crate::headless::Isolate::Clean));
    clear_params();
}

#[test]
fn walkers_walk_and_say_when_a_foot_lands() {
    let main = r#"
        local w = Walker{ x = 0, y = 300, vx = 150, height = 60 }
        local steps, last = 0, 0
        w.stepped:connect(function(foot, x, y) steps = steps + 1 last = foot end)
        function love.draw() love.graphics.print(steps .. " " .. last .. " " .. w.h .. " " .. tostring(w.flipX) .. " " .. w.speed) end
    "#;
    let seen = run_frames(main, &[], 90, 1.0 / 60.0);
    let parts: Vec<&str> = seen[0].split(' ').collect();
    assert!(parts[0].parse::<u32>().unwrap() >= 3, "it stepped: {seen:?}");
    assert!(matches!(parts[1], "1" | "2"));
    assert_eq!(parts[2..], ["60", "false", "150"]);
    assert!(error_of("Walker{ image = 'item' }").contains("isn't a walker's picture"));
    let ok = run_frames("local w = Walker{ image = 'item', height = 40 } function love.draw() love.graphics.print(w.image) end", &[("images/item.toml", "body = [0, 0, 32, 24]\nfoot = [0, 24, 12, 8]\n")], 1, 0.1);
    assert_eq!(ok, ["item"]);
}

/// `fx.flash` starts at the colour's own alpha when it gives one, else at 0xb0.
#[test]
fn a_flash_keeps_the_colours_alpha() {
    for (call, want) in [
        ("fx.flash(0xff0000)", 0xff0000b0),
        ("fx.flash({ 1, 1, 1, 0.25 })", 0xffffff40),
        ("fx.flash({ 0, 0, 1 })", 0x0000ffb0),
        ("fx.flash()", 0xffffffb0),
    ] {
        let mut v = vm(&with_main(call, &[])).unwrap_or_else(|e| panic!("{e}"));
        v.step(1.0 / 60.0, &[], &Held::default(), false, false).unwrap();
        assert_eq!(v.host().scene.flash.map(|f| f.0), Some(want), "{call}");
    }
}

#[test]
fn emitters_particle_systems_and_effects() {
    let main = r#"
        local e = Emitter{ preset = "sparkle", x = 10, y = 10, colors = { 0xff0000, 0x00ff00 }, size = 4 }
        e:emit(10)
        local smoke = Emitter{ preset = "smoke", rate = 30 }
        local ps = love.graphics.newParticleSystem("dust", 100)
        ps:setPosition(5, 5)
        ps:emit(4)
        local runner = Shape{ x = 0, vx = 60 }
        fx.burst("confetti", 0, 0, { count = 5 })
        fx.popup("+5", 0, 0, { color = 0xffff00 })
        fx.flash(0xff0000, 0.2)
        fx.shake(5, 0.2)
        fx.hitstop(0.05)
        function love.draw()
          love.graphics.draw(ps, 1, 2)
          love.graphics.print(e.preset .. " " .. #e.colors .. " " .. e.size .. " " .. ps:getCount() .. " " .. tostring(smoke.emitting) .. " " .. runner.x)
        end
    "#;
    let mut v = vm(&with_main(main, &[])).unwrap_or_else(|e| panic!("{e}"));
    let held = Held::default();
    v.step(1.0 / 60.0, &[], &held, false, false).unwrap();
    {
        let h = v.host();
        assert_eq!(printed(&v), ["sparkles 2 4 4 true 0"]);
        assert!(h.draw.cmds().contains(&Cmd::Particles { system: 0, x: 1.0, y: 2.0 }));
        assert!(h.scene.fx.particles.len() == 5 && h.scene.popups.list.len() == 1 && h.scene.flash.is_some());
        assert!(h.scene.camera.shake_x != 0.0 || h.scene.camera.shake_y != 0.0);
        let e = h.scene.ids().into_iter().find(|i| h.scene.get(*i).unwrap().kind == Kind::Emitter).unwrap();
        assert_eq!(h.scene.get(e).unwrap().emitter.as_ref().unwrap().particles.len(), 10);
    }
    // The hitstop held the world for its 0.05 s; then it moves.
    for _ in 0..3 {
        v.step(1.0 / 60.0, &[], &held, false, false).unwrap();
    }
    assert!(printed(&v)[0].ends_with(" 0"), "{:?}", printed(&v));
    v.step(1.0 / 60.0, &[], &held, false, false).unwrap();
    v.step(1.0 / 60.0, &[], &held, false, false).unwrap();
    assert!(!printed(&v)[0].ends_with(" 0"), "{:?}", printed(&v));
    assert!(error_of("Emitter{ preset = 'sparkel' }").contains("did you mean \"sparkles\""));
    assert!(error_of("fx.burst('glitter', 0, 0)").contains("fx.burst: preset \"glitter\" isn't one"));
}

/// Flappy and the runner: the platform's rules; the title card plays by
/// itself (scoring); a round nobody plays ends.
#[test]
fn flappy_and_runner_play() {
    for (name, (w, h)) in [("flappy", (360.0, 640.0)), ("runner", (640.0, 360.0))] {
        let manifest = MANIFEST.replace("[360, 640]", &format!("[{w}, {h}]")).replace("\"portrait\"", if w > h { "\"landscape\"" } else { "\"portrait\"" });
        use_project(starter(name));
        Conformance::new().max_round(60.0).manifest(&manifest).check::<Studio2>();

        let host = Host::start(5);
        use_project(starter(name));
        let mut g: Studio2 = host.init();
        host.run(&mut g, 8.0, 1.0 / 60.0);
        assert!(g.error.is_none(), "{name}: {:?}", g.error);
        let title = g.title.as_ref().expect("the attract mode");
        assert!(title.host().last_score >= 1.0, "{name}: the title card plays ({})", title.host().last_score);
        host.tap(w / 2.0, h / 2.0);
        host.run(&mut g, 0.5, 1.0 / 60.0);
        assert!(g.round.is_some(), "{name}: a tap starts a round");
        host.run(&mut g, 20.0, 1.0 / 60.0);
        assert!(g.error.is_none(), "{name}: {:?}", g.error);
        assert!(g.dump().contains("\"mode\":\"over\""), "{name}: {}", g.dump());
    }
}


// ------------------------------------------------------- the quick path

/// The catch starter's pictures as kinds (`images/<name>.toml`), a 32-unit
/// tile set, and a map.
fn quick_files(main: &str, map: &str, more: &[(&str, &str)]) -> Files {
    let mut f = with_main(main, more);
    f.insert("tiles/ground.toml".into(), b"frame = [32, 32]\n[tiles.top]\nframe = 1\nsolid = true\n[tiles.spike]\nframe = 2\nhazard = true\n".to_vec());
    f.insert("tiles/ground.png".into(), png_header(64, 32));
    f.insert("maps/level.map".into(), map.as_bytes().to_vec());
    f
}

/// Runs frames with input: `at(frame)` gives that frame's events and held.
fn play_frames(files: &Files, n: usize, at: &dyn Fn(usize) -> (Vec<LoveEvent>, Held)) -> Vm {
    let mut v = vm(files).unwrap_or_else(|e| panic!("{e}"));
    for i in 0..n {
        let (ev, held) = at(i);
        v.step(1.0 / 60.0, &ev, &held, false, false).unwrap_or_else(|e| panic!("frame {i}: {e}"));
    }
    v
}

fn none(_: usize) -> (Vec<LoveEvent>, Held) {
    (Vec::new(), Held::default())
}

const FLOOR: &str = "[legend]\n\"#\" = \"ground.top\"\n\"^\" = \"ground.spike\"\n\"@\" = { marker = \"start\" }\n[map]\n...........\n...........\n...........\n...........\n.@.........\n...........\n###########\n";

#[test]
fn flappy_flaps_falls_and_crashes_into_hazard_tiles() {
    let main = r#"
        local map = Tilemap{ map = "level" }
        local x, y = map:marker("start")
        bird = Sprite{ image = "item", x = x, y = y, behaviour = "player.flappy" }
        flaps, hurts = 0, 0
        on("flap", function(t) flaps = flaps + 1 end)
        on("hurt", function(t) hurts = hurts + 1 end)
        function love.draw() love.graphics.print(flaps .. " " .. hurts .. " " .. math.floor(bird.vy) .. " " .. round.lives .. " " .. tostring(round.level)) end
    "#;
    let map = "[legend]\n\"^\" = \"ground.spike\"\n\"@\" = { marker = \"start\" }\n[map]\n...........\n.@.........\n...........\n...........\n^^^^^^^^^^^\n";
    let tap = |i: usize| if i == 5 { (vec![LoveEvent::MousePressed { x: 100.0, y: 100.0, button: 1, istouch: true }], Held::default()) } else { none(i) };
    let v = play_frames(&quick_files(main, map, &[]), 6, &tap);
    // The flap: vy = -170 × 2.5 (1.0's knob in units), then a frame of gravity.
    let seen = printed(&v);
    let parts: Vec<&str> = seen[0].split(' ').collect();
    assert_eq!((parts[0], parts[1], parts[3], parts[4]), ("1", "0", "1", "level"), "{seen:?}");
    assert!(parts[2].parse::<f64>().unwrap() < -380.0, "{seen:?}");
    // Falling onto the spikes takes the one life: the engine ends the round.
    let v = play_frames(&quick_files(main, map, &[]), 120, &none);
    assert_eq!(printed(&v)[0].split(' ').nth(1), Some("1"));
    assert_eq!(v.host().last_lives, 0.0);
}

#[test]
fn platformers_land_walk_jump_and_stop_at_walls() {
    let main = r#"
        local map = Tilemap{ map = "level" }
        local x, y = map:marker("start")
        hero = Sprite{ image = "item", x = x, y = y - 40, behaviour = "player.platformer", knobs = { speed = 200 } }
        lands, jumps = 0, 0
        on("land", function() lands = lands + 1 end)
        on("jump", function() jumps = jumps + 1 end)
        function love.draw() love.graphics.print(lands .. " " .. jumps .. " " .. math.floor(hero.x) .. " " .. math.floor(hero.y) .. " " .. hero.knobs.speed .. " " .. hero.behaviour) end
    "#;
    // Falls and lands on the floor (its top at y 192: the box's middle at 176).
    let v = play_frames(&quick_files(main, FLOOR, &[]), 60, &none);
    assert_eq!(printed(&v), ["1 0 48 176 200 player.platformer"]);
    // Held right walks it to the map's right edge (a wall), then space jumps.
    let walk = |i: usize| {
        let mut held = Held::default();
        let mut ev = Vec::new();
        if i >= 30 {
            held = held_keys(&["right"]);
        }
        if i == 200 {
            ev.push(LoveEvent::KeyPressed("space"));
        }
        (ev, held)
    };
    let v = play_frames(&quick_files(main, FLOOR, &[]), 205, &walk);
    let seen = printed(&v);
    let p: Vec<f64> = seen[0].split(' ').take(4).map(|x| x.parse().unwrap()).collect();
    assert_eq!((p[0], p[1], p[2]), (1.0, 1.0, 336.0), "against the right edge at 352: {seen:?}");
    assert!(p[3] < 176.0, "jumping: {seen:?}");
    // By touch with two thumbs (the owner's phone check of Lantern Path):
    // the first finger held to the right walks; the second, tapped above,
    // jumps, and the first keeps walking. The second finger isn't the mouse,
    // and its tap used to do nothing.
    let thumbs = |i: usize| {
        let mut held = Held::default();
        let mut ev = Vec::new();
        if i >= 30 {
            (held.mouse[0], held.mx, held.my) = (true, 340.0, 185.0);
            held.touches.push((1, 340.0, 185.0));
        }
        if i == 40 {
            ev.push(LoveEvent::TouchPressed { id: 2, x: 120.0, y: 20.0, pressure: 1.0 });
        }
        if i >= 40 {
            held.touches.push((2, 120.0, 20.0));
        }
        (ev, held)
    };
    let v = play_frames(&quick_files(main, FLOOR, &[]), 46, &thumbs);
    let p: Vec<f64> = printed(&v)[0].split(' ').take(4).map(|x| x.parse().unwrap()).collect();
    assert_eq!(p[1], 1.0, "the second thumb jumped: {p:?}");
    assert!(p[2] > 48.0 + 200.0 * 15.0 / 60.0 - 1.0, "and the first kept walking right all the while: {p:?}");
    assert!(p[3] < 176.0, "in the air: {p:?}");
}

/// A platformer by touch, as a first-timer meets it (the owner's phone
/// check): hold beside it to walk, at any height, without jumping; a press
/// on it jumps at once and a hold there stands; the walking thumb flicked
/// up jumps on the way, and lifting it in the air keeps the jump going; a
/// lifted thumb eases the walk to a stop; a tap gets the whole jump, a graze
/// a short one.
#[test]
fn platformers_by_touch_walk_beside_and_jump_by_flick_tap_or_second_thumb() {
    let main = r#"
        local map = Tilemap{ map = "level" }
        local x, y = map:marker("start")
        hero = Sprite{ image = "item", x = x, y = y - 40, behaviour = "player.platformer", knobs = { speed = 200 } }
        jumps, miny, jumped_at = 0, 1000, -1
        on("jump", function() jumps = jumps + 1; if jumped_at < 0 then jumped_at = frame end end)
        frame = 0
        function love.update() frame = frame + 1; if hero.y < miny then miny = hero.y end end
        function love.draw() love.graphics.print(jumps .. " " .. math.floor(hero.x) .. " " .. math.floor(hero.y) .. " " .. math.floor(miny) .. " " .. jumped_at) end
    "#;
    // One finger: where it is at each frame (None: up); pressed when it comes down.
    fn run(main: &str, n: usize, path: &dyn Fn(usize) -> Option<(f32, f32)>) -> Vec<f64> {
        let finger = |i: usize| {
            let mut held = Held::default();
            let mut ev = Vec::new();
            let before = if i == 0 { None } else { path(i - 1) };
            if let Some((x, y)) = path(i) {
                (held.mouse[0], held.mx, held.my) = (true, x, y);
                held.touches.push((1, x, y));
                if before.is_none() {
                    ev.push(LoveEvent::TouchPressed { id: 1, x, y, pressure: 1.0 });
                    ev.push(LoveEvent::MousePressed { x, y, button: 1, istouch: true });
                }
            } else if let Some((x, y)) = before {
                ev.push(LoveEvent::TouchReleased { id: 1, x, y });
                ev.push(LoveEvent::MouseReleased { x, y, button: 1, istouch: true });
            }
            (ev, held)
        };
        let v = play_frames(&quick_files(main, FLOOR, &[]), n, &finger);
        printed(&v)[0].split(' ').map(|x| x.parse().unwrap()).collect()
    }
    let run = |n: usize, path: &dyn Fn(usize) -> Option<(f32, f32)>| run(main, n, path);
    // Held beside it and well above its head: it walks right and never jumps
    // (a press above the head used to jump, so walking by touch kept hopping).
    let p = run(60, &|i| (i >= 30).then_some((300.0, 20.0)));
    assert_eq!(p[0], 0.0, "no jump: {p:?}");
    assert!(p[1] > 48.0 + 200.0 * 25.0 / 60.0, "walked right: {p:?}");
    // A press on it (above its head, inside its column) jumps that very
    // frame, and held there it stands.
    let p = run(80, &|i| (i >= 40).then_some((48.0, 60.0)));
    assert_eq!((p[0], p[1]), (1.0, 48.0), "jumped, on the spot: {p:?}");
    assert!((40.0..=42.0).contains(&p[4]), "at once: {p:?}");
    // A tap (released after 0.1 s) gets the whole jump; a graze (0.03 s) a short one.
    let tap = run(100, &|i| (40..46).contains(&i).then_some((48.0, 150.0)));
    let graze = run(100, &|i| (40..42).contains(&i).then_some((48.0, 150.0)));
    let full = run(100, &|i| (40..90).contains(&i).then_some((48.0, 150.0)));
    assert_eq!(tap[3], full[3], "a tap's jump is as high as a held one: {tap:?} {full:?}");
    assert!(graze[3] > tap[3] + 20.0, "a graze's is shorter: {graze:?} {tap:?}");
    // Walking right, the thumb flicked up (60 units in 4 frames) jumps on
    // the way; lifted at the end of the flick, the jump keeps going right.
    let flick = |i: usize| match i {
        30..50 => Some((300.0, 185.0)),
        50..54 => Some((300.0, 185.0 - 15.0 * (i - 49) as f32)),
        _ => None,
    };
    let p = run(70, &flick);
    assert_eq!(p[0], 1.0, "the flick jumped: {p:?}");
    assert!((51.0..=54.0).contains(&p[4]), "during the flick, not after: {p:?}");
    let x_lift = 48.0 + 200.0 * 23.0 / 60.0;
    assert!(p[1] > x_lift + 200.0 * 10.0 / 60.0, "still going right in the air after the lift: {p:?}");
    // On the ground, a lifted thumb eases the walk to a stop: a few units on, not dead.
    let p = run(80, &|i| (30..60).contains(&i).then_some((300.0, 185.0)));
    let walked = 48.0 + 200.0 * 30.0 / 60.0;
    assert!(p[1] > walked + 2.0 && p[1] < walked + 200.0 * EASE_SECS, "eased to a stop: {p:?}");
}
const EASE_SECS: f64 = crate::quick::EASE as f64;

fn held_keys(keys: &[&'static str]) -> Held {
    let mut m = crate::input::Mapper::default();
    for k in keys {
        let code = (0..=230).find(|c| crate::input::key_name(*c) == Some(k)).unwrap();
        let e = maimbrain::input::Event { kind: maimbrain::input::Kind::KeyDown as u8, code, ..Default::default() };
        m.feed(&e, (0.0, 0.0, 1.0));
    }
    m.held.clone()
}

#[test]
fn pickups_score_hazards_hurt_with_a_blink_and_drag_follows_the_finger() {
    let main = r#"
        kind("gem", { w = 20, h = 20, behaviour = "pickup", knobs = { value = 5 } })
        kind("thorn", { w = 20, h = 20, behaviour = "hazard" })
        hero = Sprite{ image = "item", x = 100, y = 100, behaviour = "player.drag" }
        spawn("gem", { x = 100, y = 100 })
        spawn("thorn", { x = 300, y = 300 })
        got = 0
        on("pickup", function(p, g) got = got + 1 end)
        function love.draw() love.graphics.print(round.score .. " " .. got .. " " .. count("gem") .. " " .. round.lives .. " " .. math.floor(hero.x) .. " " .. math.floor(hero.y)) end
    "#;
    let mut f = quick_files(main, FLOOR, &[]);
    f.insert("studio.toml".into(), String::from_utf8(f["studio.toml"].clone()).unwrap().replace("lives = 1", "lives = 3").into_bytes());
    // Down at (110, 100), dragged to (310, 300): the hero keeps its offset and lands on the thorn.
    let drag = |i: usize| {
        let mut held = Held::default();
        held.mouse[0] = i >= 1;
        (held.mx, held.my) = if i < 10 { (110.0, 100.0) } else { (310.0, 300.0) };
        let ev = if i == 1 { vec![LoveEvent::MousePressed { x: 110.0, y: 100.0, button: 1, istouch: true }] } else { vec![] };
        (ev, held)
    };
    let v = play_frames(&f, 30, &drag);
    // One hurt, though it stayed on the thorn: the blink keeps it safe.
    assert_eq!(printed(&v), ["5 1 0 2 300 300"]);
}

#[test]
fn spawners_patrols_shooters_and_bullets() {
    let main = r#"
        kind("blob", { w = 20, h = 20, behaviour = "enemy.patrol", knobs = { speed = 100 } })
        kind("shot", { w = 6, h = 6, behaviour = "bullet" })
        kind("gun", { w = 20, h = 20, behaviour = "enemy.shooter", spawns = "shot", knobs = { every = 0.5, aim = 0 } })
        local map = Tilemap{ map = "level" }
        local b = spawn("blob", { x = 120, y = 176 })
        spawn("gun", { x = 40, y = 120 })
        local rain = spawnEvery("blob", 0.25, { x = 200, y = 20 })
        shots, xs = 0, {}
        on("shoot", function(g, s) shots = shots + 1 end)
        timer.after(1.0, function() rain:stop() end)
        function love.update() xs[#xs + 1] = math.floor(b.x) end
        function love.draw() love.graphics.print(count("blob") .. " " .. shots .. " " .. math.min(unpack(xs)) .. " " .. math.max(unpack(xs))) end
    "#;
    let map = "[legend]\n\"#\" = \"ground.top\"\n[map]\n#.........#\n#.........#\n#.........#\n#.........#\n#.........#\n#.........#\n###########\n";
    let v = play_frames(&quick_files(main, map, &[]), 120, &none);
    let seen = printed(&v);
    let p: Vec<f64> = seen[0].split(' ').map(|x| x.parse().unwrap()).collect();
    // 1 + three spawned before the timer stopped the rain at 1 s; two seconds of shots at 2 a second.
    assert_eq!((p[0], p[1]), (4.0, 4.0), "{seen:?}");
    // The patrol turned at the left wall (x 32 + half its width) and came back.
    assert!(p[2] <= 43.0 && p[3] > 120.0, "{seen:?}");
}

#[test]
fn overlaps_ticks_leaving_levels_and_speed() {
    let main = r#"
        kind("rock", { w = 20, h = 20 })
        Tilemap{ map = "level" }
        a = Shape{ tag = "fox", x = 100, y = 100, w = 20, h = 20 }
        local r = spawn("rock", { x = 105, y = 100, vx = -100 })
        hits, ticks, left = 0, 0, 0
        on("overlap", "fox", "rock", function(f, rock) hits = hits + 1 end)
        on("tick", "rock", function(rock, dt) ticks = ticks + 1 end)
        on("leave", "rock", function(rock) left = left + 1 end)
        speedUp(2)
        function love.draw() love.graphics.print(hits .. " " .. ticks .. " " .. left .. " " .. count("rock") .. " " .. tostring(nearest("rock", 0, 0) ~= nil) .. " " .. #all("fox")) end
    "#;
    // At 200 units/s (sped up) the rock passes the fox in about 0.1 s and leaves the 360-wide level by 0.6 s.
    let v = play_frames(&quick_files(main, FLOOR, &[]), 60, &none);
    let seen = printed(&v);
    let p: Vec<&str> = seen[0].split(' ').collect();
    assert!(p[0].parse::<u32>().unwrap() >= 5, "{seen:?}");
    assert_eq!(p[2..], ["1", "0", "false", "1"], "{seen:?}");
    // nextLevel past the last map ends the round, won.
    let mut v = vm(&quick_files("Tilemap{ map = 'level' } function love.update() nextLevel() end", FLOOR, &[])).unwrap();
    let s = v.step(1.0 / 60.0, &[], &Held::default(), false, false).unwrap();
    assert!(s.over.is_some_and(|o| o.won));
}

#[test]
fn names_from_elsewhere_teach_the_2_0_name() {
    assert!(error_of("score = score + 1").contains("`score` isn't in engine 2.0: use round.score"));
    assert!(error_of("game_over('x')").contains("round.over"));
    assert!(error_of("local p = Instance.new('Part')").contains("node constructor"));
    assert!(error_of("love.filesystem.read('x')").contains("save.get(key)"));
    assert!(error_of("love.graphics.setStencilTest('greater', 0)").contains("stencils aren't in 2.0"));
    assert!(error_of("love.window.setMode(800, 600)").contains("say size = [800, 600] in studio.toml"));
    assert!(error_of("on('touch', 'a', 'b', print)").contains("on(\"overlap\", tagA, tagB, fn)"));
    assert!(error_of("on('cue', print)").contains("a cutscene's { \"cue\", name } step fires it"));
    assert!(error_of("Sprite{ image = 'item', behaviour = 'player.fly' }").contains("no behaviour called 'player.fly' (bullet, enemy.chase"));
    assert!(error_of("spawn('ghost')").contains("no kind named 'ghost'"));
    // A global the game never set is still nil.
    assert!(run_frames("function love.draw() love.graphics.print(tostring(myThing)) end", &[], 1, 0.1) == ["nil"]);
}

// ------------------------------------------------------- saves and progress

/// What one round saves, the next reads; tables are the same bytes
/// whatever order they were filled in; progress has its own keys and a
/// checkpoint.
#[test]
fn saves_and_progress_round_trip() {
    let _host = Host::start(1);
    let write = r#"
        save.set("best", 42)
        save.set("name", "Ann")
        save.set("muted", true)
        save.set("deck", { cards = { "a", "b" }, size = 2, [10] = false })
        local a, b = {}, {}
        a.x = 1 a.y = 2 a.z = 3
        b.z = 3 b.y = 2 b.x = 1
        save.set("a", a) save.set("b", b)
        save.set("gone", 1) save.set("gone", nil)
        progress.set("unlocked", { "forest", "cave" })
        progress.checkpoint({ floor = 3, hp = 7 })
    "#;
    vm(&with_main(write, &[])).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(maimbrain::store::get("studio:a"), maimbrain::store::get("studio:b"), "same table, same bytes");
    let read = r#"
        local d = save.get("deck")
        local u = progress.get("unlocked")
        local r = progress.resume()
        out = { save.get("best"), save.get("name"), tostring(save.get("muted")), d.cards[2], d.size, tostring(d[10]), tostring(save.get("gone")), u[2], r.floor, tostring(progress.get("never")) }
        progress.clear()
        out[#out + 1] = tostring(progress.resume())
        function love.draw() love.graphics.print(table.concat(out, " ")) end
    "#;
    assert_eq!(run_frames(read, &[], 1, 0.1), ["42 Ann true b 2 false nil cave 3 nil nil"]);
    assert!(error_of("save.set('f', print)").contains("saves keep numbers, strings, true/false and plain tables, not function"));
    assert!(error_of("local t = {} t.t = t save.set('loop', t)").contains("nest at most 32"));
    assert!(error_of("progress.set('run', 1)").contains("progress.checkpoint"));
}

/// A game moved up from 1.0 (upgrade_engine) keeps its players' saves:
/// 1.0's text values read back as they were saved, and a value of this
/// format whose bytes start like one stays itself.
#[test]
fn engine_1_saves_read_back() {
    let _host = Host::start(1);
    for (k, v) in [("best", "n:42"), ("ratio", "n:0.5"), ("name", "s:Ann"), ("empty", "s:"), ("muted", "b:1"), ("sound", "b:0"), ("odd", "x:1")] {
        maimbrain::store::set(&format!("studio:{k}"), v.as_bytes());
    }
    // A 2.0 string of 58 bytes: its length's first byte is ':'.
    let long = "y".repeat(58);
    let mut bytes = vec![b's'];
    bytes.extend(58u32.to_le_bytes());
    bytes.extend(long.as_bytes());
    maimbrain::store::set("studio:long", &bytes);
    let read = r#"
        local out = {}
        for _, k in ipairs({ "best", "ratio", "name", "empty", "muted", "sound", "odd" }) do out[#out + 1] = tostring(save.get(k)) end
        out[#out + 1] = tostring(#save.get("long"))
        function love.draw() love.graphics.print(table.concat(out, "|")) end
    "#;
    assert_eq!(run_frames(read, &[], 1, 0.1), ["42|0.5|Ann||true|false|nil|58"]);
}

// ------------------------------------------------------- the debug menu

/// The menu has the scenarios, Studio's own rows, the kinds to spawn,
/// their knobs and the game's items; picks reach the round.
#[test]
fn the_debug_menu_reaches_the_round() {
    let main = r#"
        gap = 70
        debug.number("Gap", 70, function(v) gap = v end, 40, 120, 2)
        debug.toggle("Night", function(on) night = on end)
        debug.choice("Mode", { "easy", "hard" }, function(name, i) mode = name end)
        kind("blob", { w = 20, h = 20, behaviour = "enemy.chase" })
        function love.draw() love.graphics.print(gap .. " " .. tostring(night) .. " " .. tostring(mode) .. " " .. tostring(gold) .. " " .. tostring(debug.invincible()) .. " " .. count("blob")) end
    "#;
    let host = Host::start(3);
    let mut f = with_main(main, &[("debug/scenarios.toml", "[[scenario]]\nname = \"Gold rush\"\nnote = \"rich\"\nscore = 38\n[scenario.set]\ngold = true\n")]);
    f.insert(crate::project::PACKED.into(), Vec::new());
    use_project(f);
    let mut g: Studio2 = host.init();
    host.run(&mut g, 0.2, 1.0 / 60.0);
    let menu = host.debug_menu().expect("a menu");
    for label in ["Gold rush", "Invincible", "Score", "Show hitboxes", "Spawn blob", "blob · speed", "Gap", "Night", "Mode"] {
        assert!(menu.find_label(label).is_some(), "{label}");
    }
    host.tap(180.0, 320.0);
    host.run(&mut g, 0.2, 1.0 / 60.0);
    let id = |l: &str| host.debug_menu().unwrap().find_label(l).unwrap().id;
    host.debug_pick(id("Gap"), 90.0);
    host.debug_pick(id("Night"), 1.0);
    host.debug_pick(id("Mode"), 1.0);
    host.debug_pick(id("Invincible"), 1.0);
    host.debug_pick(id("Spawn blob"), 0.0);
    host.debug_pick(id("Score"), 25.0);
    host.run(&mut g, 0.1, 1.0 / 60.0);
    assert!(g.error.is_none(), "{:?}", g.error);
    let r = g.round.as_ref().unwrap();
    assert_eq!(printed(r), ["90 true hard nil true 1"]);
    assert_eq!(r.host().last_score, 25.0);
    // A scenario starts the round again with its picks (and keeps the rest).
    host.debug_pick(id("Gold rush"), 0.0);
    host.run(&mut g, 0.1, 1.0 / 60.0);
    let r = g.round.as_ref().unwrap();
    assert_eq!(printed(r), ["90 true hard true true 0"]);
    assert_eq!(r.host().last_score, 38.0);
}

// ------------------------------------------------------- identity and HUD

/// The identity in studio.toml themes the cards and the HUD; `ui.hud`
/// says what the HUD shows; a game's own node gets the value.
#[test]
fn identities_theme_the_cards_and_hud() {
    let main = r#"
        badge = Text{ text = "", x = 10, y = 10 }
        ui.hud{ score = badge, timer = 30, combo = 3 }
        function love.update() round.score = 7 end
    "#;
    let host = Host::start(4);
    let mut f = with_main(main, &[]);
    let toml = String::from_utf8(f["studio.toml"].clone()).unwrap().replace("[remix]", "[identity]\nname = \"swiss\"\nhud = \"badge\"\naccent = 0x2255ff\n\n[remix]");
    f.insert("studio.toml".into(), toml.into_bytes());
    use_project(f);
    let mut g: Studio2 = host.init();
    assert_eq!(g.theme.style.hud, maimbrain::ui::HudLayout::Badge);
    assert_eq!(g.theme.accent, 0x2255ffff);
    assert_eq!(g.rounds.title.theme, g.theme, "the cards are in it");
    host.tap(180.0, 320.0);
    host.run(&mut g, 0.3, 1.0 / 60.0);
    let r = g.round.as_ref().unwrap();
    let text = r.host().scene.ids().into_iter().filter_map(|n| r.host().scene.get(n)).find(|n| n.kind == Kind::Text).map(|n| n.text.clone());
    assert_eq!(text.as_deref(), Some("7"), "the HUD's score is written into the game's node");
    let (spec, _, _) = g.hud_spec.clone().unwrap();
    assert_eq!((spec.timer, spec.combo), (crate::api::ui::El::Num(30.0), crate::api::ui::El::Num(3.0)));
    // No identity: greybox.
    use_project(with_main("", &[]));
    let g: Studio2 = host.init();
    assert_eq!(g.theme.style, maimbrain::ui::Theme::preset("greybox").style);
    // The starters have their own (a placeholder too, until the game names its own).
    use_project(starter("catch"));
    let g: Studio2 = host.init();
    assert_eq!((g.theme.style, g.theme.accent), (maimbrain::ui::Theme::preset("cozy-pixel").style, 0xd8343bff));

}

// ------------------------------------------------------- the Arcade preset

/// An Arcade game with no identity keeps 1.0's score and hearts, and the
/// rest of what `ui.hud` asks for still shows under them: the best, the
/// meter's bar, the timer and combo pills. Once `ui.identity` names one,
/// the HUD is that identity's.
#[test]
fn an_arcade_hud_is_1_0s_score_with_the_rest_under_it() {
    let arcade = |main: &str| {
        let mut f = with_main(main, &[]);
        f.retain(|k, _| !k.starts_with("images/"));
        let toml = "preset = \"arcade\"\npalette = \"night16\"\nbackground = 2\n".to_string() + &String::from_utf8(f["studio.toml"].clone()).unwrap();
        f.insert("studio.toml".into(), toml.into_bytes());
        f
    };
    let main = r#"
        ui.hud{ timer = 90, combo = 3, best = true, meter = 0.5 }
        function love.update() round.score = 5 if round.time > 1 then round.over("DONE") end end
    "#;
    let host = Host::start(4);
    use_project(arcade(main));
    let mut g: Studio2 = host.init();
    host.run(&mut g, 0.5, 1.0 / 60.0);
    host.tap(72.0, 128.0);
    host.run(&mut g, 0.5, 1.0 / 60.0);
    assert_eq!(g.draw_hud(), ["arcade", "score", "bar", "1:30", "×3"], "no best before a round has ended");
    // The next round: the last one's 5 is the best to beat.
    host.run(&mut g, 4.0, 1.0 / 60.0);
    host.tap(72.0, 128.0);
    host.run(&mut g, 0.5, 1.0 / 60.0);
    assert!(g.round.is_some() && g.error.is_none(), "{:?}", g.error);
    assert_eq!(g.draw_hud(), ["arcade", "score", "best", "bar", "1:30", "×3"]);

    // ui.identity mid-game: the identity's HUD from then on.
    let main = "function love.update() if round.time > 0.2 and not switched then switched = true ui.identity(\"storybook\") end end";
    use_project(arcade(main));
    let mut g: Studio2 = host.init();
    host.run(&mut g, 0.5, 1.0 / 60.0);
    host.tap(72.0, 128.0);
    host.run(&mut g, 0.1, 1.0 / 60.0);
    assert_eq!(g.draw_hud().first().map(String::as_str), Some("arcade"));
    host.run(&mut g, 0.5, 1.0 / 60.0);
    assert_eq!(g.draw_hud().first().map(String::as_str), Some("identity"), "after ui.identity(\"storybook\")");
}

/// `preset = "arcade"`: a world of 144×256 art pixels, the palette's 16
/// colours as 0–15, the pixel font, 1.0's knobs as they were, 1.0's limits.
#[test]
fn the_arcade_preset_keeps_1_0s_look_and_limits() {
    let arcade = |main: &str, images: &[(&str, (u32, u32))]| {
        let mut f = with_main(main, &[]);
        f.retain(|k, _| !k.starts_with("images/"));
        let toml = "preset = \"arcade\"\npalette = \"night16\"\nbackground = 2\n".to_string() + &String::from_utf8(f["studio.toml"].clone()).unwrap();
        f.insert("studio.toml".into(), toml.into_bytes());
        for (n, (w, h)) in images {
            f.insert(format!("images/{n}.png"), png_header(*w, *h));
        }
        f
    };
    let main = r#"
        local s = Shape{ x = 10, y = 10, w = 4, h = 4, color = 12 }
        local t = Text{ text = "HI" }
        cat = Sprite{ image = "cat", x = 72, y = 100, behaviour = "player.flappy" }
        function love.draw() love.graphics.print(love.graphics.getWidth() .. " " .. s.color .. " " .. t.font .. " " .. t.size .. " " .. cat.knobs.flap) end
    "#;
    let f = arcade(main, &[("cat", (32, 16))]);
    let p = Project::parse(&f).unwrap();
    assert_eq!((p.size, p.unit(), p.background), ((144, 256), 1.0, Some(2)));
    // 1.0's art is drawn 2.5 units an art pixel: crisp (nearest), not blurred
    // (the owner's phone check: Puffin Hop's sprites were blurry), unless a picture says `pixel = false`.
    assert!(p.images.iter().all(|i| i.pixel), "Arcade pictures are pixel art");
    let mut soft = f.clone();
    soft.insert("images/cat.toml".into(), b"pixel = false\n".to_vec());
    assert!(!Project::parse(&soft).unwrap().images[0].pixel);
    let v = play_frames(&f, 1, &none);
    // night16's colour 12 is 0xffd23f; flap is 1.0's 170 art pixels a second.
    assert_eq!(printed(&v), [format!("144 {} pixel 8 170", 0xffd23f)]);
    assert!(v.host().scene.camera.snap);
    assert_eq!(v.host().background, 0x2e3a78ff);
    let g = { use_project(f.clone()); let host = Host::start(1); let g: Studio2 = host.init(); g };
    assert_eq!((g.theme.bg_top, g.theme.title_font), (0x2e3a78ff, maimbrain::gfx2d::Font::Pixel), "1.0's arcade cards in the project's colours");
    // 1.0's limits, saying how to lift them.
    let e = Project::parse(&arcade("", &[("big", (48, 48))])).unwrap_err();
    assert_eq!(e, "Arcade images are 16×16 or 32×32, and big.png is 48×48. Without preset = \"arcade\" any size works");
    assert!(vm(&arcade("love.graphics.newFont('Fraunces', 12)", &[])).err().unwrap().contains("Arcade games use the pixel font only"));
}

// ------------------------------------------------------- Lantern Path

pub(crate) fn example(name: &str) -> Files {
    let mut f = read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("examples").join(name));
    // What mb build writes: the fox's body and foot as it measures them.
    f.insert(crate::project::PACKED.into(), b"[images.fox]\nsize = [68, 24]\nfile = \"images/fox.png\"\nbody = [0, 0, 34, 21]\nfoot = [47, 19, 8, 4]\n".to_vec());
    f
}

/// §6.1's game plays: the title card shows the level; holding right walks
/// the fox along the path and into the first gap, which ends the round.
/// Jumping the gap reaches the lantern on the path, which lights.
#[test]
fn lantern_path_plays() {
    let f = example("lantern-path");
    // (Not Conformance's long-press check: a press held 1.5 s walks the fox
    // toward the finger and into the first gap, a fair ending.)
    let right = |_: usize| (Vec::new(), held_keys(&["right"]));
    let v = play_frames(&f, 240, &right);
    let over = v.host().over.clone().expect("it fell");
    assert_eq!((over.heading.as_deref(), over.cause.as_deref()), (Some("LOST IN THE DARK"), Some("fell")));
    // Right, with a jump at the gap's edge (x ≈ 430), lights the lantern at x 848.
    let mut vm2 = vm(&f).unwrap();
    let mut jumped = false;
    for i in 0..600 {
        let x = vm2.host().scene.all("fox").first().and_then(|n| vm2.host().scene.get(*n)).map_or(0.0, |n| n.x);
        let ev = if !jumped && x > 405.0 {
            jumped = true;
            vec![LoveEvent::KeyPressed("space")]
        } else {
            vec![]
        };
        let s = vm2.step(1.0 / 60.0, &ev, &held_keys(&["right"]), false, false).unwrap_or_else(|e| panic!("frame {i}: {e}"));
        if s.score >= 1.0 {
            return;
        }
    }
    panic!("no lantern lit: fox at {:?}", vm2.host().scene.all("fox").first().and_then(|n| vm2.host().scene.get(*n)).map(|n| (n.x, n.y)));
}

/// A bump filter that changes its own world, or makes a new one, is an
/// error from its line (not a dangling world or a Rust panic); another
/// world can still be used from a filter.
#[test]
fn bump_filters_cant_change_their_world() {
    let setup = r#"
        w = bump.newWorld(50)
        other = bump.newWorld(50)
        a, b, c = {}, {}, {}
        w:add(a, 0, 0, 10, 10)
        w:add(b, 5, 0, 10, 10)
        other:add(c, 0, 0, 10, 10)
    "#;
    for (body, want) in [
        ("w:queryRect(0, 0, 50, 50, function(i) w:remove(b) return true end)", "world:remove inside this world's own filter"),
        ("w:queryPoint(6, 5, function(i) bump.newWorld() return true end)", "bump.newWorld inside a bump filter"),
        ("w:move(a, 20, 0, function(i, o) w:update(b, 50, 50) return 'slide' end)", "world:update inside this world's own filter"),
        ("w:check(a, 20, 0, function(i, o) w:remove(o) return 'cross' end)", "world:remove inside this world's own filter"),
    ] {
        let e = error_of(&format!("{setup}\n{body}"));
        assert!(e.contains(want) && e.starts_with("game/main.lua:9:"), "{body}: {e}");
    }
    // The world is whole afterwards, and other worlds work inside a filter.
    let ok = run_frames(&format!("{setup}\nlocal n = 0\nw:queryRect(0, 0, 50, 50, function(i) n = n + other:countItems() return true end)\nlocal _, _, cols, len = w:move(a, 20, 0)\nfunction love.draw() love.graphics.print(n .. ' ' .. w:countItems() .. ' ' .. len) end"), &[], 1, 0.1);
    assert_eq!(ok, ["2 2 1"]);
}

/// A NaN saves as one NaN, whatever bits the CPU gave it, so progress is
/// the same bytes on the phone and in the server's replay.
#[test]
fn nan_saves_as_one_nan() {
    let _host = Host::start(1);
    vm(&with_main("local n = 0 / 0\nsave.set('a', n)\nsave.set('b', -n)\nsave.set('c', { x = -n })\nprogress.set('p', n)", &[])).unwrap_or_else(|e| panic!("{e}"));
    let mut want = vec![b'n'];
    want.extend(0x7ff8_0000_0000_0000u64.to_le_bytes());
    assert_eq!(maimbrain::store::get("studio:a"), Some(want.clone()));
    assert_eq!(maimbrain::store::get("studio:b"), Some(want.clone()));
    assert_eq!(maimbrain::progress::get("p"), Some(want));
    assert!(maimbrain::store::get("studio:c").unwrap().ends_with(&0x7ff8_0000_0000_0000u64.to_le_bytes()));
}

/// The heap's limit is a Lua error where Lua runs, even when what pushes
/// it over is the engine's own work outside Lua (a frame's events).
#[test]
fn the_heap_limit_is_a_clean_error() {
    let mut v = vm(&with_main("function love.update() local t = {} for i = 1, 10 do t[i] = {} end end", &[])).unwrap();
    v.step(1.0 / 60.0, &[], &Held::default(), false, false).unwrap();
    v.set_memory_limit(v.memory() + 512);
    let events: Vec<LoveEvent> = (0..2000).map(|i| LoveEvent::MousePressed { x: i as f32, y: 0.0, button: 1, istouch: false }).collect();
    let e = v.step(1.0 / 60.0, &events, &Held::default(), false, false).expect_err("over the limit");
    assert!(e.contains("not enough memory"), "{e}");
}

/// A debug pick writes the game's tables raw, so a metamethod the game
/// put there doesn't run outside Lua's protection.
#[test]
fn debug_picks_dont_run_the_games_metamethods() {
    let main = r#"
        local real = round
        round = setmetatable({}, { __index = real, __newindex = function(t, k, v)
          if k == "score" or k == "gold" then error("no writes") end
          real[k] = v
        end })
        function love.draw() love.graphics.print(tostring(rawget(round, "score"))) end
    "#;
    let mut v = vm(&with_main(main, &[])).unwrap();
    v.debug(vec![crate::debugmenu::DebugCmd::Score(25.0), crate::debugmenu::DebugCmd::Set("gold".into(), crate::project::Scalar::Bool(true))]).unwrap();
    v.step(1.0 / 60.0, &[], &Held::default(), false, false).unwrap();
    assert_eq!(printed(&v), ["25"]);
}


/// The engine's own entry points (`update`, `quick`, `over`, …) aren't a
/// global the game can reach.
#[test]
fn the_engines_entry_points_are_hidden() {
    assert_eq!(run_frames("function love.draw() love.graphics.print(tostring(__engine) .. ' ' .. tostring(rawget(_G, '__engine'))) end", &[], 1, 0.1), ["nil nil"]);
}

/// A backtracking pattern that would run for ages meets the frame's
/// budget like a loop does (Luau's matcher calls the interrupt).
#[test]
fn pattern_bombs_meet_the_budget() {
    let e = error_of("function love.update() local s = string.rep('a', 20000) s:find('.-.-.-.-.-.-.-.-b') end");
    assert!(e.contains("instruction budget exhausted"), "{e}");
}

/// Saved tables nested to the limit (and the cycle that hits it) use more
/// stack than a binding is given: it grows, rather than writing past it.
#[test]
fn deep_saved_tables_fit_the_stack() {
    let _host = Host::start(1);
    let main = r#"
        local t = {}
        local cur = t
        for i = 1, 31 do cur.next = { i = i } cur = cur.next end
        save.set("deep", t)
        local back = save.get("deep")
        local n = 0
        while back.next do back = back.next n = n + 1 end
        function love.draw() love.graphics.print(n) end
    "#;
    assert_eq!(run_frames(main, &[], 1, 0.1), ["31"]);
    assert!(error_of("local t = {} t.t = t save.set('loop', t)").contains("nest at most 32"));
}


/// Lantern Path's lantern dots and night bar are a round's HUD: not drawn
/// over the title card (the night's tint is).
#[test]
fn lantern_paths_hud_is_off_the_title_card() {
    let f = example("lantern-path");
    let p = Rc::new(Project::parse(&f).unwrap());
    let im = images(&p);
    let mut v = Vm::new(&p, 3, Mode::Title, &im, &[], &[], (0.0, 0.0, 640.0, 360.0)).unwrap();
    v.step(1.0 / 60.0, &[], &Held::default(), true, false).unwrap();
    let cmds = v.host().draw.cmds();
    assert!(!cmds.iter().any(|c| matches!(c, Cmd::Circle { .. })), "lantern dots on the title card");
    assert_eq!(cmds.iter().filter(|c| matches!(c, Cmd::Rect { .. })).count(), 1, "only the night's tint");
}

// ------------------------------------------------------- 1g: the kits' starters and 1.0's examples

/// A portrait or landscape manifest for Conformance.
fn manifest(w: f32, h: f32) -> String {
    MANIFEST.replace("[360, 640]", &format!("[{w}, {h}]")).replace("\"portrait\"", if w > h { "\"landscape\"" } else { "\"portrait\"" })
}

/// The title card plays by itself with no error; a tap starts a round; a
/// round nobody plays ends, on the game-over card.
fn plays_alone(f: Files, (w, h): (f32, f32), secs: f32) {
    let host = Host::start(5);
    use_project(f);
    let mut g: Studio2 = host.init();
    host.run(&mut g, 6.0, 1.0 / 60.0);
    assert!(g.error.is_none(), "title: {:?}", g.error);
    assert!(g.title.is_some(), "the attract mode");
    host.tap(w / 2.0, h / 2.0);
    host.run(&mut g, 0.5, 1.0 / 60.0);
    assert!(g.round.is_some(), "a tap starts a round");
    host.run(&mut g, secs, 1.0 / 60.0);
    assert!(g.error.is_none(), "{:?}", g.error);
    assert!(g.dump().contains("\"mode\":\"over\""), "{}", g.dump());
}

/// The runner and match3 kits as 2.0 starters (endless, match3): the
/// platform's rules, and a round that plays and ends.
#[test]
fn the_kit_starters_play() {
    for (name, secs) in [("endless", 10.0), ("match3", 30.0)] {
        use_project(starter(name));
        Conformance::new().max_round(60.0).manifest(&manifest(360.0, 640.0)).check::<Studio2>();
        plays_alone(starter(name), (360.0, 640.0), secs);
    }
}

/// Under the game-over card the world steps on, but nothing it does counts:
/// the quick path takes no more lives and scores no more pickups, and the
/// description shows the score the round sent (Puffin Hop's cliffs kept
/// hurting the fallen puffin to lives -3, and the summary said so).
#[test]
fn nothing_counts_under_the_game_over_card() {
    let files = read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/puffin-hop"));
    let host = Host::start(1);
    use_project(files);
    let mut g: Studio2 = host.init();
    host.run(&mut g, 1.0, 1.0 / 60.0);
    host.tap(180.0, 320.0);
    host.run(&mut g, 10.0, 1.0 / 60.0);
    let d = g.dump();
    assert!(d.contains("\"mode\":\"over\""), "{d}");
    let sent = host.scores().last().map(|s| s.1).expect("a score sent");
    assert!(d.contains(&format!("\"score\":{sent},")), "the card's score {sent}: {d}");
    assert!(d.contains("\"lives\":0"), "{d}");
    assert!(d.contains("\"card\":\"SPLASH!\""), "the card's heading: {d}");
    assert!(d.contains("\"identity\":\"cozy-pixel\""), "{d}");
    let lives = g.round.as_ref().unwrap().host().last_lives;
    assert_eq!(lives, 0.0, "no life taken after the last");
}

/// Gravity Wars (compat/gravity-wars), a LÖVE game from outside (MIT,
/// github.com/whyboris/Gravity-Wars): its Lua as written but for Luau's lack
/// of goto, studio.toml for a conf.lua. It loads (setMode for the canvas
/// studio.toml makes, `math.randomseed(os.time())`, a .ttf by its path,
/// cursors), aims by a drag near the ship and fires by a tap in the corner,
/// with the shot's trail kept on its canvas from frame to frame.
#[test]
fn gravity_wars_plays_as_written() {
    use maimbrain::input::synth;
    let mut files = read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("compat/gravity-wars"));
    // Its own font, as mb build bakes it (natively it draws as the sans).
    files.insert(crate::project::PACKED.into(), b"[fonts.basis33]\nfamily = \"basis33\"\nfile = \"fonts/basis33.mbf\"\n".to_vec());
    let host = Host::start(4);
    use_project(files);
    let mut g: Studio2 = host.init();
    host.run(&mut g, 1.0, 1.0 / 60.0);
    assert!(g.error.is_none(), "title: {:?}", g.error);
    host.tap(500.0, 400.0);
    host.run(&mut g, 0.5, 1.0 / 60.0);
    assert!(g.round.is_some(), "a tap starts a round");
    assert!(g.error.is_none(), "{:?}", g.error);

    // Player 1's ship: the first ship drawn below the bar (both are drawn in its corners too, at y 11).
    let cmds = |g: &Studio2| g.round.as_ref().unwrap().host().draw.cmds().to_vec();
    let ship = cmds(&g)
        .iter()
        .find_map(|c| match c {
            Cmd::Image { x, y, .. } if *y != 11.0 => Some((*x + 4.0, *y + 4.0)),
            _ => None,
        })
        .expect("the ship drawn");
    let angle = |g: &Studio2| g.round.as_ref().unwrap().host().draw.texts().map(str::to_string).collect::<Vec<_>>();
    let before = angle(&g);
    assert!(before.iter().any(|t| t == "360.00000"), "the angle shown: {before:?}");

    // Aim: a drag that starts 60 units right of the ship (the angle ring) and goes right.
    let t = maimbrain::sys::time() + 0.01;
    host.push(synth::stroke(&[(ship.0 + 60.0, ship.1), (ship.0 + 260.0, ship.1)], t, 1.0, synth::PANEL_HZ));
    host.run(&mut g, 1.5, 1.0 / 60.0);
    assert!(g.error.is_none(), "{:?}", g.error);
    let after = angle(&g);
    assert!(!after.iter().any(|t| t == "360.00000"), "the drag turned the aim: {after:?}");

    // Fire: a tap in the bottom-right corner, then the shot flies, leaving its trail.
    host.tap(950.0, 750.0);
    host.run(&mut g, 2.0, 1.0 / 60.0);
    assert!(g.error.is_none(), "{:?}", g.error);
    let trail = cmds(&g).iter().filter_map(|c| if let Cmd::Line { points, .. } = c { Some(points.len() / 2) } else { None }).max().unwrap_or(0);
    assert!(trail > 20, "the trail is one line of many segments, kept on the canvas: {trail} points");
}

/// Engine 1.0's examples on 2.0 (Cat Snack, Puffin Hop, Neon Dash under the
/// Arcade preset): the platform's rules, and a round that plays and ends.
#[test]
fn the_ported_examples_play() {
    let ex = |name: &str| read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("examples").join(name));
    for (name, size) in [("cat-snack", (360.0, 640.0)), ("puffin-hop", (360.0, 640.0)), ("neon-dash", (640.0, 360.0))] {
        use_project(ex(name));
        Conformance::new().max_round(60.0).manifest(&manifest(size.0, size.1)).check::<Studio2>();
        plays_alone(ex(name), size, 20.0);
    }
}


/// `player.platformer` on CharacterBody2D's controller (Phase 3b): it jumps
/// up through a one-way ledge, lands on it, and walks off its end.
#[test]
fn platformers_jump_up_through_ledges_and_land_on_them() {
    let main = r#"
        local map = Tilemap{ map = "level" }
        local x, y = map:marker("start")
        hero = Sprite{ image = "item", x = x, y = y, behaviour = "player.platformer", knobs = { speed = 200, jump = 800 } }
        lands, top = 0, 1000
        on("land", function() lands = lands + 1 end)
        function love.update() top = math.min(top, hero.y) end
        function love.draw() love.graphics.print(lands .. " " .. math.floor(hero.x) .. " " .. math.floor(hero.y) .. " " .. math.floor(top)) end
    "#;
    // A ledge (its top at y 96) over the start, the floor's top at 192.
    let map = "[legend]\n\"#\" = \"ground.top\"\n\"=\" = \"ground.ledge\"\n\"@\" = { marker = \"start\" }\n[map]\n...........\n...........\n...........\n.===.......\n...........\n.@.........\n###########\n";
    let mut files = quick_files(main, map, &[]);
    files.insert("tiles/ground.toml".into(), b"frame = [32, 32]\n[tiles.top]\nframe = 1\nsolid = true\n[tiles.ledge]\nframe = 2\noneway = true\n".to_vec());
    let jump = |i: usize| {
        let mut ev = Vec::new();
        if i == 40 {
            ev.push(LoveEvent::KeyPressed("space"));
        }
        (ev, Held::default())
    };
    let v = play_frames(&files, 120, &jump);
    let p: Vec<f64> = printed(&v)[0].split(' ').map(|x| x.parse().unwrap()).collect();
    // On the floor (box middle 176), then up through the ledge onto it (middle 80).
    assert_eq!((p[0], p[2]), (2.0, 80.0), "landed on the ledge: {p:?}");
    assert!(p[3] < 80.0, "rose above it from below: {p:?}");
    // Walking right off its end, it falls back to the floor.
    let walk = |i: usize| {
        let (ev, mut held) = jump(i);
        if i >= 120 {
            held = held_keys(&["right"]);
        }
        (ev, held)
    };
    let v = play_frames(&files, 200, &walk);
    let p: Vec<f64> = printed(&v)[0].split(' ').map(|x| x.parse().unwrap()).collect();
    assert_eq!((p[0], p[2]), (3.0, 176.0), "walked off the ledge onto the floor: {p:?}");
}

/// `Tilemap:path` (Phase 3b, Decided 48): A* round the solid tiles, as
/// cells' centres in the world, the same cells every time.
#[test]
fn tilemaps_find_paths_round_solid_tiles() {
    let main = r#"
        local map = Tilemap{ map = "level", x = 10 }
        local p = map:path(10 + 16, 16, 10 + 9 * 32 + 16, 16)
        local d = map:path(10 + 16, 16, 10 + 9 * 32 + 16, 16, { diagonal = true })
        local none = map:path(10 + 16, 16, 10 + 16, 6 * 32 + 16)
        function love.draw() love.graphics.print(#p / 2 .. " " .. #d / 2 .. " " .. tostring(none) .. " " .. p[1] .. "," .. p[2] .. " " .. p[#p - 1] .. "," .. p[#p]) end
    "#;
    // A wall down column 4 with a gap in the bottom row above the floor.
    let map = "[legend]\n\"#\" = \"ground.top\"\n[map]\n....#......\n....#......\n....#......\n....#......\n....#......\n...........\n###########\n";
    let v = play_frames(&quick_files(main, map, &[]), 1, &none);
    // Down five, across nine, up five: 20 cells; diagonally fewer.
    assert_eq!(printed(&v), ["20 13 nil 26,16 314,16"]);
}

/// A platformer destroyed takes its character out of the level's world: a
/// new one walks through where it stood (it used to stay as an invisible box).
#[test]
fn a_destroyed_platformer_leaves_nothing_in_the_way() {
    let main = r#"
        local map = Tilemap{ map = "level" }
        local x, y = map:marker("start")
        hero = Sprite{ image = "item", x = x + 96, y = y, behaviour = "player.platformer", knobs = { speed = 200 } }
        frame = 0
        function love.update()
          frame = frame + 1
          if frame == 30 then
            hero:destroy()
            hero = Sprite{ image = "item", x = x, y = y, behaviour = "player.platformer", knobs = { speed = 200 } }
          end
        end
        function love.draw() love.graphics.print(math.floor(hero.x) .. " " .. math.floor(hero.y)) end
    "#;
    let walk = |i: usize| {
        let mut held = Held::default();
        if i >= 40 {
            held = held_keys(&["right"]);
        }
        (Vec::new(), held)
    };
    let v = play_frames(&quick_files(main, FLOOR, &[]), 150, &walk);
    let p: Vec<f64> = printed(&v)[0].split(' ').map(|x| x.parse().unwrap()).collect();
    assert_eq!((p[0], p[1]), (336.0, 176.0), "walked past where the first one stood, to the right edge: {p:?}");
}

// ------------------------------------------------------- 3f: the stacker, shooter and tower-defense kits' starters

/// A 3f starter's manifest for Conformance: board 0 and a second board
/// (`daily = true` for the stacker's and the shooter's).
fn manifest_3f(name: &str, daily: bool) -> String {
    let second = format!("[[scores]]\nboard = 1\nlabel = \"Second\"\norder = \"higher\"\n{}", if daily { "daily = true\n" } else { "" });
    MANIFEST.replace("studio2.test", &format!("studio2.{name}")) + &second
}

/// The title card plays the game by itself, with no error, and moves.
fn title_plays_itself(name: &str) {
    let host = Host::start(5);
    use_project(starter(name));
    let mut g: Studio2 = host.init();
    host.run(&mut g, 8.0, 1.0 / 60.0);
    assert!(g.error.is_none(), "{name} title: {:?}", g.error);
    assert!(g.title.is_some(), "{name}: the attract mode");
    let drawn = host.drawn();
    assert!(drawn.windows(2).filter(|w| w[0] != w[1]).count() > 60, "{name}: the title card moves");
}

/// Picnic Pile: the platform's rules, tapping every 0.8 s to lose (an idle
/// bird never drops), and a title card where the bird stacks by itself.
#[test]
fn the_stacker_keeps_the_platform_rules() {
    use_project(starter("stacker"));
    Conformance::new().tap_every(0.8).max_round(120.0).manifest(&manifest_3f("stacker", true)).check::<Studio2>();
    title_plays_itself("stacker");
}

/// Nova Pip: the platform's rules (a ship nobody steers is shot down), and
/// a title card the pilot flies.
#[test]
fn the_shooter_keeps_the_platform_rules() {
    use_project(starter("shooter"));
    Conformance::new().max_round(120.0).manifest(&manifest_3f("shooter", true)).check::<Studio2>();
    title_plays_itself("shooter");
}

/// Cake Keep: the platform's rules with one POP built on the middle pad
/// (wave 1 waits for a first tower; one tower alone falls), and a title
/// card that builds and fights by itself.
#[test]
fn the_tower_defense_keeps_the_platform_rules() {
    use maimbrain::input::synth;
    use_project(starter("tower-defense"));
    // Pad 7 (180, 344), then POP in the picker above it (64 left, 70 up).
    let script = |t: f64| {
        if (0.5..0.5 + 1.0 / 60.0).contains(&t) {
            synth::tap(180.0, 344.0, 0.0)
        } else if (1.2..1.2 + 1.0 / 60.0).contains(&t) {
            synth::tap(116.0, 274.0, 0.0)
        } else {
            Vec::new()
        }
    };
    Conformance::new().play(script).max_round(240.0).manifest(&manifest_3f("tower-defense", false)).check::<Studio2>();
    title_plays_itself("tower-defense");
}

/// Each 3f starter's bot.lua plays it through `act` (natively, as `mb
/// difficulty --runner native` does): it presses, scores, ends rounds
/// without an error, and a good player outlasts a first-timer over the same
/// seeds. (Here, not bot_tests.rs, which is in the engine's fingerprint.)
#[test]
fn the_3f_starters_bots_play_them() {
    let field = |j: &str, k: &str| -> String {
        let i = j.find(&format!("\"{k}\":")).unwrap_or_else(|| panic!("no {k} in {j}")) + k.len() + 3;
        let rest = j[i..].trim_start_matches('"');
        rest[..rest.find(['"', ',', '}']).unwrap()].to_string()
    };
    for (name, cap) in [("stacker", 90.0), ("shooter", 120.0), ("tower-defense", 180.0)] {
        let f = starter(name);
        assert!(f.contains_key("game/bot.lua"), "{name} has a bot");
        let play = |preset: &str, seed: u64| -> (f64, f64) {
            let (line, _) = crate::bot::native_round(&f, preset, seed, cap, &[]).unwrap();
            assert!(!line.contains("\"error\""), "{name}: {line}");
            assert!(field(&line, "presses").parse::<u64>().unwrap() > 0, "{name}: {line}");
            (field(&line, "seconds").parse().unwrap(), field(&line, "score").parse().unwrap())
        };
        let median = |preset: &str| {
            let mut s: Vec<(f64, f64)> = (1..=3).map(|seed| play(preset, seed)).collect();
            assert!(s.iter().any(|r| r.1 > 0.0), "{name} {preset}: no score in {s:?}");
            s.sort_by(|a, b| a.0.total_cmp(&b.0));
            s[1].0
        };
        let (good, first) = (median("good"), median("first-timer"));
        assert!(good >= first, "{name}: good {good} s, first-timer {first} s");
    }
}

// ------------------------------------------------------- 4d: the trivia kit's starter

/// The manifest `mb build` writes for the trivia starter (its daily board too).
const TRIVIA_MANIFEST: &str = "abi = 0\nid = \"dev.maimbrain.studio2.trivia\"\nname = \"Bright Spark\"\nversion = \"0.1.0\"\ncreator = \"@maimbrain\"\norientation = \"portrait\"\nlogical_size = [360, 640]\ninputs = [\"touch\"]\nperf_tier = \"lite\"\nsensors = [\"haptics\"]\ncapabilities = [\"store\", \"score\"]\n[[scores]]\nboard = 0\nlabel = \"Points\"\norder = \"higher\"\nformat = \"integer\"\n[[scores]]\nboard = 1\nlabel = \"Daily points\"\norder = \"higher\"\nformat = \"integer\"\ndaily = true\n";

/// The trivia starter keeps the platform's rules, its title card plays its
/// loop, and a round nobody plays (three timeouts after the free warm-up)
/// ends on the game-over card.
#[test]
fn the_trivia_starter_keeps_the_platform_rules() {
    use_project(starter("trivia"));
    Conformance::new().max_round(90.0).manifest(TRIVIA_MANIFEST).check::<Studio2>();
    plays_alone(starter("trivia"), (360.0, 640.0), 60.0);
}

/// A tap starts a round and deals the warm-up; tapping the top answer every
/// 1.5 s gets some right and some wrong until the bulb blows and LIGHTS OUT!
/// comes, with the score sent. Returns the frames drawn.
fn trivia_round() -> Vec<u64> {
    let host = Host::start(3);
    use_project(starter("trivia"));
    let mut g: Studio2 = host.init();
    host.run(&mut g, 3.0, 1.0 / 60.0);
    assert!(g.error.is_none(), "{:?}", g.error);
    host.tap(180.0, 320.0);
    // The top answer's middle: 14 units over the bottom, four 50-unit buttons 9 apart.
    for _ in 0..80 {
        host.run(&mut g, 1.5, 1.0 / 60.0);
        if host.rounds().contains(&maimbrain::sys::Round::Over) {
            break;
        }
        host.tap(180.0, 640.0 - 14.0 - 4.0 * 50.0 - 3.0 * 9.0 + 25.0);
    }
    host.run(&mut g, 2.0, 1.0 / 60.0);
    assert!(g.error.is_none(), "{:?}", g.error);
    let mut rounds = host.rounds();
    rounds.dedup();
    assert_eq!(rounds, [maimbrain::sys::Round::Idle, maimbrain::sys::Round::Playing, maimbrain::sys::Round::Over]);
    let d = g.dump();
    assert!(d.contains("\"card\":\"LIGHTS OUT!\""), "{d}");
    assert!(host.scores().iter().any(|s| s.0 == 0), "the score went to board 0");
    host.drawn()
}

#[test]
fn the_trivia_round_plays_and_replays() {
    let a = trivia_round();
    assert_eq!(a, trivia_round(), "the same seed and taps draw the same frames");
    assert!(a.windows(2).any(|w| w[0] != w[1]), "frames change");
}

/// A trivia round on autopilot (every answer right, halfway through its
/// clock) for `secs`, with main.lua's debug globals set as `set` says
/// (`("START_AT = 0", "START_AT = 12")`). Returns what it logged.
fn trivia_autopilot(set: &[(&str, &str)], secs: f32) -> Vec<String> {
    let mut f = starter("trivia");
    let mut main = String::from_utf8(f["game/main.lua"].clone()).unwrap().replace("AUTOPLAY = false", "AUTOPLAY = true");
    for (from, to) in set {
        assert!(main.contains(from), "{from}");
        main = main.replace(from, to);
    }
    f.insert("game/main.lua".into(), main.into_bytes());
    compile(&mut f);
    let host = Host::start(9);
    use_project(f);
    let mut g: Studio2 = host.init();
    host.run(&mut g, 1.0, 1.0 / 60.0);
    host.tap(180.0, 320.0);
    host.run(&mut g, secs, 1.0 / 60.0);
    assert!(g.error.is_none(), "{:?}", g.error);
    let v = g.round.as_ref().expect("still playing");
    assert!(v.host().over.is_none() && v.host().last_score > 3000.0, "{}", v.host().last_score);
    host.calls()
        .into_iter()
        .filter_map(|c| match c {
            maimbrain::sys::test::Call::Log { message, .. } => Some(message),
            _ => None,
        })
        .collect()
}

/// Every answer right: play passes the golden questions, round 1's stats
/// card (which holds the world until it's covered, then deals the next
/// card), the streak multipliers and round 2's card, and no heart is lost.
/// Each card shows its own round's best streak (10), not the game's (20).
#[test]
fn the_trivia_autopilot_clears_rounds_of_the_show() {
    let logs = trivia_autopilot(&[], 95.0);
    assert!(logs.iter().any(|l| l.contains("trivia: question 11")), "play went on after round 1's card: {logs:?}");
    for n in [1, 2] {
        let line = logs.iter().find(|l| l.contains(&format!("trivia: round {n} done"))).unwrap_or_else(|| panic!("round {n}'s stats card: {logs:?}"));
        assert!(line.contains("10 of 10 right, best streak 10)"), "{line}");
    }
}

/// A scenario that starts past a round of the show (`START_AT` 12) answers
/// its way through round 1's break instead of hanging, and counts on from
/// round 2.
#[test]
fn the_trivia_can_start_past_a_round_of_the_show() {
    let logs = trivia_autopilot(&[("START_AT = 0 ", "START_AT = 12 ")], 40.0);
    assert!(logs.iter().any(|l| l.contains("trivia: question 13")), "{logs:?}");
    let line = logs.iter().find(|l| l.contains("trivia: round 2 done")).unwrap_or_else(|| panic!("round 2's stats card: {logs:?}"));
    assert!(line.contains("8 of 8 right, best streak 8)"), "{line}");
}

/// The trivia starter's bot.lua reads the sim through game/shared.lua and
/// plays as `act.preset` says: a good player knows and reads more than a
/// first-timer, so over the same questions it lasts longer; every round
/// ends with a miss (or at the cap). (Natively, as `mb difficulty --runner
/// native` plays it.)
#[test]
fn the_trivia_bot_reads_and_knows_by_preset() {
    let f = starter("trivia");
    let field = |j: &str, k: &str| -> String {
        let i = j.find(&format!("\"{k}\":")).unwrap_or_else(|| panic!("no {k} in {j}")) + k.len() + 3;
        let rest = j[i..].trim_start_matches('"');
        rest[..rest.find(['"', ',', '}']).unwrap()].to_string()
    };
    let play = |preset: &str, seed: u64, max: f64| -> f64 {
        let (line, _) = crate::bot::native_round(&f, preset, seed, max, &[]).unwrap();
        assert!(!line.contains("\"error\""), "{line}");
        assert!(field(&line, "capped") == "true" || field(&line, "cause").starts_with("missed a"), "{line}");
        assert!(field(&line, "presses").parse::<u32>().unwrap() > 3, "{line}");
        field(&line, "seconds").parse().unwrap()
    };
    let median = |preset: &str, max: f64| {
        let mut s: Vec<f64> = (1..=5).map(|seed| play(preset, seed, max)).collect();
        s.sort_by(f64::total_cmp);
        s[2]
    };
    let (good, first) = (median("good", 180.0), median("first-timer", 45.0));
    assert!(good > first && first > 20.0 && good > 45.0, "good {good} s, first-timer {first} s");
}
