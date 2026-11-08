//! The UI kit (api/ui.rs, docs/STUDIO-2.md §5 [ui]), natively: widgets as
//! nodes in the identity, buttons taking their presses (touch, Enter and
//! a pad's A), hints one at a time following what they point at, lessons'
//! capped slow motion, the countdown, the stats card holding the world,
//! the combo, and the bot following the hints.

use std::rc::Rc;

use maimbrain::scene::{Kind, SceneImage};
use maimbrain::sys::test::Host;
use maimbrain::ui::WidgetKind;

use crate::Studio2;
use crate::input::{Held, LoveEvent};
use crate::project::{Files, Project};
use crate::tests::with_main;
use crate::use_project;
use crate::vm::{Mode, Vm};

fn vm(files: &Files) -> Result<Vm, String> {
    let p = Rc::new(Project::parse(files).unwrap());
    let im: Vec<SceneImage> = p.images.iter().map(crate::scene_image).collect();
    Vm::new(&p, 7, Mode::Round, &im, &[], &[], (0.0, 0.0, 360.0, 640.0))
}

fn printed(v: &Vm) -> Vec<String> {
    v.host().draw.texts().map(str::to_string).collect()
}

fn step(v: &mut Vm, events: &[LoveEvent]) {
    v.step(1.0 / 60.0, events, &Held::default(), false, false).unwrap_or_else(|e| panic!("{e}"));
}

fn tap_at(x: f32, y: f32) -> [LoveEvent; 2] {
    [LoveEvent::TouchPressed { id: 3, x, y, pressure: 1.0 }, LoveEvent::MousePressed { x, y, button: 1, istouch: true }]
}

fn lift_at(x: f32, y: f32) -> [LoveEvent; 2] {
    [LoveEvent::TouchReleased { id: 3, x, y }, LoveEvent::MouseReleased { x, y, button: 1, istouch: true }]
}

fn error_of(main: &str) -> String {
    match vm(&with_main(main, &[])) {
        Err(e) => e,
        Ok(mut v) => v.step(1.0 / 60.0, &[], &Held::default(), false, false).expect_err("an error"),
    }
}

/// Each widget is a node of its own kind, fixed to the screen, sized in
/// the identity, and reads back what it was given; mistakes say what to do.
#[test]
fn widgets_are_nodes_that_read_back() {
    let mut v = vm(&with_main(
        r#"
        b = ui.button{ text = "PLAY", x = 180, y = 500, kind = "secondary" }
        p = ui.panel{ x = 180, y = 300, w = 300, h = 200 }
        pill = ui.pill{ text = "WAVE 3", icon = "flag", x = 180, y = 200, tone = "accent" }
        bar = ui.bar{ x = 180, y = 250, w = 200, value = 0.5 }
        ring = ui.ring{ x = 100, y = 400, r = 30, value = 2, text = "65" }
        ic = ui.icon("coin", { x = 40, y = 40, size = 24 })
        function love.draw()
          love.graphics.print(tostring(b) .. " " .. b.text .. " " .. b.kind .. " " .. b.w .. "x" .. b.h .. " " .. tostring(b.fixed) .. " " .. tostring(b.enabled))
          love.graphics.print(tostring(p) .. " " .. p.w .. " " .. tostring(pill) .. " " .. pill.icon .. " " .. pill.tone .. " " .. pill.h)
          love.graphics.print(bar.value .. " " .. ring.value .. " " .. ring.w .. " " .. ring.text .. " " .. ic.icon .. " " .. ic.w)
        end
        "#,
        &[],
    ))
    .unwrap();
    step(&mut v, &[]);
    let p = printed(&v);
    assert_eq!(p[0], "ui.button PLAY secondary 160x56 true true");
    assert_eq!(p[1], "ui.panel 300 ui.pill flag accent 32");
    assert_eq!(p[2], "0.5 1 60 65 coin 24", "a ring's value is 0–1 and its r its radius");
    let kinds: Vec<WidgetKind> = v.host().scene.ids().into_iter().filter_map(|id| v.host().scene.get(id)).filter(|n| n.kind == Kind::Widget).map(|n| n.widget.as_ref().unwrap().kind).collect();
    assert_eq!(kinds.len(), 6);
    assert!(error_of("ui.panel{ value = 1 }").contains("a ui.panel has no value (bars and rings have a value)"), "{}", error_of("ui.panel{ value = 1 }"));
    assert!(error_of("ui.icon(\"hart\")").contains("no icon \"hart\" (play, pause"));
    assert!(error_of("ui.button{ kind = \"big\" }").contains("kind is \"primary\", \"secondary\" or \"ghost\""));
    assert!(error_of("ui.button{ colour = 1 }").contains("did you mean `color`"), "{}", error_of("ui.button{ colour = 1 }"));
    assert!(error_of("ui.hint{ at = 3 }").contains("text is the hint's words"));
}

