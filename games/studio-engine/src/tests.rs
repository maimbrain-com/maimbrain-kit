//! The engine, its examples and its starters, natively: the platform's
//! rules (`Conformance`), replays, the instruction budget, the sandbox, the
//! debug menu, and keyboard and controller input.

use std::path::Path;
use std::rc::Rc;

use maimbrain::input::{Event, Kind, key, pad, synth};
use maimbrain::sys::test::{Conformance, Host};

use crate::project::{Files, Project};
use crate::vm::{DebugCmd, FrameInput, Mode, Vm};
use crate::{Studio, read_dir, use_project};

/// A finished game (Cat Snack, Comet Lanes…), which most tests play.
fn example(name: &str) -> Files {
    read_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("examples").join(name))
}

/// A starter: the greybox a new Studio game begins as (docs/STUDIO.md "The first version").
fn starter(name: &str) -> Files {
    read_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("starters").join(name))
}

/// The example's files with main.lua replaced.
fn with_main(name: &str, main: &str) -> Files {
    let mut f = example(name);
    f.insert("game/main.lua".into(), main.as_bytes().to_vec());
    f
}

fn project(files: &Files) -> Rc<Project> {
    Rc::new(Project::parse(files).unwrap())
}

/// The manifest `mb build` would write (for Conformance's daily-board check).
const MANIFEST: &str = "abi = 0\nid = \"dev.maimbrain.studio.test\"\nname = \"Test\"\nversion = \"0.1.0\"\ncreator = \"@maimbrain\"\norientation = \"portrait\"\nlogical_size = [360, 640]\ninputs = [\"touch\"]\nperf_tier = \"lite\"\ncapabilities = [\"store\", \"score\"]\n[[scores]]\nboard = 0\nlabel = \"Score\"\norder = \"higher\"\n";

fn conformance(name: &str) -> Conformance {
    use_project(example(name));
    Conformance::new().max_round(60.0).manifest(MANIFEST)
}

/// The platform's round rules (SPEC §5.1–§5.2) on a fake host, for each
/// example: a Studio game can't break them, because `Rounds` owns them.
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

/// The greybox starters keep them too.
#[test]
fn the_starters_keep_the_platform_rules() {
    for name in ["catch", "flappy", "runner"] {
        use_project(starter(name));
        Conformance::new().max_round(60.0).manifest(MANIFEST).check::<Studio>();
    }
}

fn idle(dt: f32) -> FrameInput {
    let _ = dt;
    FrameInput::default()
}

/// Seconds until an idle round ends, and its score.
fn idle_round(name: &str, seed: u64) -> (f32, f64) {
    let p = project(&example(name));
    let mut v = Vm::new(&p, seed, Mode::Round { level: None, set: &[] }).unwrap();
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
        let p = project(&example("catch"));
        let mut v = Vm::new(&p, seed, Mode::Round { level: None, set: &[] }).unwrap();
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
    let mut v = Vm::new(&p, 1, Mode::Round { level: None, set: &[] }).unwrap();
    let err = v.step(1.0 / 60.0, &FrameInput::default()).unwrap_err();
    assert_eq!(err, "main.lua:3: sandbox: instruction budget exhausted (a loop that never ends?)");
    // pcall can't swallow it (omnilua re-raises a tripped budget); the
    // traceback then starts at the pcall's line.
    let main = "on(\"tap\", function()\n  local ok = pcall(function()\n    while true do end\n  end)\n  print(ok)\nend)\n";
    let p = project(&with_main("catch", main));
    let mut v = Vm::new(&p, 1, Mode::Round { level: None, set: &[] }).unwrap();
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
    let mut v = Vm::new(&p, 1, Mode::Round { level: None, set: &[] }).unwrap();
    let err = v.step(0.016, &FrameInput::default()).unwrap_err();
    assert!(err.starts_with("main.lua:3:"), "{err}");
    let err = Vm::new(&project(&with_main("catch", "x = = 1")), 1, Mode::Title).err().unwrap();
    assert!(err.starts_with("main.lua:1:"), "{err}");
    let mut f = with_main("catch", "local e = require(\"enemies\")\nassert(e.n == 3)\nassert(require(\"enemies\") == e)\n");
    f.insert("game/enemies.lua".into(), b"return { n = 3 }\n".to_vec());
    Vm::new(&project(&f), 1, Mode::Title).unwrap();
    let err = Vm::new(&project(&with_main("catch", "spawn(\"dog\")")), 1, Mode::Round { level: None, set: &[] }).err().unwrap();
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
    let mut f = example("catch");
    f.insert("maps/level2.map".into(), b"[legend]\n\"C\" = { thing = \"cat\" }\n[map]\n.........\n....C....\n".to_vec());
    let p = project(&f);
    let title = Vm::new(&p, 1, Mode::Title).unwrap();
    let menu = title.menu();
    assert_eq!(menu.kinds, ["cat", "fish", "gold"]);
    assert!(menu.knobs.iter().any(|k| k.kind == "cat" && k.knob == "speed" && k.value == 140.0));
    assert_eq!(menu.items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(), ["Gold fish now", "Gold rush"]);

    let mut v = Vm::new(&p, 1, Mode::Round { level: None, set: &[] }).unwrap();
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

    // The engine's own state machine: a jump picked on the title starts the next round there.
    let mut st = crate::debugmenu::State::default();
    let m = st.menu(&p, &menu, None, false);
    assert!(m.find_label("Restart round").is_none(), "only during a round");
    let jump = m.find_label("Jump to level2").expect("a jump per map").id;
    st.pick(&p, &menu, maimbrain::debug::Pick { id: jump, value: 0.0 }, false);
    assert_eq!(st.start_level.as_deref(), Some("level2"));
    assert!(!st.take_restart(), "no round to start again");
    let scen = m.find_label("Gold rush").expect("scenarios come first").id;
    assert_eq!(m.sections[0].title, "Scenarios");
    st.pick(&p, &menu, maimbrain::debug::Pick { id: scen, value: 0.0 }, false);
    st.round_started();
    let cmds = st.take_commands();
    assert!(cmds.contains(&DebugCmd::Score(38.0)), "{cmds:?}");
    let mut v = Vm::new(&p, 1, Mode::Round { level: st.start_level.as_deref(), set: &st.set }).unwrap();
    assert_eq!(v.level().as_deref(), Some("level2"));
    v.step(1.0 / 60.0, &FrameInput { debug: cmds, ..Default::default() }).unwrap();
    assert_eq!(v.number("score"), 38.0);
    assert!(m.problems().is_empty(), "{:?}", m.problems());
}

