//! The difficulty bot (bot.rs, autoplay.rs, api/act.rs), natively: rounds
//! played by `bot::native_round` as `mb difficulty`'s native runner plays
//! them, with the starters' `game/bot.lua`, the quick path's default bots,
//! and the report line's cause, cap and read-backs.

use crate::bot::native_round;
use crate::project::Files;
use crate::tests::{compile, example, starter, with_main};

/// A round's `studio-bot` line, parsed a little: (seconds, score, cause, capped, presses, the line).
struct Line {
    seconds: f64,
    score: f64,
    cause: String,
    capped: bool,
    won: bool,
    presses: u64,
    reads: bool,
    raw: String,
}

fn field<'a>(j: &'a str, k: &str) -> &'a str {
    let pat = format!("\"{k}\":");
    let i = j.find(&pat).unwrap_or_else(|| panic!("no {k} in {j}")) + pat.len();
    let rest = &j[i..];
    if let Some(s) = rest.strip_prefix('"') { &s[..s.find('"').unwrap()] } else { &rest[..rest.find([',', '}']).unwrap()] }
}

fn round(files: &Files, preset: &str, seed: u64, max: f64) -> Line {
    let (raw, _) = native_round(files, preset, seed, max, &[]).unwrap();
    Line {
        seconds: field(&raw, "seconds").parse().unwrap(),
        score: field(&raw, "score").parse().unwrap(),
        cause: field(&raw, "cause").to_string(),
        capped: field(&raw, "capped") == "true",
        won: field(&raw, "won") == "true",
        presses: field(&raw, "presses").parse().unwrap(),
        reads: field(&raw, "reads") == "true",
        raw,
    }
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

/// Lantern Path has no bot.lua: the platformer's default bot holds along
/// the path, jumps its gaps and thorns, and lights lanterns; a good player
/// wins (lights them all) more often than a first-timer.
#[test]
fn lantern_path_plays_itself_with_the_default_bot() {
    let f = example("lantern-path");
    let good: Vec<Line> = (1..=6).map(|s| round(&f, "good", s, 120.0)).collect();
    for l in &good {
        assert!(l.presses >= 2 && l.seconds > 8.0, "{}", l.raw);
        assert!(!l.reads, "Lantern Path reads nothing back: {}", l.raw);
    }
    assert!(good.iter().any(|l| l.raw.contains("overlap fox lantern") && l.score >= 1.0), "a lantern lit: {}", good[0].raw);
    let first: Vec<Line> = (1..=6).map(|s| round(&f, "first-timer", s, 45.0)).collect();
    let wins = |v: &[Line]| v.iter().filter(|l| l.won).count();
    assert!(wins(&good) > wins(&first), "good won {} of 6, first-timer {}", wins(&good), wins(&first));
}

/// Puffin Hop (engine 1.0's example, moved up) has no bot.lua: the flappy
/// default flies its cliffs. Its sea, a hazard row across the whole view,
/// is a floor, not the next thing in the way (the bot used to aim at the
/// sky over it and hit every first cliff, so skill didn't matter); skill
/// matters, as it did for engine 1.0's bot (spike S5).
#[test]
fn puffin_hop_plays_itself_with_the_default_bot() {
    let f = crate::tests::read(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/puffin-hop"));
    let expert: Vec<Line> = (1..=6).map(|s| round(&f, "expert", s, 45.0)).collect();
    let first: Vec<Line> = (1..=6).map(|s| round(&f, "first-timer", s, 45.0)).collect();
    let (e, ft) = (median(expert.iter().map(|l| l.seconds).collect()), median(first.iter().map(|l| l.seconds).collect()));
    assert!(e > 5.0, "an expert gets past the first cliffs: {e} s, {}", expert[0].raw);
    assert!(e > ft, "expert {e} s, first-timer {ft} s");
    assert!(expert.iter().any(|l| l.score >= 3.0), "{}", expert[0].raw);
}

/// Neon Dash (engine 1.0's runner example, moved up) has no bot.lua: the
/// runner default jumps its crates at each crate's own speed, as the world
/// moves after speedUp (it used to time crates by the faster drones and by
/// vx without speedUp, jumping early, then late: every preset died in 7 s).
#[test]
fn neon_dash_plays_itself_with_the_default_bot() {
    let f = crate::tests::read(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/neon-dash"));
    let expert: Vec<Line> = (1..=6).map(|s| round(&f, "expert", s, 45.0)).collect();
    let first: Vec<Line> = (1..=6).map(|s| round(&f, "first-timer", s, 45.0)).collect();
    let (e, ft) = (median(expert.iter().map(|l| l.seconds).collect()), median(first.iter().map(|l| l.seconds).collect()));
    assert!(e > 15.0, "an expert outruns the first speed-ups: {e} s, {}", expert[0].raw);
    assert!(e > ft, "expert {e} s, first-timer {ft} s");
}

/// The same seed and preset play the same round.
#[test]
fn a_bot_round_replays() {
    let f = example("lantern-path");
    assert_eq!(round(&f, "casual", 3, 30.0).raw, round(&f, "casual", 3, 30.0).raw);
}

/// Each 2D starter's bot.lua plays it through `act`: it presses, scores,
/// and a good player outlasts a first-timer.
#[test]
fn the_starters_bots_play_them() {
    for name in ["catch", "flappy", "runner"] {
        let f = starter(name);
        assert!(f.contains_key("game/bot.lua"), "{name} has a bot");
        let good: Vec<Line> = (1..=3).map(|s| round(&f, "good", s, 60.0)).collect();
        let first: Vec<Line> = (1..=3).map(|s| round(&f, "first-timer", s, 45.0)).collect();
        for l in good.iter().chain(&first) {
            assert!(l.presses > 0 && !l.raw.contains("\"error\""), "{name}: {}", l.raw);
        }
        assert!(good.iter().any(|l| l.score > 0.0), "{name}: {}", good[0].raw);
        let (g, ft) = (median(good.iter().map(|l| l.seconds).collect()), median(first.iter().map(|l| l.seconds).collect()));
        assert!(g >= ft, "{name}: good {g} s, first-timer {ft} s");
    }
}

/// A quick-path flappy: walls come in pairs; the default bot flaps at the gaps.
const FLAPPY: &str = r#"
kind("bird", { w = 24, h = 24, behaviour = "player.flappy" })
kind("wall", { w = 40, h = 400, behaviours = { "hazard" } })
kind("floor", { w = 400, h = 20, behaviours = { "hazard" } })
function love.load()
  spawn("bird", { x = 100, y = 320 })
  spawn("floor", { x = 180, y = 640 })
  timer.every(1.8, function()
    local mid = 180 + love.math.random() * 280
    spawn("wall", { x = 420, y = mid - 90 - 200, vx = -120 })
    spawn("wall", { x = 420, y = mid + 90 + 200, vx = -120 })
  end)
end
on("hurt", function() round.over("CRASHED", "wall") end)
on("leave", "bird", function() round.over("GONE", "left") end)
"#;

/// A quick-path catch: coins and rocks fall; the paddle catches coins and dodges rocks.
const CATCH: &str = r#"
kind("paddle", { w = 70, h = 16, behaviour = "player.paddle" })
kind("coin", { w = 20, h = 20, behaviours = { "pickup" } })
kind("rock", { w = 26, h = 26, behaviours = { "hazard" } })
function love.load()
  spawn("paddle", { x = 180, y = 580 })
  timer.every(0.9, function()
    spawn(love.math.random() < 0.75 and "coin" or "rock", { x = 30 + love.math.random() * 300, y = -10, vy = 180 })
  end)
end
on("hurt", function() round.over("HIT", "rock") end)
on("leave", "coin", function(c) if c.y > 600 then round.over("MISSED", "missed") end end)
"#;

/// A quick-path runner: the world comes at it; crates to jump.
const RUNNER: &str = r#"
kind("runner", { w = 24, h = 32, behaviour = "player.runner" })
kind("crate", { w = 28, h = 28, behaviours = { "hazard" } })
kind("ground", { w = 400, h = 40, behaviours = { "solid" } })
function love.load()
  spawn("ground", { x = 180, y = 620 })
  spawn("runner", { x = 90, y = 580 })
  timer.every(1.3, function() spawn("crate", { x = 400, y = 586, vx = -200 }) end)
end
on("hurt", function() round.over("CRASHED", "crate") end)
"#;

fn quick_game(main: &str) -> Files {
    let mut f = with_main(main, &[]);
    f.remove("game/bot.lua");
    f.remove("game/bot.luac");
    f
}

/// The quick path's default bots play their verbs better than nobody, and
/// a good player better than a first-timer.
#[test]
fn the_default_bots_play_flappy_drag_and_runner() {
    for (name, main) in [("flappy", FLAPPY), ("catch", CATCH), ("runner", RUNNER)] {
        let f = quick_game(main);
        let good: Vec<Line> = (1..=5).map(|s| round(&f, "good", s, 40.0)).collect();
        let first: Vec<Line> = (1..=5).map(|s| round(&f, "first-timer", s, 40.0)).collect();
        let g = median(good.iter().map(|l| l.seconds).collect());
        let ft = median(first.iter().map(|l| l.seconds).collect());
        assert!(good.iter().all(|l| l.presses > 0), "{name}: {}", good[0].raw);
        assert!(g > 8.0, "{name}: the good bot plays it (p50 {g} s): {}", good[0].raw);
        assert!(g >= ft, "{name}: good {g} s, first-timer {ft} s");
    }
}

/// The cap ends a round the game doesn't, and says so; a game that ends by
/// winning says that; a bot.lua error is the report's.
#[test]
fn the_report_line_has_the_cap_the_win_and_errors() {
    let f = with_main("function love.update() end", &[("game/bot.lua", "function bot(act) act.tap(180, 320, { at = 0.5 }) end")]);
    let l = round(&f, "good", 1, 3.0);
    assert!(l.capped && l.cause == "time" && (l.seconds - 3.0).abs() < 0.05, "{}", l.raw);
    assert!(l.presses >= 2, "{}", l.raw);
    let f = with_main("function love.update() if round.time > 2 then round.over(\"YES\", \"lit\", true) end end", &[]);
    let l = round(&f, "casual", 1, 10.0);
    assert!(l.won && l.cause == "lit" && !l.capped, "{}", l.raw);
    let f = with_main("function love.update() end", &[("game/bot.lua", "function bot(act) act.tap(\"left\") end")]);
    let (raw, _) = native_round(&f, "good", 1, 5.0, &[]).unwrap();
    assert!(raw.contains("\"error\":\"game/bot.lua:1: act.tap(x, y"), "{raw}");
}

/// A game whose logic measures text says it reads back from the host.
#[test]
fn reading_text_metrics_is_a_read_back() {
    let f = with_main("local w = 0\nfunction love.update() w = love.graphics.getFont():getWidth(\"hi\") end", &[]);
    assert!(round(&f, "good", 1, 2.0).reads);
    let f = with_main("function love.update() end", &[]);
    assert!(!round(&f, "good", 1, 2.0).reads);
    // Measured only to lay out love.draw's text: the native runner plays it.
    let f = with_main("function love.draw() local f = love.graphics.getFont() love.graphics.print(\"hi\", 180 - f:getWidth(\"hi\") / 2, 20) end", &[]);
    assert!(!round(&f, "good", 1, 2.0).reads);
}

/// At 0 lives the round ends at once (Decided 28); with `round.endDelay`
/// the game's own ending plays first, and `round.over` still ends it sooner.
#[test]
fn zero_lives_waits_out_the_end_delay() {
    let lose = "function love.update() if round.time > 1 and round.lives > 0 then round.lives = 0 end end";
    let at_once = round(&with_main(lose, &[]), "good", 1, 10.0);
    assert!(!at_once.capped && at_once.cause == "lives" && at_once.seconds < 1.2, "{}", at_once.raw);
    let delayed = round(&with_main(&format!("round.endDelay = 1.5\n{lose}"), &[]), "good", 1, 10.0);
    assert!(!delayed.capped && delayed.cause == "lives", "{}", delayed.raw);
    assert!((delayed.seconds - at_once.seconds - 1.5).abs() < 0.05, "{} vs {}", delayed.raw, at_once.raw);
    let over = round(&with_main(&format!("round.endDelay = 5\n{lose}\nfunction love.draw() if round.lives == 0 and round.time > 1.5 then round.over(\"SPLAT\", \"tipped\") end end"), &[]), "good", 1, 10.0);
    assert!(over.cause == "tipped" && over.seconds < 1.7, "{}", over.raw);
    // At most 10 s; a string is a mistake the game hears about.
    let long = round(&with_main(&format!("round.endDelay = 60\n{lose}"), &[]), "good", 1, 30.0);
    assert!(!long.capped && (long.seconds - at_once.seconds - 10.0).abs() < 0.05, "{}", long.raw);
    let bad = round(&with_main("round.endDelay = \"soon\"\nfunction love.update() end", &[]), "good", 1, 3.0);
    assert!(bad.raw.contains("round.endDelay is seconds"), "{}", bad.raw);
}

/// `act` is the policy's only: not a global, and bot.lua loads only in bot runs.
#[test]
fn bot_lua_loads_only_in_bot_runs() {
    use crate::{Studio2, use_project};
    use maimbrain::sys::test::Host;
    let mut f = with_main("function love.update() if bot then round.score = 7 end end", &[("game/bot.lua", "function bot(act) end")]);
    compile(&mut f);
    let host = Host::start(1);
    use_project(f.clone());
    let mut g: Studio2 = host.init();
    host.run(&mut g, 0.5, 1.0 / 60.0);
    host.tap(180.0, 320.0);
    host.run(&mut g, 1.0, 1.0 / 60.0);
    assert_eq!(g.round.as_ref().unwrap().host().last_score, 0.0, "no bot.lua outside a bot run");
    let l = round(&f, "good", 1, 2.0);
    assert_eq!(l.score, 7.0, "{}", l.raw);
}

/// `BOT_GAME=lantern-path|flappy|catch|runner|starter:<name> cargo test -p studio-engine2 --lib print_rounds -- --ignored --nocapture`
#[test]
#[ignore]
fn print_rounds() {
    let game = std::env::var("BOT_GAME").unwrap_or("lantern-path".into());
    let f = match game.as_str() {
        "flappy" => quick_game(FLAPPY),
        "catch" => quick_game(CATCH),
        "runner" => quick_game(RUNNER),
        g if g.starts_with("starter:") => starter(&g[8..]),
        "lantern-path" => example("lantern-path"),
        g => crate::tests::read(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples").join(g)),
    };
    let max: f64 = std::env::var("BOT_MAX").ok().and_then(|m| m.parse().ok()).unwrap_or(60.0);
    // One round traced: BOT_PRESET=good BOT_SEED=2.
    if let (Ok(preset), Some(seed)) = (std::env::var("BOT_PRESET"), std::env::var("BOT_SEED").ok().and_then(|s| s.parse().ok())) {
        let (raw, events) = native_round(&f, &preset, seed, max, &[("studio_trace".into(), "1".into())]).unwrap();
        for e in events {
            println!("{e}");
        }
        println!("{raw}");
        return;
    }
    for preset in ["first-timer", "casual", "good", "expert"] {
        for seed in 1..=4 {
            let l = round(&f, preset, seed, max);
            let ev: Vec<&str> = l.raw.split("\"events\":").nth(1).map(|e| &e[..e.len().min(300)]).into_iter().collect();
            println!("{preset:12} {seed} {:6.1}s score {:4} {:10} presses {:3} {:?}", l.seconds, l.score, l.cause, l.presses, ev);
        }
    }
}
