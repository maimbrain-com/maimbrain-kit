//! The engine and the starters, natively: the platform's rules
//! (`Conformance`), replays, the instruction budget, the sandbox, the debug
//! menu, and keyboard and controller input.

use std::path::Path;
use std::rc::Rc;

use maimbrain::input::{Event, Kind, key, pad, synth};
use maimbrain::sys::test::{Conformance, Host};

use crate::project::{Files, Project};
use crate::vm::{DebugCmd, FrameInput, Mode, Vm};
use crate::{Studio, read_dir, use_project};

fn starter(name: &str) -> Files {
    read_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("starters").join(name))
}

/// The starter's files with main.lua replaced.
fn with_main(name: &str, main: &str) -> Files {
    let mut f = starter(name);
    f.insert("game/main.lua".into(), main.as_bytes().to_vec());
    f
}

fn project(files: &Files) -> Rc<Project> {
    Rc::new(Project::parse(files).unwrap())
}

/// The manifest `mb build` would write (for Conformance's daily-board check).
const MANIFEST: &str = "abi = 0\nid = \"dev.maimbrain.studio.test\"\nname = \"Test\"\nversion = \"0.1.0\"\ncreator = \"@maimbrain\"\norientation = \"portrait\"\nlogical_size = [360, 640]\ninputs = [\"touch\"]\nperf_tier = \"lite\"\ncapabilities = [\"store\", \"score\"]\n[[scores]]\nboard = 0\nlabel = \"Score\"\norder = \"higher\"\n";

fn conformance(name: &str) -> Conformance {
    use_project(starter(name));
    Conformance::new().max_round(60.0).manifest(MANIFEST)
}

/// The platform's round rules (SPEC §5.1–§5.2) on a fake host, for each
/// starter: a Studio game can't break them, because `Rounds` owns them.
#[test]
fn catch_keeps_the_platform_rules() {
    conformance("catch").check::<Studio>();
}

#[test]
fn flappy_keeps_the_platform_rules() {
    conformance("flappy").check::<Studio>();
}

#[test]
fn runner_keeps_the_platform_rules() {
    conformance("runner").check::<Studio>();
}

fn idle(dt: f32) -> FrameInput {
    let _ = dt;
    FrameInput::default()
}

/// Seconds until an idle round ends, and its score.
fn idle_round(name: &str, seed: u64) -> (f32, f64) {
    let p = project(&starter(name));
    let mut v = Vm::new(&p, seed, Mode::Round { level: None }).unwrap();
    let mut t = 0.0;
    loop {
        let s = v.step(1.0 / 60.0, &idle(1.0 / 60.0)).unwrap();
        t += 1.0 / 60.0;
        if s.over {
            return (t, v.number("score"));
        }
        assert!(t < 60.0, "{name}: an idle round never ended");
    }
}

#[test]
fn idle_rounds_end_by_themselves() {
    let (t, score) = idle_round("catch", 7);
    assert!((1.0..12.0).contains(&t) && score == 0.0, "catch: {t} s, {score}");
    let (t, _) = idle_round("flappy", 7);
    assert!(t < 5.0, "flappy: {t} s");
    let (t, _) = idle_round("runner", 7);
    assert!(t < 15.0, "runner: {t} s");
}

/// A bot keeps the cat under the lowest fish: it scores, and the same seed
/// and input give the same run, frame by frame (what replays need).
#[test]
fn a_catch_round_replays_exactly() {
    let run = |seed| {
        let p = project(&starter("catch"));
        let mut v = Vm::new(&p, seed, Mode::Round { level: None }).unwrap();
        let mut trace = Vec::new();
        let mut cat_x = 72.0;
        for f in 0..60 * 40 {
            let target = v
                .out
                .borrow()
                .things
                .iter()
                .filter_map(|t| t.as_ref().ok())
                .filter(|t| t.sprite != p.sprites.iter().position(|s| s.name == "cat").unwrap())
                .max_by(|a, b| a.y.total_cmp(&b.y))
                .map(|t| t.x + 8.0);
            let mut i = FrameInput::default();
            if f == 0 {
                (i.tap, i.tx, i.ty) = (true, cat_x, 220.0);
            }
            if let Some(x) = target {
                cat_x = x;
            }
            (i.down, i.x, i.y) = (true, cat_x, 220.0);
            let s = v.step(1.0 / 60.0, &i).unwrap();
            let out = v.out.borrow();
            trace.push((v.number("score") as i64, out.things.iter().map(|t| format!("{t:?}")).collect::<Vec<_>>().join(";")));
            if s.over {
                break;
            }
        }
        trace
    };
    let a = run(42);
    assert!(a.last().unwrap().0 > 15, "the bot should score, got {}", a.last().unwrap().0);
    assert_eq!(a, run(42));
    assert_ne!(a, run(43));
}