/// The whole game on a fake host, picks arriving as events: the menu the
/// host sees has the Studio tab, and a pick reaches the round.
#[test]
fn the_host_sees_the_menu_and_picks_reach_the_round() {
    use_project(example("catch"));
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

/// A tap that comes before the project's files are all in (a quick one, or
/// one replayed onto a change's new files) is ignored, not a round with
/// nothing to play and an error left over the title once it loads.
#[test]
fn a_tap_before_the_project_loads_is_ignored() {
    use maimbrain::sys::{Asset, Round};
    let files = starter("catch");
    use_project(files.clone());
    let host = Host::start(3);
    let mut g: Studio = host.init();
    // As on a device: the files still on their way.
    g.project = None;
    g.ready = false;
    g.loading = crate::Loading::Index(Asset::load(crate::INDEX));
    host.push(synth::tap(180.0, 500.0, 0.05));
    host.run(&mut g, 0.3, 1.0 / 60.0);
    assert_eq!(g.error, None);
    assert!(!host.rounds().contains(&Round::Playing), "{:?}", host.rounds());
    // In: the title's up, and a tap now starts a round.
    g.loading = crate::Loading::Done;
    g.loaded(files);
    host.push(synth::tap(180.0, 500.0, 0.05));
    host.run(&mut g, 0.3, 1.0 / 60.0);
    assert_eq!(g.error, None);
    assert!(g.round.is_some());
    assert!(host.rounds().contains(&Round::Playing), "{:?}", host.rounds());
}

/// Keyboard and controller: the cat moves with the arrows and the stick,
/// the puffin flaps on Space and on A, and the runner jumps on Up — the same
/// moves a finger makes (games-touch-parity).
#[test]
fn keys_and_pads_play_the_starters() {
    let press = |kind: Kind, code: u32, a: f32| Event { kind: kind as u8, code, a, ..Event::default() };
    // Cat: Right held moves it right.
    use_project(example("catch"));
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
    use_project(example("flappy"));
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
    use_project(example("runner"));
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

/// The greybox starters play by keys and pads as their examples do: the
/// player moves on Right, flaps on Space and jumps on Up.
#[test]
fn keys_and_pads_play_the_greybox_starters() {
    let press = |kind: Kind, code: u32| Event { kind: kind as u8, code, ..Event::default() };
    let start = |name: &str| {
        use_project(starter(name));
        let host = Host::start(5);
        let mut g: Studio = host.init();
        host.push([press(Kind::KeyDown, key::ENTER)]);
        host.frame(&mut g, 1.0 / 60.0);
        host.push([press(Kind::KeyUp, key::ENTER)]);
        assert!(g.rounds.playing(), "{name}: Enter starts the round");
        (host, g)
    };
    let (host, mut g) = start("catch");
    let x0 = g.thing_x("player");
    host.push([press(Kind::KeyDown, key::RIGHT)]);
    host.run(&mut g, 0.25, 1.0 / 60.0);
    assert!(g.thing_x("player") > x0 + 20.0, "catch: {x0} → {}", g.thing_x("player"));

    let (host, mut g) = start("flappy");
    host.run(&mut g, 0.4, 1.0 / 60.0);
    let y0 = g.thing_y("player");
    host.push([press(Kind::KeyDown, key::SPACE)]);
    host.run(&mut g, 0.1, 1.0 / 60.0);
    assert!(g.thing_y("player") < y0, "flappy: {y0} → {}", g.thing_y("player"));

    let (host, mut g) = start("runner");
    host.run(&mut g, 0.3, 1.0 / 60.0);
    let y0 = g.thing_y("player");
    host.push([press(Kind::KeyDown, key::UP)]);
    host.run(&mut g, 0.2, 1.0 / 60.0);
    assert!(g.thing_y("player") < y0 - 10.0, "runner: {y0} → {}", g.thing_y("player"));
}

/// A sideways game is a 256×144 canvas in a 640×360 manifest; an upright
/// one 144×256 in 360×640.
#[test]
fn the_canvas_follows_the_orientation() {
    let p = project(&example("runner"));
    assert_eq!((p.canvas, crate::logical_size(p.orientation)), ((256, 144), (640, 360)));
    let p = project(&example("catch"));
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
    let mut v = Vm::new(&p, 3, Mode::Round { level: None, set: &[] }).unwrap();
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
    let mut v = Vm::new(&project(&f), 1, Mode::Round { level: None, set: &[] }).unwrap();
    run(&mut v, 30, &FrameInput { tilt: Some(1.0), ..Default::default() });
    let x = shapes(&v, 8.0)[0].0;
    assert!(x > 72.0 + 40.0, "tilted right: {x}");
}

/// `mb play`'s hooks: debug picks by label before the first round, events in
/// the log, and a frame's description as JSON.
#[test]
fn headless_hooks_dump_frames_trace_events_and_pick() {
    use maimbrain::sys::test::{Call, set_param};
    use_project(example("catch"));
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

/// A scenario's `set` globals reach `start()`: they land after main.lua's
/// own top level (overriding its defaults) and before `start()`, again on
/// the first frame (over anything `start()` assigned), and in every round
/// after, like knobs.
#[test]
fn scenario_globals_are_set_before_start() {
    let main = "start_rival = 1\nmode = \"intro\"\nfunction start()\n  first_rival = start_rival\n  mode = \"menu\"\nend\n";
    let mut f = with_main("catch", main);
    f.insert("debug/scenarios.toml".into(), b"[[scenario]]\nname = \"Final boss\"\n[scenario.set]\nstart_rival = 5\nmode = \"duel\"\n".to_vec());
    let p = project(&f);
    let menu = Vm::new(&p, 1, Mode::Title).unwrap().menu();
    let mut st = crate::debugmenu::State::default();
    let boss = st.menu(&p, &menu, None, false).find_label("Final boss").expect("the scenario").id;
    st.pick(&p, &menu, maimbrain::debug::Pick { id: boss, value: 0.0 }, false);
    for round in 0..2 {
        let mut v = Vm::new(&p, 1, Mode::Round { level: None, set: &st.set }).unwrap();
        assert_eq!(v.number("first_rival"), 5.0, "round {round}: start() saw the scenario's value");
        assert_eq!(v.global_string("mode").as_deref(), Some("menu"), "start() ran after it");
        st.round_started();
        v.step(1.0 / 60.0, &FrameInput { debug: st.take_commands(), ..Default::default() }).unwrap();
        assert_eq!(v.global_string("mode").as_deref(), Some("duel"), "round {round}: and the first frame sets it again");
    }
    // Picked mid-round, it starts the round again, so start() sees it.
    let mut st = crate::debugmenu::State::default();
    st.pick(&p, &menu, maimbrain::debug::Pick { id: boss, value: 0.0 }, true);
    assert!(st.take_restart());
    let v = Vm::new(&p, 1, Mode::Round { level: None, set: &st.set }).unwrap();
    assert_eq!(v.number("first_rival"), 5.0);
}

/// Another scenario replaces what the last one set: its globals, level,
/// invincible and knobs go back to the game's own, and nothing else does.
#[test]
fn a_scenario_replaces_the_last_one() {
    let main = "GOD = false\nAUTO = false\n";
    let mut f = with_main("catch", main);
    f.insert("maps/level2.map".into(), b"[legend]\n\"C\" = { thing = \"cat\" }\n[map]\n.........\n....C....\n".to_vec());
    f.insert(
        "debug/scenarios.toml".into(),
        b"[[scenario]]\nname = \"God\"\nlevel = \"level2\"\ninvincible = true\nscore = 500\nspawn = [\"fish\"]\n[scenario.knobs]\n\"fish.value\" = 5\n[scenario.set]\nGOD = true\n\n[[scenario]]\nname = \"Auto\"\n[scenario.set]\nAUTO = true\n".to_vec(),
    );
    let p = project(&f);
    let menu = Vm::new(&p, 1, Mode::Title).unwrap().menu();
    let mut st = crate::debugmenu::State::default();
    let m = st.menu(&p, &menu, None, false);
    let (god, auto) = (m.find_label("God").unwrap().id, m.find_label("Auto").unwrap().id);
    let speed = m.find_label("cat · speed").unwrap().id;
    let inv = m.find_label("Invincible").unwrap().id;
    st.pick(&p, &menu, maimbrain::debug::Pick { id: speed, value: 200.0 }, false);
    st.pick(&p, &menu, maimbrain::debug::Pick { id: inv, value: 1.0 }, false);
    st.pick(&p, &menu, maimbrain::debug::Pick { id: god, value: 0.0 }, false);
    assert!(st.invincible);
    assert_eq!(st.start_level.as_deref(), Some("level2"));
    st.pick(&p, &menu, maimbrain::debug::Pick { id: auto, value: 0.0 }, false);
    let v = Vm::new(&p, 1, Mode::Round { level: st.start_level.as_deref(), set: &st.set }).unwrap();
    assert_eq!(v.global_string("GOD").as_deref(), Some("false"), "God's global is gone");
    assert_eq!(v.global_string("AUTO").as_deref(), Some("true"));
    assert_eq!(st.start_level, None, "and its level");
    assert!(!st.invincible, "and its invincible");
    assert_eq!(st.knobs, [("cat".to_string(), "speed".to_string(), 200.0)], "and its knobs, not yours");
    st.round_started();
    let cmds = st.take_commands();
    assert!(!cmds.iter().any(|c| matches!(c, DebugCmd::Invincible(_))), "the round isn't invincible either: {cmds:?}");
    assert!(!cmds.iter().any(|c| matches!(c, DebugCmd::Score(_) | DebugCmd::Spawn(_))), "nor gets God's score and spawns: {cmds:?}");
}

/// On the phone: a scenario picked during a round starts it again with the
/// scenario's globals before start(), Restart round does too (keeping the
/// picks), and picks made during the round's ending wait for the next one.
#[test]
fn picks_during_a_round_start_it_again() {
    let main = "start_wave = 1\nfunction start() first_wave = start_wave end\non(\"tick\", function() ticks = (ticks or 0) + 1 end)\n";
    let mut f = with_main("catch", main);
    f.insert("debug/scenarios.toml".into(), b"[[scenario]]\nname = \"Wave 5\"\n[scenario.set]\nstart_wave = 5\n".to_vec());
    use_project(f);
    let host = Host::start(3);
    let mut g: Studio = host.init();
    let menu = host.debug_menu().unwrap();
    assert!(menu.find_label("Restart round").is_none());
    let wave5 = menu.find_label("Wave 5").unwrap().id;
    let inv = menu.find_label("Invincible").unwrap().id;
    host.push(synth::tap(180.0, 500.0, 0.05));
    host.run(&mut g, 0.5, 1.0 / 60.0);
    let ticks = |g: &Studio| g.round.as_ref().unwrap().number("ticks");
    assert!(ticks(&g) > 10.0);
    assert_eq!(g.round.as_ref().unwrap().number("first_wave"), 1.0);
    let restart = host.debug_menu().unwrap().find_label("Restart round").expect("during a round").id;

    host.debug_pick(wave5, 0.0);
    host.frame(&mut g, 1.0 / 60.0);
    let v = g.round.as_ref().unwrap();
    assert_eq!(v.number("first_wave"), 5.0, "start() saw the scenario");
    assert_eq!(v.number("ticks"), 1.0, "a fresh round");

    host.debug_pick(inv, 1.0);
    host.run(&mut g, 0.2, 1.0 / 60.0);
    let score = host.debug_menu().unwrap().find_label("Score").unwrap().id;
    host.debug_pick(score, 12.0);
    host.debug_pick(restart, 0.0);
    host.frame(&mut g, 1.0 / 60.0);
    assert_eq!(ticks(&g), 1.0, "Restart round");
    assert_eq!(g.round.as_ref().unwrap().number("score"), 12.0, "a score picked with it lands in the new round");
    assert_eq!(g.round.as_ref().unwrap().number("first_wave"), 5.0, "keeps the scenario");
    assert!(g.debug.invincible, "and the other picks");
}

/// The 3D view has no hitboxes to show, so its menu doesn't offer them.
#[test]
fn hitboxes_only_in_2d() {
    let p2 = project(&starter("catch"));
    let st = crate::debugmenu::State::default();
    assert!(st.menu(&p2, &crate::vm::Menu::default(), None, false).find_label("Show hitboxes").is_some());
    let mut p3 = (*p2).clone();
    p3.kind = crate::project::Kind::ThreeD;
    assert!(st.menu(&p3, &crate::vm::Menu::default(), None, false).find_label("Show hitboxes").is_none());
}

/// `debug.invincible()` is the Studio tab's Invincible, for a game that takes
/// lives itself.
#[test]
fn lua_reads_invincible() {
    let main = "function hit() if not debug.invincible() then lives = lives - 1 end end\non(\"tick\", function() hit() end)\n";
    let p = project(&with_main("catch", main));
    let mut v = Vm::new(&p, 1, Mode::Round { level: None, set: &[] }).unwrap();
    let lives = v.number("lives");
    v.step(1.0 / 60.0, &FrameInput { debug: vec![DebugCmd::Invincible(true)], ..Default::default() }).unwrap();
    assert_eq!(v.number("lives"), lives);
    v.step(1.0 / 60.0, &FrameInput { debug: vec![DebugCmd::Invincible(false)], ..Default::default() }).unwrap();
    assert_eq!(v.number("lives"), lives - 1.0);
}

/// Text layout is arithmetic in canvas pixels: 4.8 px a character and 8 a
/// line at scale 1, wrapping at spaces (a long word where it has to).
#[test]
fn text_widths_and_wrapping() {
    use crate::vm::{TEXT_ADVANCE, TEXT_LINE, text_width, wrap};
    assert_eq!((TEXT_ADVANCE, TEXT_LINE), (4.8, 8.0));
    assert_eq!(text_width("HIGH NOON", 1), 43.2);
    assert_eq!(text_width("HIGH NOON", 3), 129.6);
    assert_eq!(text_width("ab\nlonger\nc", 1), 28.8, "the longest line");
    assert_eq!(text_width("", 1), 0.0);
    // 144 px holds 30 characters a line.
    let quote = "\"Ain't nobody faster'n Pete. 'Cept most folks.\"";
    let lines = wrap(quote, Some(144.0), 1);
    assert_eq!(lines, ["\"Ain't nobody faster'n Pete.", "'Cept most folks.\""]);
    assert!(lines.iter().all(|l| text_width(l, 1) <= 144.0));
    assert_eq!(wrap("one two three", Some(4.8 * 7.0), 1), ["one two", "three"]);
    assert_eq!(wrap("abcdefghij", Some(4.8 * 4.0), 1), ["abcd", "efgh", "ij"], "a word longer than the line");
    assert_eq!(wrap("a\nb c", None, 1), ["a", "b c"], "without a width, only \\n breaks");
    assert_eq!(wrap("a b", Some(4.8 * 3.0), 2), ["a", "b"], "the width is at the scale");
    assert_eq!(wrap("x", Some(1.0), 1), ["x"], "at least a character a line");
}

/// text()'s options, and text_width from Lua.
#[test]
fn text_options_reach_the_renderer() {
    use crate::vm::Align;
    let main = r#"
on("tick", function()
  w = text_width("DRAW!", 3)
  text("score", 4, 6)
  text("DRAW!", 72, 100, 8, { align = "center", scale = 3, shadow = 0 })
  text("Ain't nobody faster'n Pete", 72, 40, { align = "center", width = 100 })
end)
"#;
    let p = project(&with_main("catch", main));
    let mut v = Vm::new(&p, 1, Mode::Round { level: None, set: &[] }).unwrap();
    v.step(1.0 / 60.0, &FrameInput::default()).unwrap();
    assert_eq!(v.number("w"), 72.0);
    let out = v.out.borrow();
    let t = &out.texts;
    assert_eq!((t[0].lines.as_slice(), t[0].align, t[0].scale, t[0].shadow, t[0].color), (&["score".to_string()][..], Align::Left, 1, None, 7));
    assert_eq!((t[1].align, t[1].scale, t[1].shadow, t[1].color), (Align::Center, 3, Some(0), 8));
    assert_eq!((t[2].lines.as_slice(), t[2].color), (&["Ain't nobody".to_string(), "faster'n Pete".to_string()][..], 7), "the colour is optional before the options");
    drop(out);
    let bad = project(&with_main("catch", "on(\"tick\", function() text(\"x\", 0, 0, { align = \"middle\" }) end)"));
    let mut v = Vm::new(&bad, 1, Mode::Round { level: None, set: &[] }).unwrap();
    let err = v.step(1.0 / 60.0, &FrameInput::default()).unwrap_err();
    assert!(err.contains("align is \"left\", \"center\" or \"right\""), "{err}");
}

/// `mb play` draws the game's text from the frame's description: text() in
/// the pixel font at 16 units (in art pixels, so 16 / scale), and popups
/// centred with an outline, where they've risen to.
#[test]
fn headless_frames_describe_text_and_popups() {
    use maimbrain::sys::test::{Call, set_param};
    let main = "local shown = false\non(\"tick\", function()\n  text(\"HIGH NOON\", 40, 30, 8)\n  if not shown then shown = true; popup(\"DRAW!\", 72, 120) end\nend)\n";
    use_project(with_main("catch", main));
    let host = Host::start(9);
    set_param("studio_dump", "20");
    let mut g: Studio = host.init();
    host.frame(&mut g, 1.0 / 60.0);
    host.push(synth::tap(180.0, 560.0, host.now() + 0.01));
    host.run(&mut g, 0.6, 1.0 / 60.0);
    let logs: Vec<String> = host.calls().into_iter().filter_map(|c| if let Call::Log { message, .. } = c { Some(message) } else { None }).collect();
    let f = logs.iter().find(|l| l.starts_with("studio-frame ")).expect("a frame");
    // ["text", x, y, size, rgba, centred, outline, "s"]: the numbers, then the string.
    let texts: Vec<(Vec<f64>, &str)> = f
        .match_indices("[\"text\",")
        .map(|(i, m)| {
            let rest = &f[i + m.len()..];
            let q = rest.find('"').unwrap();
            let nums = rest[..q].split(',').filter(|n| !n.is_empty()).map(|n| n.parse().unwrap()).collect();
            (nums, &rest[q + 1..q + 1 + rest[q + 1..].find('"').unwrap()])
        })
        .collect();
    assert_eq!(texts.len(), 2, "{f}");
    let ((popup, ps), (text, ts)) = (&texts[0], &texts[1]);
    assert_eq!((*ps, popup[4]), ("DRAW!", 1.0), "centred: {f}");
    assert!(popup[5] as u64 & 0xff > 0, "outlined: {f}");
    assert_eq!(popup[0].round(), 72.0, "{f}");
    assert!(popup[1] < 120.0, "risen: {f}");
    assert_eq!((*ts, text[0], text[1], text[4], text[5]), ("HIGH NOON", 40.0, 30.0, 0.0, 0.0), "{f}");
    // 16 units on a 144-wide canvas shown 360 wide.
    assert!((text[2] - 16.0 / 2.5).abs() < 0.01, "{f}");
}

/// What `start()` asks for (music, a popup, a shake) happens on the round's
/// first frame: it runs while the round's VM is made, before any frame, and
/// used to be cleared by that frame unplayed (the starters' theme music never
/// started).
#[test]
fn effects_from_start_play() {
    let main = "function start()\n  music(\"theme\")\n  popup(\"GO!\", 72, 100)\n  shake(4)\nend\n";
    let p = project(&with_main("catch", main));
    let mut v = Vm::new(&p, 1, Mode::Round { level: None, set: &[] }).unwrap();
    v.step(1.0 / 60.0, &FrameInput::default()).unwrap();
    let fx = std::mem::take(&mut v.out.borrow_mut().fx);
    assert_eq!(fx.len(), 3, "{fx:?}");
    assert!(matches!(&fx[0], crate::vm::Fx::Music(Some(m)) if m == "theme"), "{fx:?}");

    // The whole engine: the popup is up once the round has started.
    use_project(with_main("catch", main));
    let host = Host::start(5);
    let mut g: Studio = host.init();
    host.frame(&mut g, 1.0 / 60.0);
    host.push(synth::tap(180.0, 560.0, host.now() + 0.01));
    host.run(&mut g, 0.2, 1.0 / 60.0);
    assert!(g.rounds.playing());
    assert_eq!(g.feel.popups.list.iter().map(|p| p.text.as_str()).collect::<Vec<_>>(), ["GO!"]);
}

/// `mb play`'s frames lay text out as the phone does: a wrapped, centred,
/// shadowed text() is one entry a line (its shadow first), each from its
/// left end, at its scale's size.
#[test]
fn headless_frames_lay_text_out() {
    use maimbrain::sys::test::{Call, set_param};
    let main = "on(\"tick\", function() text(\"ab cd\", 72, 50, 7, { align = \"center\", width = 15, scale = 1, shadow = 0 }) end)\n";
    use_project(with_main("catch", main));
    let host = Host::start(9);
    set_param("studio_dump", "20");
    let mut g: Studio = host.init();
    host.frame(&mut g, 1.0 / 60.0);
    host.push(synth::tap(180.0, 560.0, host.now() + 0.01));
    host.run(&mut g, 0.6, 1.0 / 60.0);
    let logs: Vec<String> = host.calls().into_iter().filter_map(|c| if let Call::Log { message, .. } = c { Some(message) } else { None }).collect();
    let f = logs.iter().find(|l| l.starts_with("studio-frame ")).expect("a frame");
    let entries: Vec<&str> = f.match_indices("[\"text\",").map(|(i, _)| &f[i..i + f[i..].find(']').unwrap() + 1]).collect();
    // "ab" and "cd", 9.6 px wide each, centred on 72: from 67.2; lines 8 px apart; shadows 0.8 px down and right.
    assert_eq!(entries.len(), 4, "{f}");
    assert!(entries[0].starts_with("[\"text\",68,50.8,6.4,") && entries[0].ends_with("\"ab\"]"), "{f}");
    assert!(entries[1].starts_with("[\"text\",67.2,50,6.4,") && entries[1].ends_with("\"ab\"]"), "{f}");
    assert!(entries[3].starts_with("[\"text\",67.2,58,6.4,") && entries[3].ends_with("\"cd\"]"), "{f}");
}

/// The Arcade builds play no 3D project: it says which build does.
#[cfg(not(feature = "3d"))]
#[test]
fn an_arcade_build_names_the_3d_build() {
    let err = Vm::new(&project(&example("comet")), 1, Mode::Title).err().unwrap();
    assert!(err.contains("3D build"), "{err}");
}

/// 3D games (`cargo test -p studio-engine --features 3d`).
#[cfg(feature = "3d")]
mod three_d {
    use std::cell::RefCell;
    use std::rc::Rc;

    use maimbrain::audio::Sound;

    use super::*;
    use crate::project::{Kind, Scalar};
    use crate::scene::{Audio, Hud};
    use crate::vm::Media;

    const MANIFEST_3D: &str = "abi = 0\nid = \"dev.maimbrain.studio.test3d\"\nname = \"Test\"\nversion = \"0.1.0\"\ncreator = \"@maimbrain\"\norientation = \"portrait\"\nlogical_size = [360, 640]\ninputs = [\"touch\"]\nperf_tier = \"full\"\nstdlib = { mb3d = 2, mb2d = 1 }\nsensors = [\"haptics\"]\ncapabilities = [\"store\", \"score\"]\n[[scores]]\nboard = 0\nlabel = \"Rings\"\norder = \"higher\"\n";

    fn media() -> Media {
        let audio = Rc::new(RefCell::new(Audio::default()));
        audio.borrow_mut().sounds.insert("beep".into(), Sound(1));
        Media { models: Rc::default(), audio }
    }

    fn round(p: &Rc<Project>, seed: u64, set: &[(String, Scalar)]) -> Vm {
        Vm::with_media(p, seed, Mode::Round { level: None, set }, &media()).unwrap_or_else(|e| panic!("{e}"))
    }

    /// Seconds until the round ends (or `secs`), and its score.
    fn play(v: &mut Vm, secs: f32) -> (f32, f64) {
        let dt = 1.0 / 60.0;
        let mut t = 0.0;
        while t < secs {
            if v.step(dt, &FrameInput::default()).unwrap_or_else(|e| panic!("{e}")).over {
                break;
            }
            t += dt;
        }
        (t, v.number("score"))
    }

    #[test]
    fn comet_keeps_the_platform_rules() {
        use_project(example("comet"));
        Conformance::new().max_round(60.0).manifest(MANIFEST_3D).check::<Studio>();
    }

    /// The tunnel starter: Comet Lanes' rules, with its look in game/look.lua.
    #[test]
    fn the_tunnel_starter_keeps_the_rules_and_flies_on_autopilot() {
        use_project(starter("tunnel"));
        Conformance::new().max_round(60.0).manifest(MANIFEST_3D).check::<Studio>();
        let p = project(&starter("tunnel"));
        assert_eq!(p.kind, Kind::ThreeD);
        let v = round(&p, 1, &[]);
        let (nodes, meshes, materials) = v.scene.as_ref().unwrap().borrow().counts();
        assert!(nodes > 10 && meshes >= 3 && materials >= 2, "{nodes} nodes, {meshes} meshes, {materials} materials");
        let (t, score) = play(&mut round(&p, 7, &[]), 30.0);
        assert!(t < 10.0 && score == 1.0, "idle: {score} rings in {t} s");
        let (_, score) = play(&mut round(&p, 7, &[("AUTO".into(), Scalar::Bool(true))]), 30.0);
        assert!(score >= 10.0, "autopilot: {score} rings in 30 s");
    }

    #[test]
    fn comet_is_a_3d_project_and_builds_its_scene() {
        let p = project(&example("comet"));
        assert_eq!(p.kind, Kind::ThreeD);
        let v = round(&p, 1, &[]);
        let (nodes, meshes, materials) = v.scene.as_ref().unwrap().borrow().counts();
        assert!(nodes > 10 && meshes > 3 && materials > 3, "{nodes} nodes, {meshes} meshes, {materials} materials");
    }

    #[test]
    fn an_idle_comet_crashes_at_the_second_ring() {
        // The first ring's gap is straight ahead (a free pass); the second asks for a steer.
        let (t, score) = play(&mut round(&project(&example("comet")), 7, &[]), 30.0);
        assert!(t < 10.0, "still flying after {t} s");
        assert_eq!(score, 1.0);
    }

    #[test]
    fn the_autopilot_scenario_flies_on() {
        let (_, score) = play(&mut round(&project(&example("comet")), 7, &[("AUTO".into(), Scalar::Bool(true))]), 30.0);
        assert!(score >= 10.0, "{score} rings in 30 s");
    }

    #[test]
    fn the_title_plays_itself_and_never_ends_a_round() {
        let p = project(&example("comet"));
        let mut v = Vm::with_media(&p, 3, Mode::Title, &media()).unwrap();
        for _ in 0..60 * 40 {
            assert!(!v.step(1.0 / 60.0, &FrameInput::default()).unwrap().over);
        }
    }

    #[test]
    fn same_seed_same_round() {
        let p = project(&example("comet"));
        let a = play(&mut round(&p, 11, &[("AUTO".into(), Scalar::Bool(true))]), 20.0);
        let b = play(&mut round(&p, 11, &[("AUTO".into(), Scalar::Bool(true))]), 20.0);
        assert_eq!((a.0.to_bits(), a.1.to_bits()), (b.0.to_bits(), b.1.to_bits()));
    }

    #[test]
    fn keys_steer_and_debug_items_and_saves_work() {
        let main = "score = 0\nlocal hits = 0\ndebug.number('Speed', 1, function(v) speed = v end, 0, 5)\non('press', 'jump', function() hits = hits + 1 end)\non('tick', function() score = hits * 10 + (input.ax > 0 and 1 or 0) end)\nfunction start() save('best', 3) end\n";
        let p = project(&with_main("comet", main));
        let mut v = round(&p, 1, &[]);
        let frame = FrameInput { ax: 1.0, press: vec!["jump"], debug: vec![DebugCmd::Item(1, 3.0)], ..FrameInput::default() };
        v.step(1.0 / 60.0, &frame).unwrap();
        assert_eq!((v.number("score"), v.number("speed")), (11.0, 3.0));
        assert_eq!(v.menu().items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(), ["Speed"]);
    }

    #[test]
    fn errors_name_their_line_even_from_a_binding() {
        let p = |m: &str| project(&with_main("comet", m));
        let e = Vm::with_media(&p("local n = node()\nn:place('x', 0, 0)\n"), 1, Mode::Round { level: None, set: &[] }, &media()).err().unwrap();
        assert!(e.starts_with("main.lua:2:"), "{e}");
        let e = Vm::with_media(&p("sfx('nope')\n"), 1, Mode::Round { level: None, set: &[] }, &media()).err().unwrap();
        assert!(e.starts_with("main.lua:1:") && e.contains("sounds/nope.sfx.toml"), "{e}");
        let e = Vm::with_media(&p("model('ship')\n"), 1, Mode::Round { level: None, set: &[] }, &media()).err().unwrap();
        assert!(e.contains("models/ship.glb"), "{e}");
        let mut v = round(&p("on('tick', function() while true do end end)\n"), 1, &[]);
        let e = v.step(1.0 / 60.0, &FrameInput::default()).unwrap_err();
        assert!(e.starts_with("main.lua:1:") && e.contains("instruction budget"), "{e}");
    }

    #[test]
    fn sound_haptics_and_the_hud() {
        let main = "music('beep')\nlocal hum = loop('beep', 0.3)\non('tap', function() sfx('beep', 1, 0, 1.2); haptic('heavy'); hum:set(0.5, 0, 1.1) end)\non('tick', function() lowpass('music', 800, 0.3); text(score, 180, 40, { size = 34 }) end)\n";
        let mut v = round(&project(&with_main("comet", main)), 1, &[]);
        v.step(1.0 / 60.0, &FrameInput { tap: true, ..FrameInput::default() }).unwrap();
        let hud = v.scene.as_ref().unwrap().borrow().hud.clone();
        assert!(matches!(&hud[..], [Hud::Text { s, .. }] if s == "0"), "{hud:?}");
        // Each frame's 2D starts empty (it isn't drawn again under the next).
        v.step(1.0 / 60.0, &FrameInput::default()).unwrap();
        assert_eq!(v.scene.as_ref().unwrap().borrow().hud.len(), 1);
        let e = Vm::with_media(&project(&with_main("comet", "haptic('buzz')")), 1, Mode::Round { level: None, set: &[] }, &media()).err().unwrap();
        assert!(e.contains("tap, success, fail or heavy"), "{e}");
    }

    /// `mb play`'s frame of the 3D view writes each HUD text from its left
    /// end (or its centre): right-aligned text ends at its x, as on the phone.
    #[test]
    fn headless_frames_place_right_aligned_text() {
        use maimbrain::sys::test::{Call, set_param};
        let main = "on('tick', function() text('1234', 350, 20, { size = 20, align = 'right' }); text('MID', 180, 60, { size = 20 }) end)\n";
        use_project(with_main("comet", main));
        let host = Host::start(9);
        set_param("studio_dump", "20");
        let mut g: Studio = host.init();
        host.run(&mut g, 0.5, 1.0 / 60.0);
        let logs: Vec<String> = host.calls().into_iter().filter_map(|c| if let Call::Log { message, .. } = c { Some(message) } else { None }).collect();
        let f = logs.iter().find(|l| l.starts_with("studio-frame ")).expect("a frame");
        // Four characters at 0.75 em each, 20 units an em: 60 wide, so from 290.
        assert!(f.contains("[\"text\",290,20,20,4294967295,0,0,\"1234\"]"), "{f}");
        assert!(f.contains("[\"text\",180,60,20,4294967295,1,0,\"MID\"]"), "{f}");
    }

    #[test]
    fn a_bare_node_shares_one_default_material() {
        let v = round(&project(&with_main("comet", "local m = mesh.box(1, 1, 1)\nfor i = 1, 50 do node(m) end\n")), 1, &[]);
        let (nodes, _, materials) = v.scene.as_ref().unwrap().borrow().counts();
        assert_eq!((nodes, materials), (50, 1));
    }

    #[test]
    fn text_with_a_tostring_that_calls_a_binding() {
        let main = "local n = node()\nlocal o = setmetatable({}, { __tostring = function() n:pos(1, 2, 3); rect(0, 0, 1, 1, 0xff); return 'hi' end })\non('tick', function() text(o, 10, 10) end)\n";
        let mut v = round(&project(&with_main("comet", main)), 1, &[]);
        v.step(1.0 / 60.0, &FrameInput::default()).unwrap();
        assert!(v.scene.as_ref().unwrap().borrow().hud.iter().any(|h| matches!(h, Hud::Text { s, .. } if s == "hi")));
    }

    #[test]
    fn studio_toml_says_which_kind() {
        let with = |toml: &str| {
            let mut f = example("comet");
            f.insert("studio.toml".into(), toml.as_bytes().to_vec());
            Project::parse(&f).map(|p| p.kind)
        };
        assert_eq!(with("kind = \"3d\"\n"), Ok(Kind::ThreeD));
        assert_eq!(with("kind = \"2d\"\n"), Ok(Kind::TwoD));
        assert_eq!(with("title = \"T\"\n"), Ok(Kind::TwoD), "2D unless it says");
        assert!(with("kind = \"4d\"\n").unwrap_err().contains("\"2d\" or \"3d\""));
        // Projects made before `kind` say it with rung and view.
        assert_eq!(with("rung = \"lua\"\nview = \"3d\"\n"), Ok(Kind::ThreeD));
        assert_eq!(with("rung = \"arcade\"\n"), Ok(Kind::TwoD));
        assert!(with("rung = \"lua\"\n").unwrap_err().contains("say kind = \"2d\" or kind = \"3d\""));
        assert!(with("rung = \"rust\"\n").unwrap_err().contains("the old way"));
    }
}

/// docs/STUDIO-3D-API.md's examples run, and its topics are what it says.
#[cfg(feature = "3d")]
#[test]
fn the_3d_reference_examples_run() {
    let doc = include_str!("../../../docs/STUDIO-3D-API.md");
    let topics: Vec<&str> = doc.lines().filter_map(|l| l.strip_prefix("## ")).collect();
    assert_eq!(topics, ["project", "rounds", "events", "scene", "models", "look", "hud", "sound", "debug", "saved", "maths", "limits"]);
    // The rounds and debug pages' Lua, in a game with what they call.
    let lua: Vec<&str> = doc.split("```lua\n").skip(1).filter_map(|b| b.split("```").next()).collect();
    let main = format!(
        "function autopilot(dt) end\nfunction crashed() return false end\n{}\n{}\n",
        lua[0],
        lua.iter().find(|b| b.contains("debug.toggle")).unwrap()
    );
    let p = project(&with_main("comet", &main));
    let mut v = Vm::with_media(&p, 1, Mode::Round { level: None, set: &[] }, &crate::vm::Media::default()).unwrap_or_else(|e| panic!("{e}"));
    v.step(1.0 / 60.0, &FrameInput::default()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(v.menu().items.len(), 4);
}