/// A button's press is the button's: the game's callbacks don't hear it,
/// a release on it taps it (its touch area at least 44 across, around a
/// small one), and a press beside it is the game's.
#[test]
fn a_button_takes_its_press_and_taps_on_release() {
    let mut v = vm(&with_main(
        r#"
        log = {}
        b = ui.button{ icon = "plus", x = 100, y = 100, w = 20, h = 20, tap = function() log[#log + 1] = "tap" end }
        function love.mousepressed(x, y) log[#log + 1] = "game " .. x end
        function love.touchpressed(id, x, y) log[#log + 1] = "touch " .. x end
        function love.draw() love.graphics.print(table.concat(log, ",")) end
        "#,
        &[],
    ))
    .unwrap();
    step(&mut v, &[]); // (the press that starts a round is never a button's)
    step(&mut v, &tap_at(120.0, 100.0));
    step(&mut v, &lift_at(120.0, 100.0));
    step(&mut v, &[]);
    assert_eq!(printed(&v), ["tap"], "20 from its centre is inside the 44 around it");
    step(&mut v, &tap_at(200.0, 300.0));
    step(&mut v, &lift_at(200.0, 300.0));
    assert_eq!(printed(&v), ["tap,touch 200,game 200"]);
    // Dragged off before letting go: no tap.
    step(&mut v, &tap_at(100.0, 100.0));
    step(&mut v, &[LoveEvent::MouseMoved { x: 250.0, y: 300.0, dx: 150.0, dy: 200.0, istouch: true }]);
    step(&mut v, &lift_at(250.0, 300.0));
    assert_eq!(printed(&v), ["tap,touch 200,game 200"]);
    // Disabled, it's the game's again.
    let mut v2 = vm(&with_main(
        r#"
        log = {}
        b = ui.button{ text = "GO", x = 100, y = 100, enabled = false, tap = function() log[#log + 1] = "tap" end }
        function love.mousepressed(x, y) log[#log + 1] = "game" end
        function love.draw() love.graphics.print(table.concat(log, ",")) end
        "#,
        &[],
    ))
    .unwrap();
    step(&mut v2, &[]);
    step(&mut v2, &tap_at(100.0, 100.0));
    step(&mut v2, &lift_at(100.0, 100.0));
    assert_eq!(printed(&v2), ["game"]);
}

/// Enter and a pad's A press the focused button (the first made, or
/// `focus = true`); the arrows and d-pad move the focus; the game doesn't
/// hear the press or its release.
#[test]
fn enter_and_a_press_the_focused_button() {
    let mut v = vm(&with_main(
        r#"
        log = {}
        left = ui.button{ text = "LEFT", x = 90, y = 500, tap = function() log[#log + 1] = "left" end }
        right = ui.button{ text = "RIGHT", x = 270, y = 500, tap = function() log[#log + 1] = "right" end }
        function love.keypressed(k) log[#log + 1] = "key " .. k end
        function love.keyreleased(k) log[#log + 1] = "up " .. k end
        function love.draw() love.graphics.print(table.concat(log, ",") .. " " .. tostring(right.focus)) end
        "#,
        &[],
    ))
    .unwrap();
    // The round's first frame: its Enter started the round, so it's the game's.
    step(&mut v, &[LoveEvent::KeyPressed("return")]);
    step(&mut v, &[LoveEvent::KeyReleased("return")]);
    assert_eq!(printed(&v), ["key return,up return false"]);
    v.host_mut().ui.focus.on = None;
    step(&mut v, &[LoveEvent::KeyPressed("return")]);
    step(&mut v, &[LoveEvent::KeyReleased("return")]);
    assert_eq!(printed(&v), ["key return,up return,left false"]);
    step(&mut v, &[LoveEvent::KeyPressed("right")]);
    step(&mut v, &[LoveEvent::GamepadPressed("a")]);
    step(&mut v, &[]);
    assert_eq!(printed(&v), ["key return,up return,left,key right,right true"], "the arrow moved the focus and still reached the game");
    let w = v.host().scene.ids().into_iter().filter_map(|id| v.host().scene.get(id)).filter_map(|n| n.widget.as_deref()).find(|w| w.text() == "RIGHT").unwrap().clone();
    assert!(w.focused && w.ring, "a key was used: the ring shows");
    step(&mut v, &tap_at(10.0, 10.0));
    let w = v.host().scene.ids().into_iter().filter_map(|id| v.host().scene.get(id)).filter_map(|n| n.widget.as_deref()).find(|w| w.text() == "RIGHT").unwrap().clone();
    assert!(!w.ring, "a finger: no ring");
}

/// Hints: one on screen at a time, the most important; each follows the
/// node it points at, goes when it's dismissed or its node does, and a
/// higher one hides a lower one until it's gone.
#[test]
fn hints_show_one_at_a_time_and_follow_their_node() {
    let mut v = vm(&with_main(
        r#"
        bell = Shape{ x = 100, y = 400, tag = "bell" }
        a = ui.hint{ text = "TAP THE BELL", at = bell }
        function love.update()
          if bell.alive then bell.x = bell.x + 1 end
          if round.time > 0.5 and not lunge then lunge = ui.hint{ text = "LET GO", priority = "threat" } end
          if round.time > 1.0 and lunge then lunge:dismiss() end
          if round.time > 1.5 and bell.alive then bell:destroy() end
        end
        function love.draw() love.graphics.print(tostring(a:showing())) end
        "#,
        &[],
    ))
    .unwrap();
    let showing = |v: &Vm| v.host().ui.hints.showing().map(|h| (h.text.clone(), h.ask, h.at)).collect::<Vec<_>>();
    for _ in 0..12 {
        step(&mut v, &[]);
    }
    let s = showing(&v);
    assert_eq!(s.len(), 1);
    assert_eq!((s[0].0.as_str(), s[0].1), ("TAP THE BELL", maimbrain::ui::Ask::Tap), "the ask is read off its words");
    assert!((s[0].2.unwrap().0 - 112.0).abs() < 1.5, "it follows the bell: {:?}", s[0].2);
    assert_eq!(printed(&v), ["true"]);
    for _ in 0..30 {
        step(&mut v, &[]);
    }
    assert_eq!(showing(&v)[0].0, "LET GO", "the threat takes the slot");
    assert_eq!(printed(&v), ["false"]);
    for _ in 0..40 {
        step(&mut v, &[]);
    }
    assert_eq!(showing(&v)[0].0, "TAP THE BELL", "back once the threat is dismissed");
    for _ in 0..40 {
        step(&mut v, &[]);
    }
    assert!(showing(&v).is_empty(), "the bell went, so did its hint");
}

/// A lesson slows the world (love.update's dt) for at most 3 s however
/// long it asks, teaches once a round, and its prompt shows until its
/// `learned` check says the move was made.
#[test]
fn a_lesson_slows_the_world_at_most_three_seconds_and_teaches_once() {
    let mut v = vm(&with_main(
        r#"
        world = 0
        function love.update(dt)
          world = world + dt
          if round.time > 0.1 then l = ui.lesson{ prompt = "HOLD to charge", slow = 0.25, hold = 10, learned = function() return charged end } end
        end
        function love.draw() love.graphics.print(string.format("%.2f %s", world, tostring(l and l:showing()))) end
        "#,
        &[],
    ))
    .unwrap();
    for _ in 0..60 {
        step(&mut v, &[]);
    }
    let world_at = |v: &Vm| printed(v)[0].split(' ').next().unwrap().parse::<f64>().unwrap();
    let w1 = world_at(&v);
    assert!(w1 < 0.45, "slowed to a quarter after 0.1 s: {w1}");
    assert_eq!(v.host().ui.hints.showing().next().map(|h| (h.text.as_str(), h.ask)), Some(("HOLD to charge", maimbrain::ui::Ask::Hold)));
    for _ in 0..(4 * 60) {
        step(&mut v, &[]);
    }
    let w2 = world_at(&v);
    assert!(w2 - w1 > 1.5, "full speed again within 3 s (+ easing), though it asked for 10: {w1} → {w2}");
    assert!(printed(&v)[0].ends_with("true"), "the prompt waits for the move");
    v.step(1.0 / 60.0, &[], &Held::default(), false, false).unwrap();
    // Learned: the prompt goes, and asking again doesn't teach it again.
    let mut v = vm(&with_main(
        r#"
        world = 0
        function love.update(dt)
          world = world + dt
          if round.time > 0.1 then l = ui.lesson{ prompt = "TAP TO JUMP", learned = function() return round.time > 0.5 end } end
        end
        function love.draw() love.graphics.print(string.format("%.2f %s", world, tostring(l and l:showing()))) end
        "#,
        &[],
    ))
    .unwrap();
    for _ in 0..120 {
        step(&mut v, &[]);
    }
    assert!(printed(&v)[0].ends_with("false"), "{:?}", printed(&v));
    let w = world_at(&v);
    for _ in 0..60 {
        step(&mut v, &[]);
    }
    assert!((world_at(&v) - w - 1.0).abs() < 0.02, "no slow motion a second time");
}

/// `ui.countdown(3, fn)`: 3, 2, 1, GO, then fn; `ui.combo`: hits, a
/// multiplier, a timeout and a reset; the HUD shows it.
#[test]
fn the_countdown_calls_back_at_go_and_combos_count() {
    let mut v = vm(&with_main(
        r#"
        ui.countdown(3, function() went = round.time end)
        c = ui.combo{ window = 1 }
        for i = 1, 5 do c:hit() end
        function love.update() if round.time > 1.5 and not reset then reset = true end end
        function love.draw() love.graphics.print(tostring(went) .. " " .. c.count .. " " .. c.multiplier .. " " .. c.best) end
        "#,
        &[],
    ))
    .unwrap();
    step(&mut v, &[]);
    assert_eq!(printed(&v), ["nil 5 2 5"]);
    for _ in 0..(4 * 60) {
        step(&mut v, &[]);
    }
    let p = printed(&v)[0].clone();
    let went: f64 = p.split(' ').next().unwrap().parse().unwrap();
    assert!((went - 3.0).abs() < 0.1, "GO at 3 s: {p}");
    assert!(p.ends_with(" 0 1 5"), "the combo timed out after its window: {p}");
    assert!(error_of("ui.countdown(0)").contains("from 1 to 10"));
    assert!(error_of("c = ui.combo() c:jump()").contains("a combo has :hit() and :reset()"));
}

/// The stats card: the world holds under it, a tap reveals it all, and
/// `done` runs once it covers the screen; then the round goes on.
#[test]
fn the_stats_card_holds_the_world_and_calls_done_when_covered() {
    let main = r#"
        world = 0
        function love.update(dt)
          world = world + dt
          if round.time > 0.3 and not shown then
            shown = true
            ui.stats{ title = "Wave 1", rows = { { "Coins", 12 }, { "Accuracy", 0.9, "percent" } }, rank = true, done = function() done_at = round.time end }
          end
          round.score = math.floor(world * 10)
        end
    "#;
    let host = Host::start(4);
    use_project(with_main(main, &[]));
    let mut g: Studio2 = host.init();
    host.tap(180.0, 320.0);
    host.run(&mut g, 0.5, 1.0 / 60.0);
    let world = |g: &Studio2| g.round.as_ref().unwrap().host().last_score;
    let held = world(&g);
    host.run(&mut g, 1.0, 1.0 / 60.0);
    assert_eq!(world(&g), held, "held under the card");
    let card = g.round.as_ref().unwrap().host().ui.stats.as_ref().and_then(|s| s.card.clone()).expect("the card is up");
    assert!(!card.all_shown());
    host.tap(180.0, 320.0);
    host.run(&mut g, 0.2, 1.0 / 60.0);
    assert!(g.round.as_ref().unwrap().host().ui.stats.as_ref().unwrap().card.as_ref().unwrap().all_shown(), "a tap reveals it all");
    host.run(&mut g, 4.0, 1.0 / 60.0);
    assert!(g.round.as_ref().unwrap().host().ui.stats.is_none(), "gone after its hold and wipe");
    assert!(world(&g) > held, "the world goes on");
    assert!(g.error.is_none(), "{:?}", g.error);
}

/// The bot with no bot.lua and no quick-path player does what the hints
/// say (the kit's `bot::Literal`'s rules): it taps the bell its hint points
/// at, holds on HOLD and lets go on LET GO. Without the hints it does
/// nothing at all.
#[test]
fn the_bot_follows_the_hints() {
    let main = r#"
        bell = Shape{ shape = "circle", x = 120, y = 420, radius = 26 }
        rung, held, presses = 0, 0, 0
        function love.mousepressed(x, y)
          presses = presses + 1
          if (x - bell.x) ^ 2 + (y - bell.y) ^ 2 < 50 * 50 then rung = rung + 1 end
        end
        function love.update(dt)
          bell.x = 120 + math.sin(round.time) * 60
          if love.mouse.isDown(1) then held = held + dt end
          if HINTS then
            if round.time < 4 then tapH = ui.hint{ text = "TAP THE BELL", at = bell }
            elseif round.time < 6 then
              if tapH then tapH:dismiss() tapH = nil end
              holdH = ui.hint{ text = "HOLD" }
            elseif round.time < 7 then
              if holdH then holdH:dismiss() holdH = nil end
              goH = ui.hint{ text = "LET GO" }
            elseif goH then goH:dismiss() goH = nil end
          end
          if round.time > 7.5 and love.mouse.isDown(1) then stuck = true end
          round.score = rung + (held > 1 and 1000 or 0) + (stuck and 100000 or 0)
          if round.time > 8 then round.over("DONE", "time", true) end
        end
    "#;
    let with = |hints: bool| {
        let mut f = with_main(&format!("HINTS = {hints}\n{main}"), &[]);
        // No policy of its own: the catch starter's bot.lua goes.
        f.retain(|k, _| !k.starts_with("game/bot.lua"));
        f
    };
    let (line, _) = crate::bot::native_round(&with(true), "casual", 3, 20.0, &[]).unwrap();
    let score: f64 = field(&line, "score").parse().unwrap();
    let presses: u64 = field(&line, "presses").parse().unwrap();
    assert!(score >= 1005.0 && score < 100000.0, "it rang the bell the hint pointed at again and again (score - 1000), held on HOLD (1000) and let go on LET GO (not 100000): {line}");
    assert!(presses >= 6, "{line}");
    let (line, _) = crate::bot::native_round(&with(false), "casual", 3, 20.0, &[]).unwrap();
    assert_eq!(field(&line, "score"), "0", "no hints, nothing to do: {line}");
}

fn field<'a>(j: &'a str, k: &str) -> &'a str {
    let pat = format!("\"{k}\":");
    let i = j.find(&pat).unwrap_or_else(|| panic!("no {k} in {j}")) + pat.len();
    let rest = &j[i..];
    if let Some(s) = rest.strip_prefix('"') { &s[..s.find('"').unwrap()] } else { &rest[..rest.find([',', '}']).unwrap()] }
}

/// Bell Keeper (examples/bell-keeper) has no bot.lua and no quick-path
/// player: the bot plays it from its hints alone (the bell's RING IT, the
/// storm's lesson and HOLD THE ROPE, LET GO), and skill matters: a good
/// player outlasts a first-timer, and each is on the kit's target.
#[test]
fn bell_keeper_is_played_from_its_hints() {
    let f = crate::tests::example("bell-keeper");
    let secs = |preset: &str, cap: f64| -> Vec<f64> {
        let mut v: Vec<f64> = (1..=5).map(|s| field(&crate::bot::native_round(&f, preset, s, cap, &[]).unwrap().0, "seconds").parse().unwrap()).collect();
        v.sort_by(f64::total_cmp);
        v
    };
    let (first, good) = (secs("first-timer", 45.0), secs("good", 180.0));
    let (fm, gm) = (first[2], good[2]);
    assert!((20.0..=45.0).contains(&fm), "a first-timer's median, {fm} s: {first:?}");
    assert!((60.0..180.0).contains(&gm), "a good player's median, {gm} s: {good:?}");
    let (line, _) = crate::bot::native_round(&f, "good", 2, 180.0, &[]).unwrap();
    assert!(field(&line, "presses").parse::<u64>().unwrap() > 40, "{line}");
}

/// A finger on a button is the button's for the held state too: neither
/// `love.mouse.isDown` nor `love.touch.getTouches` sees it while the
/// button has it, from the frame it takes the press (Decided 67).
#[test]
fn a_finger_on_a_button_isnt_held_for_the_game() {
    let mut v = vm(&with_main(
        r#"
        b = ui.button{ text = "GO", x = 100, y = 100 }
        function love.draw() love.graphics.print(tostring(love.mouse.isDown(1)) .. " " .. #love.touch.getTouches()) end
        "#,
        &[],
    ))
    .unwrap();
    let held = |x: f32, y: f32| {
        let mut h = Held::default();
        h.mouse[0] = true;
        (h.mx, h.my) = (x, y);
        h.touches.push((3, x, y));
        h
    };
    v.step(1.0 / 60.0, &[], &Held::default(), false, false).unwrap();
    v.step(1.0 / 60.0, &tap_at(100.0, 100.0), &held(100.0, 100.0), false, false).unwrap();
    assert_eq!(printed(&v), ["false 0"], "the frame the button takes it");
    v.step(1.0 / 60.0, &[], &held(100.0, 100.0), false, false).unwrap();
    assert_eq!(printed(&v), ["false 0"], "while it holds it");
    v.step(1.0 / 60.0, &lift_at(100.0, 100.0), &Held::default(), false, false).unwrap();
    // A finger beside it is the game's.
    v.step(1.0 / 60.0, &tap_at(250.0, 400.0), &held(250.0, 400.0), false, false).unwrap();
    assert_eq!(printed(&v), ["true 1"]);
}

/// A finger already down when a stats card comes up lifts for the game
/// when it lifts, though the card takes the touch: it isn't held forever.
#[test]
fn a_finger_down_when_the_stats_card_comes_lifts_for_the_game() {
    use maimbrain::input::{Kind as K, synth};
    let main = r#"
        function love.update()
          if round.time > 0.4 and not shown then shown = true ui.stats{ title = "Wave 1", rows = { { "Coins", 1 } } } end
        end
    "#;
    let host = Host::start(4);
    use_project(with_main(main, &[]));
    let mut g: Studio2 = host.init();
    host.tap(180.0, 320.0);
    host.run(&mut g, 0.2, 1.0 / 60.0);
    let t = host.now();
    host.push([synth::touch(K::TouchDown, 100.0, 500.0, t + 0.01)]);
    host.run(&mut g, 0.5, 1.0 / 60.0);
    assert!(g.round.as_ref().unwrap().host().ui.stats_up(), "the card is up");
    assert!(g.input.held.mouse[0], "the finger is down");
    let t = host.now();
    host.push([synth::touch(K::TouchUp, 100.0, 500.0, t + 0.01)]);
    host.run(&mut g, 0.2, 1.0 / 60.0);
    assert!(!g.input.held.mouse[0] && g.input.held.touches.is_empty(), "lifted for the game too");
    let h = g.round.as_ref().unwrap().host();
    assert!(!h.held.mouse[0] && h.held.touches.is_empty(), "and the round sees it lifted");
}