/// `while true do end` in a handler, even inside pcall, is a Lua error with
/// the line it was on, from the VM's own instruction budget.
#[test]
fn a_runaway_handler_is_a_lua_error_with_its_line() {
    let main = "on(\"tick\", function(dt)\n  local n = 0\n  while true do n = n + 1 end\nend)\n";
    let p = project(&with_main("catch", main));
    let mut v = Vm::new(&p, 1, Mode::Round { level: None }).unwrap();
    let err = v.step(1.0 / 60.0, &FrameInput::default()).unwrap_err();
    assert_eq!(err, "main.lua:3: sandbox: instruction budget exhausted (a loop that never ends?)");
    // pcall can't swallow it (omnilua re-raises a tripped budget); the
    // traceback then starts at the pcall's line.
    let main = "on(\"tap\", function()\n  local ok = pcall(function()\n    while true do end\n  end)\n  print(ok)\nend)\n";
    let p = project(&with_main("catch", main));
    let mut v = Vm::new(&p, 1, Mode::Round { level: None }).unwrap();
    let err = v.step(1.0 / 60.0, &FrameInput { tap: true, ..Default::default() }).unwrap_err();
    assert!(err.starts_with("main.lua:2:") && err.contains("instruction budget"), "{err}");
    // A runaway loop at load time too.
    let p = project(&with_main("catch", "local x = 0\nwhile true do x = x + 1 end\n"));
    let err = Vm::new(&p, 1, Mode::Title).err().unwrap();
    assert!(err.starts_with("main.lua:2:"), "{err}");
}

#[test]
fn errors_name_the_line_and_the_module() {
    let p = project(&with_main("catch", "on(\"tick\", function(dt)\n  local x = nil\n  return x.y\nend)\n"));
    let mut v = Vm::new(&p, 1, Mode::Round { level: None }).unwrap();
    let err = v.step(0.016, &FrameInput::default()).unwrap_err();
    assert!(err.starts_with("main.lua:3:"), "{err}");
    let err = Vm::new(&project(&with_main("catch", "x = = 1")), 1, Mode::Title).err().unwrap();
    assert!(err.starts_with("main.lua:1:"), "{err}");
    let mut f = with_main("catch", "local e = require(\"enemies\")\nassert(e.n == 3)\nassert(require(\"enemies\") == e)\n");
    f.insert("game/enemies.lua".into(), b"return { n = 3 }\n".to_vec());
    Vm::new(&project(&f), 1, Mode::Title).unwrap();
    let err = Vm::new(&project(&with_main("catch", "spawn(\"dog\")")), 1, Mode::Round { level: None }).err().unwrap();
    assert!(err.starts_with("main.lua:1: no kind named 'dog'"), "{err}");
}

