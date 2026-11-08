//! Voice and dialogue (api/voice.rs), natively: The Toll's lines and
//! portraits, a scene's timing (the same as V8's), answers, skipping a
//! cutscene, barks, and the errors.

use std::path::Path;
use std::rc::Rc;

use maimbrain::input::synth;
use maimbrain::sys::test::Host;

use crate::input::Held;
use crate::project::{Files, Project};
use crate::tests::{compile, read};
use crate::vm::{Mode, Vm};
use crate::{Studio2, use_project};

fn toll() -> Files {
    read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/the-toll"))
}

/// The Toll with its main.lua replaced.
fn toll_with(main: &str) -> Files {
    let mut f = toll();
    f.insert("game/main.lua".into(), main.as_bytes().to_vec());
    compile(&mut f);
    f
}

fn vm(files: &Files) -> Result<Vm, String> {
    let p = Rc::new(Project::parse(files).unwrap());
    let im: Vec<_> = p.images.iter().map(crate::scene_image).collect();
    Vm::new(&p, 7, Mode::Round, &im, &[], &[], (0.0, 0.0, 360.0, 640.0))
}

/// Frames of 1/60 s for `secs`, game time (the dialogue's clock) with them.
fn run(v: &mut Vm, secs: f32) {
    for _ in 0..(secs * 60.0).round() as usize {
        maimbrain::sys::test::advance(1.0 / 60.0);
        v.step(1.0 / 60.0, &[], &Held::default(), false, false).unwrap_or_else(|e| panic!("{e}"));
    }
}

fn printed(v: &Vm) -> Vec<String> {
    v.host().draw.texts().map(str::to_string).collect()
}

#[test]
fn the_toll_reads_its_lines_and_portraits() {
    let p = Project::parse(&toll()).unwrap();
    assert_eq!(p.voices.len(), 12);
    let halt = &p.voices[p.voice("gruk", "halt").unwrap()];
    assert_eq!((halt.text.as_str(), halt.emotion.as_str(), halt.ogg), ("Oi. Nobody crosses Gruk's bridge for free.", "smug", true));
    assert!((halt.lips.dur() - 2.82).abs() < 1e-6, "timed by its lip track: {}", halt.lips.dur());
    let jeer = &p.voices[p.voice("gruk", "jeer2").unwrap()];
    assert_eq!(jeer.tag, "wrong");
    let gruk = p.images.iter().find(|i| i.name == "portraits/gruk").unwrap();
    assert_eq!(gruk.portrait.as_deref().unwrap(), ["neutral", "smug", "annoyed", "laughing", "bored", "blink"]);
    assert_eq!(gruk.frame, (192, 192));
}

/// A line with no lip track is timed by its sound's length (the packer's),
/// else by its text: never by when its audio decodes.
#[test]
fn a_line_without_a_lip_track_is_timed_by_its_length_or_its_text() {
    let mut f = toll();
    f.remove("voices/gruk/halt.lips.txt");
    f.remove("voices/pip/please.lips.txt");
    f.insert("packed.toml".into(), b"[voices.\"gruk/halt\"]\nsecs = 2.5\n".to_vec());
    let p = Project::parse(&f).unwrap();
    assert!((p.voices[p.voice("gruk", "halt").unwrap()].lips.dur() - 2.5).abs() < 1e-6);
    let please = &p.voices[p.voice("pip", "please").unwrap()].lips;
    assert!((please.dur() - (please.text().chars().count() as f32 / 30.0 + 0.3)).abs() < 1e-5);
}