#[test]
fn the_sandbox_has_no_way_out() {
    let main = r#"
        assert(os == nil and io == nil and loadstring == nil and dofile == nil and loadfile == nil)
        -- `load` is the saved-data reader, not Lua's chunk loader.
        assert(type(load) == "function" and load("x = 1") == nil)
        assert(package == nil and collectgarbage == nil)
        assert(not pcall(setmetatable, {}, { __gc = function() end }))
        assert(not pcall(setmetatable, {}, { __mode = "k" }))
        -- Redefining rawget doesn't get past the guard.
        rawget = function() end
        assert(not pcall(setmetatable, {}, { __gc = function() end }))
        assert(not pcall(setmetatable, {}, { __mode = "k" }))
        assert(tostring(0/0) == "nan" and tostring(-(0/0)) == "nan")
        -- math.random's range is checked without overflowing.
        assert(not pcall(math.random, math.mininteger, math.maxinteger))
        assert(not pcall(math.random, -2^62 // 1, 2^62 // 1))
        local x = math.random(-3, 3)
        assert(x >= -3 and x <= 3)
    "#;
    Vm::new(&project(&with_main("catch", main)), 1, Mode::Title).unwrap();
}

/// `save` and `load` keep a game's data (docs/STUDIO-API.md "Saved data"):
/// the sandbox mustn't take `load` away along with Lua's chunk loader.
#[test]
fn saved_data_round_trips() {
    let _host = maimbrain::sys::test::Host::start(1);
    let main = r#"
        save("k", 3)
        assert(load("k") == 3)
        save("name", "puffin")
        assert(load("name") == "puffin")
        save("won", true)
        assert(load("won") == true)
        assert(load("missing") == nil)
    "#;
    Vm::new(&project(&with_main("catch", main)), 1, Mode::Title).unwrap();
}

/// `pairs()` over string keys is the same order everywhere: omnilua hashes
/// strings with a fixed seed (docs/spike/studio-lua-vm.md, the probe ran the
/// same in V8 and JSC).
#[test]
fn pairs_order_is_fixed() {
    let main = r#"
        local t = { apple = 1, cherry = 2, date = 3, fig = 4, kiwi = 5, lime = 6, mango = 7, pear = 8, plum = 9, quince = 10, sloe = 11, yuzu = 12 }
        local keys = {}
        for k in pairs(t) do keys[#keys + 1] = k end
        order = table.concat(keys, ",")
    "#;
    let a = Vm::new(&project(&with_main("catch", main)), 1, Mode::Title).unwrap();
    let b = Vm::new(&project(&with_main("catch", main)), 99, Mode::Title).unwrap();
    let get = |v: &Vm| v.global_string("order");
    assert_eq!(get(&a), get(&b));
    assert_eq!(get(&a).as_deref(), Some("yuzu,apple,cherry,quince,date,sloe,kiwi,fig,mango,plum,lime,pear"));
}

/// The debug menu: the Studio tab's picks change the round, a level jump
/// made between rounds starts the next one there, and knobs and the game's
/// own items show and apply.
#[test]
fn debug_picks_apply() {
    let mut f = starter("catch");
    f.insert("maps/level2.map".into(), b"[legend]\n\"C\" = { thing = \"cat\" }\n[map]\n.........\n....C....\n".to_vec());
    let p = project(&f);
    let title = Vm::new(&p, 1, Mode::Title).unwrap();
    let menu = title.menu();
    assert_eq!(menu.kinds, ["cat", "fish", "gold"]);
    assert!(menu.knobs.iter().any(|k| k.kind == "cat" && k.knob == "speed" && k.value == 140.0));
    assert_eq!(menu.items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(), ["Gold fish now", "Gold rush"]);

    let mut v = Vm::new(&p, 1, Mode::Round { level: None }).unwrap();
    let cmds = vec![
        DebugCmd::Score(38.0),
        DebugCmd::Invincible(true),
        DebugCmd::Knob("fish".into(), "value".into(), 4.0),
        DebugCmd::Item(2, 1.0),
        DebugCmd::Spawn("gold".into()),
        DebugCmd::Hitboxes(true),
    ];
    v.step(1.0 / 60.0, &FrameInput { debug: cmds, ..Default::default() }).unwrap();
    assert_eq!(v.number("score"), 38.0);
    assert_eq!(v.global_string("gold_rush").as_deref(), Some("true"));
    assert!(!v.out.borrow().boxes.is_empty(), "hitboxes drawn");
    // Invincible: a missed fish doesn't end the round.
    for _ in 0..60 * 20 {
        assert!(!v.step(1.0 / 60.0, &FrameInput::default()).unwrap().over);
    }
    v.step(1.0 / 60.0, &FrameInput { debug: vec![DebugCmd::Level("level2".into())], ..Default::default() }).unwrap();
    assert_eq!(v.level().as_deref(), Some("level2"));

    // The engine's own state machine: a jump picked on the title starts the next round there.
    let mut st = crate::debugmenu::State::default();
    let m = st.menu(&p, &menu, None);
    let jump = m.find_label("Jump to level2").expect("a jump per map").id;
    st.pick(&p, &menu, maimbrain::debug::Pick { id: jump, value: 0.0 }, false);
    assert_eq!(st.start_level.as_deref(), Some("level2"));
    let scen = m.find_label("Gold rush").expect("scenarios come first").id;
    assert_eq!(m.sections[0].title, "Scenarios");
    st.pick(&p, &menu, maimbrain::debug::Pick { id: scen, value: 0.0 }, false);
    st.round_started();
    let cmds = st.take_commands();
    assert!(cmds.contains(&DebugCmd::Score(38.0)), "{cmds:?}");
    let mut v = Vm::new(&p, 1, Mode::Round { level: st.start_level.as_deref() }).unwrap();
    assert_eq!(v.level().as_deref(), Some("level2"));
    v.step(1.0 / 60.0, &FrameInput { debug: cmds, ..Default::default() }).unwrap();
    assert_eq!(v.number("score"), 38.0);
    assert!(m.problems().is_empty(), "{:?}", m.problems());
}

/// The whole game on a fake host, picks arriving as events: the menu the
/// host sees has the Studio tab, and a pick reaches the round.
#[test]
fn the_host_sees_the_menu_and_picks_reach_the_round() {
    use_project(starter("catch"));
    let host = Host::start(3);
    let mut g: Studio = host.init();
    let menu = host.debug_menu().expect("a menu at once");
    assert!(menu.sections.iter().any(|s| s.title == "Studio"));
    let score = menu.find_label("Score").unwrap().id;
    host.push(synth::tap(180.0, 500.0, 0.05));
    host.run(&mut g, 0.3, 1.0 / 60.0);
    host.debug_pick(score, 12.0);
    host.frame(&mut g, 1.0 / 60.0);
    let shown = host.debug_menu().unwrap();
    assert!(matches!(shown.find_label("Score").unwrap().item, maimbrain::debug::Item::Number { value: 12.0, .. }));
}

/// Keyboard and controller: the cat moves with the arrows and the stick,
/// the puffin flaps on Space and on A, and the runner jumps on Up — the same
/// moves a finger makes (games-touch-parity).
#[test]
fn keys_and_pads_play_the_starters() {
    let press = |kind: Kind, code: u32, a: f32| Event { kind: kind as u8, code, a, ..Event::default() };
    // Cat: Right held moves it right.
    use_project(starter("catch"));
    let host = Host::start(5);
    let mut g: Studio = host.init();
    host.push([press(Kind::KeyDown, key::ENTER, 0.0)]);
    host.frame(&mut g, 1.0 / 60.0);
    assert!(g.rounds.playing(), "Enter starts the round");
    let x0 = g.thing_x("cat");
    host.push([press(Kind::KeyDown, key::RIGHT, 0.0)]);
    host.run(&mut g, 0.25, 1.0 / 60.0);
    assert!(g.thing_x("cat") > x0 + 20.0, "{x0} → {}", g.thing_x("cat"));
    host.push([press(Kind::KeyUp, key::RIGHT, 0.0), press(Kind::PadAxis, pad::LX, -1.0)]);
    let x1 = g.thing_x("cat");
    host.run(&mut g, 0.25, 1.0 / 60.0);
    assert!(g.thing_x("cat") < x1 - 20.0, "the stick moves it back");

    // Puffin: A flaps (it rises).
    use_project(starter("flappy"));
    let host = Host::start(5);
    let mut g: Studio = host.init();
    host.push([press(Kind::PadButton, pad::A, 1.0)]);
    host.frame(&mut g, 1.0 / 60.0);
    assert!(g.rounds.playing(), "A starts the round");
    host.push([press(Kind::PadButton, pad::A, 0.0)]);
    host.run(&mut g, 0.4, 1.0 / 60.0);
    let y0 = g.thing_y("puffin");
    host.push([press(Kind::KeyDown, key::SPACE, 0.0)]);
    host.run(&mut g, 0.1, 1.0 / 60.0);
    assert!(g.thing_y("puffin") < y0, "Space flaps: {y0} → {}", g.thing_y("puffin"));

    // Runner: Up jumps.
    use_project(starter("runner"));
    let host = Host::start(5);
    let mut g: Studio = host.init();
    host.push([press(Kind::KeyDown, key::ENTER, 0.0)]);
    host.frame(&mut g, 1.0 / 60.0);
    host.push([press(Kind::KeyUp, key::ENTER, 0.0)]);
    host.run(&mut g, 0.3, 1.0 / 60.0);
    let y0 = g.thing_y("runner");
    host.push([press(Kind::KeyDown, key::UP, 0.0)]);
    host.run(&mut g, 0.2, 1.0 / 60.0);
    assert!(g.thing_y("runner") < y0 - 10.0, "Up jumps: {y0} → {}", g.thing_y("runner"));
}

/// A sideways game is a 256×144 canvas in a 640×360 manifest; an upright
/// one 144×256 in 360×640.
#[test]
fn the_canvas_follows_the_orientation() {
    let p = project(&starter("runner"));
    assert_eq!((p.canvas, crate::logical_size(p.orientation)), ((256, 144), (640, 360)));
    let p = project(&starter("catch"));
    assert_eq!((p.canvas, crate::logical_size(p.orientation)), ((144, 256), (360, 640)));
}

fn files(list: &[(&str, &str)]) -> Files {
    list.iter().map(|(k, v)| (k.to_string(), v.as_bytes().to_vec())).collect()
}

fn run(v: &mut Vm, frames: usize, input: &FrameInput) -> crate::vm::Step {
    let mut last = None;
    for _ in 0..frames {
        last = Some(v.step(1.0 / 60.0, input).unwrap());
    }
    last.unwrap()
}

/// Shapes drawn this frame `w` wide (the tests' kinds have no sprites).
fn shapes(v: &Vm, w: f32) -> Vec<(f32, f32)> {
    v.out.borrow().things.iter().filter_map(|t| t.as_ref().err()).filter(|s| s.w == w).map(|s| (s.x, s.y)).collect()
}

const ZOO: &str = "[legend]
\"#\" = \"rock\"
\"H\" = { thing = \"hero\" }
\"W\" = { thing = \"walker\" }
\"B\" = { thing = \"bat\" }
\"T\" = { thing = \"turret\" }
\"G\" = { thing = \"gen\" }
\"S\" = { thing = \"block\" }
[map]
....................
..B.................
....................
..........G.........
....................
....................
....................
....................
....................
....................
....................
....................
...............T....
.H.....W........S...
####################
####################
";

/// Every behaviour, on a map wider than the screen: the platformer walks
/// and jumps and the camera follows, a patroller walks, a chaser closes in,
/// a shooter's bolts fly, a spawner makes pickups.
#[test]
fn every_behaviour_does_its_job() {
    let f = files(&[
        ("studio.toml", "title = \"Zoo\"\nlives = 99\n"),
        ("tiles/rock.toml", "[tiles.solid]\nframe = 0\nsolid = true\n"),
        ("sprites/hero.toml", "behaviour = \"player.platformer\"\nsize = [10, 14]\n"),
        ("sprites/walker.toml", "behaviour = \"enemy.patrol\"\nsize = [11, 12]\n[knobs]\nspeed = 60\n"),
        ("sprites/bat.toml", "behaviour = \"enemy.chase\"\nsize = [8, 8]\n[knobs]\nspeed = 30\nsight = 400\n"),
        ("sprites/turret.toml", "behaviour = \"enemy.shooter\"\nspawns = \"bolt\"\nsize = [12, 12]\n[knobs]\nevery = 0.5\n"),
        ("sprites/bolt.toml", "behaviour = \"bullet\"\nsize = [4, 4]\n"),
        ("sprites/gen.toml", "behaviour = \"spawner\"\nspawns = \"coin\"\nsize = [5, 5]\n[knobs]\nevery = 0.4\nvy = 40\n"),
        ("sprites/coin.toml", "behaviour = \"pickup\"\nsize = [6, 6]\n"),
        ("sprites/block.toml", "behaviour = \"solid\"\nsize = [16, 16]\n"),
        ("maps/zoo.map", ZOO),
        ("game/main.lua", "shots = 0\non(\"shoot\", function() shots = shots + 1 end)\n"),
    ]);
    let p = project(&f);
    let mut v = Vm::new(&p, 3, Mode::Round { level: None }).unwrap();
    let still = FrameInput::default();
    run(&mut v, 30, &still);
    let hero = |v: &Vm| shapes(v, 10.0)[0];
    let (x0, y0) = hero(&v);
    assert!((y0 - (224.0 - 7.0)).abs() < 0.01, "standing on the rock at {y0}");
    let walker0 = shapes(&v, 11.0).iter().map(|s| s.0).fold(0.0, f32::max);
    let bat0 = shapes(&v, 8.0)[0];
    // Walking right: the camera follows across the 320-wide map.
    let s = run(&mut v, 120, &FrameInput { ax: 1.0, ..Default::default() });
    let (x1, _) = hero(&v);
    assert!(x1 > x0 + 60.0, "walked {x0} → {x1}");
    assert!(s.camx > 0.0, "the camera follows");
    assert!(v.number("shots") >= 3.0, "shots: {}", v.number("shots"));
    assert!(!shapes(&v, 6.0).is_empty(), "coins spawned");
    assert!(!shapes(&v, 4.0).is_empty(), "bolts flying");
    assert!(shapes(&v, 11.0).iter().all(|s| s.0 != walker0), "the walker walks");
    let bat1 = shapes(&v, 8.0)[0];
    assert!(bat1.0 > bat0.0 + 20.0 && bat1.1 > bat0.1 + 20.0, "the bat heads for the hero: {bat0:?} → {bat1:?}");
    // Jump: a jump press while standing lifts the hero.
    run(&mut v, 30, &still);
    let (_, ya) = hero(&v);
    v.step(1.0 / 60.0, &FrameInput { jump: true, ..Default::default() }).unwrap();
    run(&mut v, 10, &still);
    let (_, yb) = hero(&v);
    assert!(yb < ya - 10.0, "jumped {ya} → {yb}");
}

/// `player.tilt` follows the phone's tilt (the sensors build feeds it).
#[test]
fn tilt_moves_a_tilt_player() {
    let f = files(&[
        ("studio.toml", "title = \"Tilt\"\ninputs = [\"touch\", \"tilt\"]\n"),
        ("sprites/ball.toml", "behaviour = \"player.tilt\"\nsize = [8, 8]\n"),
        ("game/main.lua", "function start() spawn(\"ball\", { x = 72, y = 128 }) end\n"),
    ]);
    let mut v = Vm::new(&project(&f), 1, Mode::Round { level: None }).unwrap();
    run(&mut v, 30, &FrameInput { tilt: Some(1.0), ..Default::default() });
    let x = shapes(&v, 8.0)[0].0;
    assert!(x > 72.0 + 40.0, "tilted right: {x}");
}

/// `mb play`'s hooks: debug picks by label before the first round, events in
/// the log, and a frame's description as JSON.
#[test]
fn headless_hooks_dump_frames_trace_events_and_pick() {
    use maimbrain::sys::test::{Call, set_param};
    use_project(starter("catch"));
    let host = Host::start(9);
    set_param("studio_dump", "40,200");
    set_param("studio_trace", "1");
    set_param("studio_debug", "Gold rush;Invincible=1");
    let mut g: Studio = host.init();
    host.frame(&mut g, 1.0 / 60.0);
    host.push(synth::tap(180.0, 560.0, host.now() + 0.01));
    host.run(&mut g, 4.0, 1.0 / 60.0);
    let logs: Vec<String> = host.calls().into_iter().filter_map(|c| if let Call::Log { message, .. } = c { Some(message) } else { None }).collect();
    let frames: Vec<&String> = logs.iter().filter(|l| l.starts_with("studio-frame ")).collect();
    assert_eq!(frames.len(), 2, "{logs:?}");
    let f = &frames[1];
    assert!(f.contains("\"mode\":\"play\"") && f.contains("\"score\":38") && f.contains("assets/sprites/cat.png"), "{f}");
    assert!(f.contains("[\"cat\","), "things listed: {f}");
    // The scenario's gold rush: a catch brings a gold fish, and invincible keeps the round going.
    assert!(g.rounds.playing(), "invincible");
    let _ = logs.iter().any(|l| l.starts_with("studio-event "));
}