/// The trace a scene leaves: each callback's round.time. Printed in V8 by
/// `mb play` (its log) when 4a was built; a change here moves every scene.
const TRACE: &str = r#"
local t = {}
local function mark(what)
  local s = string.format("%s %.4f", what, round.time)
  t[#t + 1] = s
  print(s)
end
function love.load()
  cast("gruk", { portrait = "gruk", side = "right", name = "GRUK" })
  cast("pip", { portrait = "pip" })
  on("cue", "mid", function() mark("cue") end)
  cutscene{
    { "letterbox", 0.1 },
    { "camera", 200, 470, 1.3, 1, "inOutSine" },
    { "say", "gruk", "halt" },
    { "cue", "mid" },
    { "say", "pip", "please", "nervous" },
    { "wait", 0.5 },
    done = function()
      mark("cut")
      dialogue{ { "gruk", "riddle1" }, { "gruk", "right1" }, advance = "auto",
        done = function(c)
          mark("dialogue " .. tostring(c))
          say("gruk", "idle1").done:connect(function() mark("said") end)
        end }
    end,
  }
end
function love.draw() love.graphics.print(table.concat(t, ","), 8, 8) end
"#;

#[test]
fn a_scene_lands_on_the_same_frames_as_in_v8() {
    maimbrain::sys::test::reset();
    let mut v = vm(&toll_with(TRACE)).unwrap();
    run(&mut v, 22.0);
    assert_eq!(printed(&v), ["cue 4.4500,cut 9.0000,dialogue nil 17.7333,said 20.2833"]);
}

/// The same session plays the same: lines, answers and the scene.
#[test]
fn the_toll_plays_through_and_replays() {
    let play = || {
        let host = Host::start(5);
        use_project(toll());
        let mut g: Studio2 = host.init();
        host.run(&mut g, 0.05, 1.0 / 60.0);
        host.tap(180.0, 320.0);
        // A wrong answer, then the right one, then a tap on Gruk's line (mb play's script).
        let base = maimbrain::sys::time();
        let _ = base;
        host.push(synth::tap(180.0, 334.0, 17.5));
        host.push(synth::tap(180.0, 386.0, 26.5));
        host.push(synth::tap(180.0, 560.0, 30.5));
        host.run(&mut g, 46.0, 1.0 / 60.0);
        assert!(g.error.is_none(), "{:?}", g.error);
        (host.scores(), host.drawn())
    };
    let (scores, drawn) = play();
    assert_eq!(scores.last().map(|s| s.1), Some(1), "across, one riddle");
    assert_eq!((scores.clone(), drawn), play());
}

/// A long press skips a cutscene: its cues still fire, in order, then its done.
#[test]
fn holding_skips_a_cutscene_and_its_cues_still_fire() {
    let host = Host::start(9);
    use_project(toll_with(
        r#"
        local t = {}
        on("cue", "a", function() t[#t + 1] = "a" end)
        on("cue", "b", function() t[#t + 1] = "b" end)
        function love.load()
          cutscene{ { "say", "gruk", "halt" }, { "cue", "a" }, { "wait", 3 }, { "cue", "b" }, { "title", "LATER", "", 2 },
            done = function() t[#t + 1] = "done " .. string.format("%.1f", round.time) end }
        end
        function love.draw() love.graphics.print(table.concat(t, " "), 8, 8) end
        "#,
    ));
    let mut g: Studio2 = host.init();
    host.run(&mut g, 0.05, 1.0 / 60.0);
    host.tap(180.0, 320.0);
    host.run(&mut g, 1.0, 1.0 / 60.0);
    let t = maimbrain::sys::time();
    host.push(synth::hold(200.0, 300.0, t + 0.01, 0.9));
    host.run(&mut g, 1.5, 1.0 / 60.0);
    let r = g.round.as_ref().unwrap();
    let said: Vec<String> = r.host().draw.texts().map(str::to_string).collect();
    assert_eq!(said.len(), 1);
    assert!(said[0].starts_with("a b done 1.7"), "{said:?}");
    assert!(!r.host().voice.cinematic());
}

/// Choices: the answer reaches done, a bark may show while one is open, and
/// a tap anywhere else doesn't answer.
#[test]
fn an_answer_reaches_done() {
    let host = Host::start(4);
    use_project(toll_with(
        r#"
        local got = "none"
        function love.load()
          dialogue{ { "gruk", "riddle1", "smug", choices = { "A shadow", "A river", "A coat" } }, done = function(c) got = tostring(c) end }
        end
        function love.draw() love.graphics.print(got, 8, 8) end
        "#,
    ));
    let mut g: Studio2 = host.init();
    host.run(&mut g, 0.05, 1.0 / 60.0);
    host.tap(180.0, 320.0);
    host.run(&mut g, 7.0, 1.0 / 60.0);
    let asking = g.round.as_ref().unwrap().host().voice.talk.asking();
    assert!(asking, "the question waits with its text out");
    // A tap on the scene doesn't answer it.
    host.tap(40.0, 100.0);
    host.run(&mut g, 0.5, 1.0 / 60.0);
    assert!(g.round.as_ref().unwrap().host().voice.talk.asking());
    let layout = maimbrain::ui::Layout::from_screen(&maimbrain::sys::screen());
    let v = &g.round.as_ref().unwrap().host().voice;
    let r = crate::api::voice::choice_rects(&layout, &v.theme, &["A shadow".into(), "A river".into(), "A coat".into()])[1];
    host.tap(r.cx(), r.cy());
    host.run(&mut g, 0.5, 1.0 / 60.0);
    let r = g.round.as_ref().unwrap();
    assert_eq!(r.host().draw.texts().next(), Some("2"));
    assert!(!r.host().voice.busy());
}

/// A cast member's health is the engine's: hits and writes read back the
/// same whether or not the portrait's image has decoded (natively it never
/// does, on the server either), so a replay plays as the phone did. The
/// portrait is made once its image is there, showing that health.
#[test]
fn health_is_the_engines_and_the_portrait_comes_when_its_image_does() {
    let mut v = vm(&toll_with(
        r#"
        local said = ""
        function love.load()
          cast("pip", { portrait = "pip" })
          cast("pip"):hit(0.34)
          cast("pip").health = cast("pip").health - 0.1
          said = string.format("%.2f", cast("pip").health)
          cast("gruk", { portrait = "gruk", side = "right" }):hit(2)
          said = said .. string.format(" %.2f", cast("gruk").health)
        end
        function love.draw() love.graphics.print(said, 8, 8) end
        "#,
    ))
    .unwrap();
    run(&mut v, 0.1);
    assert_eq!(printed(&v), ["0.56 0.00"]);
    let pip = v.host().voice.casts.iter().find(|c| c.key == "pip").unwrap();
    assert!(pip.portrait.is_none() && pip.wanted.is_some(), "natively the image never decodes: the portrait waits");
    // Its image decodes: the portrait is made, showing the health.
    let im = v.host().project.image("portraits/pip").unwrap() as u16;
    v.host_mut().scene.image_mut(im).unwrap().image = Some(maimbrain::gfx2d::Image(7));
    run(&mut v, 0.05);
    let pip = v.host().voice.casts.iter().find(|c| c.key == "pip").unwrap();
    assert!(pip.wanted.is_none());
    assert!((pip.portrait.as_ref().unwrap().health() - 0.56).abs() < 1e-5);
    assert_eq!(printed(&v), ["0.56 0.00"]);
}

/// A cutscene that ends on a fade out leaves the screen covered while its
/// bars slide away (the fade used to lift with them); the next cutscene
/// takes over from it, and one that fades in shows the scene again.
#[test]
fn a_cutscene_that_ends_faded_out_stays_covered() {
    let mut v = vm(&toll_with(
        r#"
        local said = "playing"
        function love.load()
          cutscene{ { "letterbox", 0.12 }, { "wait", 0.5 }, { "fade", "out", 0.5 },
            done = function()
              said = "ended"
              timer.after(2, function() cutscene{ { "fade", "in", 0.5 }, done = function() said = "shown" end } end)
            end }
        end
        function love.draw() love.graphics.print(said, 8, 8) end
        "#,
    ))
    .unwrap();
    run(&mut v, 2.0);
    assert_eq!(printed(&v), ["ended"]);
    assert_eq!(v.host().voice.covered(), 1.0, "still covered a second after it ended, its bars gone");
    run(&mut v, 1.2);
    assert_eq!(v.host().voice.covered(), 0.0, "the next cutscene's fade in took over");
    assert!(v.host().voice.cinematic());
    run(&mut v, 1.0);
    assert_eq!(printed(&v), ["shown"]);
    assert_eq!(v.host().voice.covered(), 0.0);
}

/// A fade out with no cutscene after it lifts after a hold while the round
/// goes on (play isn't left black), and stays once the round has ended.
#[test]
fn a_fade_out_lifts_after_a_hold_unless_the_round_ended() {
    for (end, want) in [(false, 0.0), (true, 1.0)] {
        let mut v = vm(&toll_with(&format!(
            r#"
            function love.load()
              cutscene{{ {{ "fade", "out", 0.3 }}, done = function() if {end} then round.over("THE END") end end }}
            end
            "#
        )))
        .unwrap();
        run(&mut v, 1.5);
        assert_eq!(v.host().voice.covered(), 1.0, "held (round over: {end})");
        run(&mut v, 4.0);
        assert_eq!(v.host().voice.covered(), want, "round over: {end}");
    }
}

/// Bark lines (those the script's tag column puts in a pool: The Toll's
/// jeers and idle lines) are wanted from the round's start, so the first
/// bark's sound has decoded when it's asked for. It used to be wanted the
/// frame the bark showed, and its voice started late (or, decoded slowly,
/// not at all). The rest of the script still waits until it's queued.
#[test]
fn bark_lines_decode_from_the_start() {
    let v = vm(&toll_with("function love.load() end")).unwrap();
    let p = &v.host().project;
    let wanted: Vec<String> = p.voices.iter().zip(&v.host().voice.wanted).filter(|(_, w)| **w).map(|(l, _)| format!("{}/{}", l.who, l.line)).collect();
    assert_eq!(wanted, ["gruk/jeer1", "gruk/jeer2", "gruk/jeer3", "gruk/idle1", "gruk/idle2"]);
}

/// The music dips under a voice on top of the game's own music level
/// (`audio.bus("music").volume`): each is a part of the bus's volume, and
/// neither undoes the other.
#[test]
fn ducking_multiplies_the_games_music_level() {
    use crate::api::sound::music_mix::{GAME, VOICE, get};
    let mut v = vm(&toll_with("function love.load() audio.bus('music').volume = 0.5; say('gruk', 'jeer2') end")).unwrap();
    run(&mut v, 0.5);
    assert_eq!(get(GAME), 0.5);
    assert!((get(VOICE) - 10f32.powf(-5.0 / 20.0)).abs() < 1e-4, "dipped 5 dB while gruk speaks: {}", get(VOICE));
    run(&mut v, 3.0);
    assert_eq!((get(GAME), get(VOICE)), (0.5, 1.0), "back up after the line; the game's level stays");
}

#[test]
fn mistakes_say_what_is_there() {
    let e = |main: &str| match vm(&toll_with(main)) {
        Err(e) => e,
        Ok(mut v) => {
            run(&mut v, 0.05);
            v.step(1.0 / 60.0, &[], &Held::default(), false, false).err().unwrap_or_default()
        }
    };
    assert_eq!(e("say(\"gruk\", \"hello\")"), "game/main.lua:1: say: voices/lines.csv has no line \"hello\" for gruk (theirs: halt, riddle1, right1, riddle2, right2, jeer1, jeer2, jeer3, idle1, idle2)");
    assert_eq!(e("say(\"bob\", \"hello\")"), "game/main.lua:1: say: voices/lines.csv has no lines for \"bob\" (its characters: gruk, pip)");
    assert_eq!(e("bark(\"hurt\")"), "game/main.lua:1: bark: no line in voices/lines.csv is tagged \"hurt\" (tags: halt, idle, please, riddle, right, thanks, wrong)");
    assert_eq!(e("cast(\"gruk\", { portrait = \"troll\" })"), "game/main.lua:1: cast \"gruk\": there's no portrait \"troll\" (images/portraits/troll.png, made with generate kind \"portrait\")");
    assert_eq!(e("cast(\"gruk\", { portrait = \"gate\" })"), "game/main.lua:1: cast \"gruk\": images/gate.png isn't a portrait: its .toml says its rows, portrait = [\"neutral\", …, \"blink\"]");
    assert_eq!(e("cutscene{ { \"zoom\", 2 } }"), "game/main.lua:1: cutscene step 1: there's no step \"zoom\" (letterbox, camera, shot, say, wait, cue, title, fade, credits, meanwhile)");
    assert_eq!(e("on(\"cue\", function() end)"), "game/main.lua:1: on(\"cue\", name, fn): a cutscene's { \"cue\", name } step fires it");
}
