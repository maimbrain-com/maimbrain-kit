//! The platform's rules for a game's round, checked natively (SKILL.md
//! "Platform facts that bite", SPEC §5.1–§5.2): a [`Host`] plays a session
//! the way a player in the feed does (wait on the title card, tap, play,
//! lose, tap through the game-over card) and looks at what the game reported.

use std::panic::{AssertUnwindSafe, catch_unwind};

use super::host::{Call, Host};
use crate::input::{Event, Kind, synth};
use crate::sys::Round;

/// One rule's verdict.
#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    pub rule: &'static str,
    /// None: not checked (an earlier step didn't get there).
    pub ok: Option<bool>,
    pub detail: String,
}

impl Finding {
    fn new(rule: &'static str, ok: bool, detail: impl Into<String>) -> Finding {
        Finding { rule, ok: Some(ok), detail: detail.into() }
    }
    fn skipped(rule: &'static str, why: impl Into<String>) -> Finding {
        Finding { rule, ok: None, detail: why.into() }
    }
}

type Script = Box<dyn Fn(f64) -> Vec<Event>>;

/// Checks a game against the platform's rules for a round, on a fake host:
///
/// - `init` reports nothing but `Idle`, and the first frames run with no
///   assets loaded (natively they never load);
/// - the title card waits for a press: nothing else starts a round;
/// - a press on the title reports `Playing` in the frame it arrives;
/// - the round ends (`Over`), with the score submitted before `Over`;
/// - a tap just after `Over` doesn't restart it, a tap after the guard does,
///   in its frame;
/// - the best is saved, and a new session reads its saved data in `init`;
/// - the same seed and input give the same session (what replays need);
/// - `Rounds::daily(board)` is the board the manifest marks `daily = true`
///   (read from the crate's manifest.toml);
/// - a press on the title held through the start (an intro, a cutscene, a
///   tutorial line) and let go during it leaves the game with nothing held,
///   and the player can act after. It holds the starting press for a tap's
///   length and 1.5 s, then compares what the game draws ([`Host::drawn`])
///   with and without a second release 1.8, 3.5 and 7 s after the press (a
///   game that heard the first one doesn't change), and with and without a
///   fresh touch then (a press held where rounds start, and the
///   [`play`](Conformance::play) script's input from then on): touches that
///   change the game after a tap start must change it after a held start.
///
/// By default nobody plays the round, which ends most games (the bubble
/// shrinks away, the runner hits the first crate). For one that waits,
/// script a way to lose with [`play`](Conformance::play).
///
/// ```ignore
/// #[test]
/// fn keeps_the_platform_rules() {
///     maimbrain::sys::test::Conformance::new().check::<MyGame>();
/// }
/// ```
pub struct Conformance {
    seed: u64,
    dt: f32,
    start: (f32, f32),
    max_round: f32,
    scores: bool,
    play: Option<Script>,
    every: Option<f64>,
    /// The manifest's text, if not the crate's manifest.toml.
    manifest: Option<String>,
}

impl Default for Conformance {
    fn default() -> Self {
        Conformance::new()
    }
}

impl Conformance {
    pub fn new() -> Conformance {
        Conformance { seed: 1, dt: 1.0 / 60.0, start: (180.0, 320.0), max_round: 300.0, scores: true, play: None, every: None, manifest: None }
    }

    /// The session's seed (`sys::rand_seed`; default 1).
    pub fn seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// The frame time (default 1/60 s; 1/120 is a ProMotion iPhone).
    pub fn dt(mut self, dt: f32) -> Self {
        self.dt = dt;
        self
    }

    /// Where the taps that start a round land (default the middle of 360×640).
    pub fn tap_at(mut self, x: f32, y: f32) -> Self {
        self.start = (x, y);
        self
    }

    /// The longest a round may take to end (default 300 s).
    pub fn max_round(mut self, seconds: f32) -> Self {
        self.max_round = seconds;
        self
    }

    /// The game has no leaderboard: don't expect a score before `Over`.
    pub fn no_scores(mut self) -> Self {
        self.scores = false;
        self
    }

    /// Input during the round, to lose it (or to score, so there's a best
    /// to save): called each frame with the seconds since the round started,
    /// it returns the touches to make from now (`input::synth`, their times
    /// in seconds from now: `synth::tap(x, y, 0.0)` taps now,
    /// `synth::stroke(&pts, 0.0, 0.1, ..)` swipes over the next 0.1 s).
    /// Default: none.
    pub fn play(mut self, script: impl Fn(f64) -> Vec<Event> + 'static) -> Self {
        self.play = Some(Box::new(script));
        self
    }

    /// A tap every `seconds` during the round, where the round was started
    /// ([`tap_at`](Conformance::tap_at)): for a game that idling doesn't end.
    pub fn tap_every(mut self, seconds: f32) -> Self {
        self.every = Some(seconds.max(0.01) as f64);
        self
    }

    /// The game's manifest.toml, as text (default: the one in the crate
    /// being tested, `CARGO_MANIFEST_DIR`).
    pub fn manifest(mut self, toml: &str) -> Self {
        self.manifest = Some(toml.to_string());
        self
    }

    /// Checks `G`, prints each rule's verdict, and panics if any rule failed.
    pub fn check<G: crate::Game>(&self) {
        let findings = self.findings::<G>();
        let mut failed = 0;
        println!("platform rules for {}:", std::any::type_name::<G>());
        for f in &findings {
            let mark = match f.ok {
                Some(true) => "ok  ",
                Some(false) => {
                    failed += 1;
                    "FAIL"
                }
                None => "--  ",
            };
            println!("  {mark} {}{}", f.rule, if f.detail.is_empty() { String::new() } else { format!(": {}", f.detail) });
        }
        assert!(failed == 0, "{failed} platform rule(s) broken (SKILL.md \"Platform facts that bite\"):\n{}", findings.iter().filter(|f| f.ok == Some(false)).map(|f| format!("- {}: {}", f.rule, f.detail)).collect::<Vec<_>>().join("\n"));
    }

    /// Each rule's verdict, without printing or panicking.
    pub fn findings<G: crate::Game>(&self) -> Vec<Finding> {
        let (mut findings, first) = self.session::<G>();
        let (_, second) = self.session::<G>();
        let rule = "the same seed and input give the same session";
        findings.push(match (&first, &second) {
            (Some(a), Some(b)) if a.calls == b.calls => Finding::new(rule, true, ""),
            (Some(a), Some(b)) => {
                let (a, b) = (&a.calls, &b.calls);
                let i = a.iter().zip(b).position(|(x, y)| x != y).unwrap_or(a.len().min(b.len()));
                let at = |v: &[Call]| v.get(i).map_or("nothing".into(), |c| format!("{c:?}"));
                Finding::new(rule, false, format!("call {i} differs: {} then {} (randomness not from the seed, or iteration over a HashMap?)", at(a), at(b)))
            }
            _ => Finding::skipped(rule, "the session didn't finish"),
        });
        findings.push(self.daily_rule(first.as_ref()));
        findings.extend(self.release_rules::<G>());
        findings
    }

    /// The starting press held for a while and let go: nothing stays held,
    /// and touches after it do something whenever they do after a tap start
    /// (natively sprites never load, so a game may draw no response at all:
    /// the tap start is what a held start is compared with).
    fn release_rules<G: crate::Game>(&self) -> [Finding; 2] {
        let held = "a press that starts the game, held through its start (an intro, a cutscene) and let go, leaves nothing held";
        let acts = "the player can act after letting go of the starting press";
        let mut stuck = Vec::new();
        let mut responses: Vec<(String, bool)> = Vec::new();
        let mut judged_held = 0;
        let mut why = "";
        let window = (PROBE_WINDOW / self.dt).round() as usize;
        for hold in [synth::TAP_HOLD as f32, 1.5] {
            let what = if hold < 0.5 { "a tap's length".to_string() } else { format!("{hold} s") };
            let Some(base) = self.hold_session::<G>(hold, None) else {
                why = "the session panicked (see above)";
                continue;
            };
            let same = |d: &[u64]| d.iter().all(|x| *x == base.drawn[0]);
            let mut answered = None;
            for (i, delay) in PROBES.iter().enumerate() {
                let (Some(again), Some(touched)) = (self.hold_session::<G>(hold, Some((i, After::Release))), self.hold_session::<G>(hold, Some((i, After::Touch)))) else {
                    why = "the session panicked (see above)";
                    continue;
                };
                let at = base.probes[i];
                if again.drawn[..at] != base.drawn[..at] || touched.drawn[..at] != base.drawn[..at] {
                    why = "the same input drew differently (see the determinism rule)";
                    continue;
                }
                judged_held += 1;
                if again.drawn[at..] != base.drawn[at..] {
                    stuck.push(format!("held for {what}, a second release {:.1} s after the first", delay - hold));
                }
                // Only while the round is on and not ending (an ending takes presses).
                if same(&base.drawn) && same(&touched.drawn) {
                    why = "it draws the same every frame natively";
                } else if base.state[i] == Some(Round::Playing) && !base.over.iter().any(|f| (at..at + window).contains(f)) {
                    let changed = touched.drawn[at..(at + window).min(touched.drawn.len())] != base.drawn[at..(at + window).min(base.drawn.len())];
                    answered = Some(answered.unwrap_or(false) || changed);
                }
            }
            // Any of the moments will do: early on an intro may still be
            // playing, later the game may be between turns.
            if let Some(changed) = answered {
                responses.push((what, changed));
            }
        }
        let held_finding = match (judged_held, stuck.is_empty()) {
            (0, _) => Finding::skipped(held, why),
            (_, true) => Finding::new(held, true, ""),
            (_, false) => Finding::new(
                held,
                false,
                format!(
                    "the game still thought the finger was down: a second release changed what it drew ({}). Something kept the release from it (a cutscene, dialogue or menu that took it, or a `continue` past it): give the game every release, or ask `input::held()` what's down",
                    stuck.join("; ")
                ),
            ),
        };
        let list = |ok: bool| responses.iter().filter(|r| r.1 == ok).map(|r| r.0.as_str()).collect::<Vec<_>>().join(" or ");
        let acts_finding = if responses.is_empty() {
            Finding::skipped(acts, if judged_held == 0 { why } else { "the round wasn't on after letting go" })
        } else if responses.iter().all(|r| !r.1) {
            Finding::skipped(acts, "no touch changed what it draws natively (sprites never load there, or the touches missed what they act on: `.play(..)` scripts real moves), so there's nothing to compare with")
        } else if responses.iter().all(|r| r.1) {
            Finding::new(acts, true, "")
        } else {
            Finding::new(
                acts,
                false,
                format!("after a starting press held for {}, touches {} s after the press changed nothing the game drew (held for {} they did): holding the start left its input locked, waiting for a release it never got", list(false), PROBES.map(|p| p.to_string()).join(", "), list(true)),
            )
        };
        [held_finding, acts_finding]
    }

    /// The title, then a press held `hold` s and let go, then at each of
    /// [`PROBES`] after the press, `probe`'s event if it's that one.
    fn hold_session<G: crate::Game>(&self, hold: f32, probe: Option<(usize, After)>) -> Option<HoldRun> {
        let (x, y) = self.start;
        let dt = self.dt;
        let host = Host::start(self.seed);
        let ran = catch_unwind(AssertUnwindSafe(|| {
            let mut game: G = host.init();
            host.run(&mut game, 1.0, dt);
            let down = host.now() + 1e-4;
            host.push([synth::touch(Kind::TouchDown, x, y, down)]);
            host.push([synth::touch(Kind::TouchUp, x, y, down + hold as f64)]);
            let mut at = Vec::new();
            // A touch is a press held where the round was started, and the
            // `play` script's input from then on (where its moves are).
            let mut playing_from = None;
            let end = down + (PROBES[PROBES.len() - 1] + PROBE_WINDOW) as f64;
            while host.now() < end - 1e-6 {
                let now = host.now();
                if let Some(&delay) = PROBES.get(at.len())
                    && now >= down + delay as f64 - 1e-6
                {
                    let i = at.len();
                    at.push(host.frames() as usize);
                    match probe {
                        Some((p, After::Release)) if p == i => host.push([synth::touch(Kind::TouchUp, x, y, now + 1e-4)]),
                        Some((p, After::Touch)) if p == i => {
                            host.push(synth::hold(x, y, now + 1e-4, 0.4));
                            playing_from = Some(now);
                        }
                        _ => {}
                    }
                }
                if let (Some(from), Some(script)) = (playing_from, &self.play)
                    && now - from < PROBE_WINDOW as f64
                {
                    host.push(script(now - from).into_iter().map(|mut e| {
                        e.time = now + e.time.max(1e-4);
                        e
                    }));
                }
                host.frame(&mut game, dt);
            }
            at
        }));
        let probes = ran.ok()?;
        let calls = host.calls();
        let rounds: Vec<(usize, Round)> = calls.iter().filter_map(|c| if let Call::Round { frame, state, .. } = c { Some((*frame as usize, *state)) } else { None }).collect();
        let state = probes.iter().map(|at| rounds.iter().rfind(|(f, _)| f <= at).map(|r| r.1)).collect();
        let over = rounds.iter().filter(|r| r.1 == Round::Over).map(|r| r.0).collect();
        Some(HoldRun { drawn: host.drawn(), probes, state, over })
    }

    /// `Rounds::daily(board)` against the manifest's `daily = true` board.
    fn daily_rule(&self, s: Option<&Session>) -> Finding {
        let rule = "Rounds' daily board is the manifest's `daily = true` board";
        let Some(rounds) = s.and_then(|s| s.daily_board) else { return Finding::skipped(rule, "no Rounds::daily") };
        let text = self.manifest.clone().or_else(|| std::env::var("CARGO_MANIFEST_DIR").ok().and_then(|d| std::fs::read_to_string(std::path::Path::new(&d).join("manifest.toml")).ok()));
        let Some(text) = text else { return Finding::skipped(rule, "no manifest.toml in the crate's directory") };
        let boards = score_boards(&text);
        let marked: Vec<u32> = boards.iter().filter(|b| b.1).map(|b| b.0).collect();
        match marked.first() {
            Some(&d) if d == rounds => Finding::new(rule, true, format!("board {d}")),
            Some(&d) => Finding::new(rule, false, format!("Rounds::daily({rounds}), but the manifest's daily board is {d}: the platform keeps board {d}'s standings per day")),
            None if boards.iter().any(|b| b.0 == rounds) => Finding::new(rule, false, format!("Rounds::daily({rounds}), but board {rounds} isn't `daily = true` in the manifest: mark it (`mb upgrade --apply` does), so its standings start over each day")),
            None => Finding::new(rule, false, format!("Rounds::daily({rounds}), but the manifest has no [[scores]] board {rounds}")),
        }
    }

    /// One session; its findings and, if it ran to the end, what it reported.
    fn session<G: crate::Game>(&self) -> (Vec<Finding>, Option<Session>) {
        let mut out = Vec::new();
        let host = Host::start(self.seed);
        let ran = catch_unwind(AssertUnwindSafe(|| self.steps::<G>(&host, &mut out)));
        if let Err(p) = ran {
            let msg = p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            out.push(Finding::new("runs without panicking", false, format!("frame {}: {msg}", host.frames())));
            return (out, None);
        }
        let calls: Vec<Call> = host.calls().into_iter().filter(|c| !matches!(c, Call::Log { .. })).collect();
        (out, Some(Session { calls, daily_board: host.rounds_daily_board() }))
    }

    fn steps<G: crate::Game>(&self, host: &Host, out: &mut Vec<Finding>) {
        let (x, y) = self.start;
        let dt = self.dt;
        let mut game: G = host.init();
        let at_init = host.rounds();
        out.push(Finding::new("init reports nothing but Idle", at_init.iter().all(|r| *r == Round::Idle), format!("{at_init:?}")));
        let loads = host.calls().iter().filter(|c| matches!(c, Call::Load { frame: 0, .. })).count();

        // The title card: frames with nothing loaded, a hover, no press.
        host.push([synth::touch(Kind::MouseMove, x, y, 0.5), synth::touch(Kind::MouseMove, x + 40.0, y, 1.0)]);
        host.run(&mut game, 2.0, dt);
        let started = host.rounds().contains(&Round::Playing);
        out.push(Finding::new("the title waits for a press (no timer, hover or sensor starts a round)", !started, if started { "Playing was reported with no press" } else { "" }));

        // The tap that starts the round.
        let calls_before = host.calls().len();
        host.tap(x, y);
        host.frame(&mut game, dt);
        let tap_frame = host.frames();
        let playing_at = first_round_after(host, calls_before, Round::Playing);
        out.push(match playing_at {
            Some(f) if f == tap_frame => Finding::new("a press on the title starts the round in its frame", true, ""),
            Some(f) => Finding::new("a press on the title starts the round in its frame", false, format!("Playing came {} frame(s) after the tap", f - tap_frame)),
            None => Finding::new("a press on the title starts the round in its frame", false, format!("no Playing after a tap at ({x}, {y}); `.tap_at(x, y)` if a round starts elsewhere")),
        });
        if playing_at.is_none() {
            return;
        }

        // The round, until Over.
        let t0 = host.now();
        let mut over = None;
        let mut next_tap = self.every.map(|e| t0 + e);
        while host.now() - t0 < self.max_round as f64 {
            let now = host.now();
            if let Some(script) = &self.play {
                // Times from now; never before the next frame.
                host.push(script(now - t0).into_iter().map(|mut e| {
                    e.time = now + e.time.max(1e-4);
                    e
                }));
            }
            if let (Some(at), Some(every)) = (&mut next_tap, self.every)
                && now >= *at
            {
                host.tap(x, y);
                *at += every;
            }
            host.frame(&mut game, dt);
            over = first_round_after(host, calls_before, Round::Over);
            if over.is_some() {
                break;
            }
        }
        let rule = "the round ends";
        let Some(over_frame) = over else {
            out.push(Finding::new(rule, false, format!("no Over in {} s: give it a way to lose with `.play(..)`, or `.max_round(..)`", self.max_round)));
            return;
        };
        out.push(Finding::new(rule, true, format!("Over {:.1} s after the tap", host.now() - t0)));

        let round: Vec<Call> = host.calls()[calls_before..].to_vec();
        let over_at = round.iter().position(|c| matches!(c, Call::Round { state: Round::Over, .. })).unwrap();
        let score = round[..over_at].iter().rev().find_map(|c| if let Call::Score { value, .. } = c { Some(*value) } else { None });
        let rule = "the score is submitted before Over (the replay is saved on Over)";
        if self.scores {
            let after = round[over_at..].iter().any(|c| matches!(c, Call::Score { .. }));
            out.push(match (score, after) {
                (Some(v), _) => Finding::new(rule, true, format!("score {v}")),
                (None, true) => Finding::new(rule, false, "submitted after Over"),
                (None, false) => Finding::new(rule, false, "no score submitted (`.no_scores()` if the game has no leaderboard)"),
            });
        } else {
            out.push(Finding::skipped(rule, "no leaderboard"));
        }
        let rule = "the best is saved";
        let saved = round.iter().any(|c| matches!(c, Call::Save { .. }));
        out.push(match score {
            Some(v) if v > 0 => Finding::new(rule, saved, if saved { "" } else { "a round scored but nothing was saved" }),
            _ => Finding::skipped(rule, "the round scored nothing"),
        });

        // A frantic tap during the failure, then a deliberate one.
        let calls_over = host.calls().len();
        host.tap(x, y);
        host.run(&mut game, 0.25, dt);
        let early = first_round_after(host, calls_over, Round::Playing);
        out.push(Finding::new("a tap just after Over doesn't restart", early.is_none(), if early.is_some() { "a tap 0.02 s after Over started a round: ignore taps for ~0.35 s" } else { "" }));
        let rule = "a tap on the game-over card retries, in its frame (the press, or a button's release)";
        if early.is_some() {
            out.push(Finding::skipped(rule, "the round had already restarted"));
            return;
        }
        let since_over = host.now() - frame_time(host, over_frame);
        if since_over < 1.0 {
            host.run(&mut game, (1.0 - since_over) as f32, dt);
        }
        // Anywhere on the card retries on the press; its retry button, like
        // any button, on the release: either is the tap's own moment.
        let calls_retry = host.calls().len();
        host.tap(x, y);
        host.frame(&mut game, dt);
        let press_frame = host.frames();
        host.run(&mut game, synth::TAP_HOLD as f32 + dt, dt);
        let release_frame = host.frames();
        let retry = first_round_after(host, calls_retry, Round::Playing);
        out.push(match retry {
            Some(f) if f <= release_frame => Finding::new(rule, true, if f > press_frame { "on the release (a button)" } else { "" }),
            Some(f) => Finding::new(rule, false, format!("Playing came {} frame(s) after the tap", f - press_frame)),
            None => Finding::new(rule, false, "a tap 1 s after Over didn't start a round"),
        });

        // A new session reads back what this one saved.
        let rule = "a new session reads its saved data in init";
        if round.iter().any(|c| matches!(c, Call::Save { .. })) {
            let n = host.calls().len();
            let _again: G = host.init();
            let read = host.calls()[n..].iter().any(|c| matches!(c, Call::Load { frame: 0, .. }));
            out.push(Finding::new(rule, read, if read { "" } else { "init didn't read the store: the best is lost between sessions" }));
        } else {
            out.push(if loads > 0 { Finding::new(rule, true, "") } else { Finding::skipped(rule, "nothing saved") });
        }
    }
}

/// The manifest's `[[scores]]` boards: (board, daily). A small reader for
/// just these keys (the SDK has no TOML parser).
fn score_boards(toml: &str) -> Vec<(u32, bool)> {
    let mut out: Vec<(u32, bool)> = Vec::new();
    let mut in_scores = false;
    for line in toml.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.starts_with('[') {
            in_scores = line == "[[scores]]";
            if in_scores {
                out.push((0, false));
            }
            continue;
        }
        let (Some(b), true) = (out.last_mut(), in_scores) else { continue };
        let Some((k, v)) = line.split_once('=') else { continue };
        match k.trim() {
            "board" => b.0 = v.trim().parse().unwrap_or(0),
            "daily" => b.1 = v.trim() == "true",
            _ => {}
        }
    }
    out
}

/// What `hold_session` does at a probe.
#[derive(Clone, Copy)]
enum After {
    /// The same finger lets go again (a game that heard the first doesn't care).
    Release,
    /// A fresh touch, held 0.4 s.
    Touch,
}

/// Seconds after the starting press that `release_rules` lets go again or
/// touches, after it was let go (an intro may still be playing at the
/// first). From the press, so a tap start and a held one are compared at the
/// same moments of the round.
const PROBES: [f32; 3] = [1.8, 3.5, 7.0];
/// How long after a probe its effect is looked for (s).
const PROBE_WINDOW: f32 = 1.5;

struct HoldRun {
    /// [`Host::drawn`].
    drawn: Vec<u64>,
    /// At each of `PROBES`: the index in `drawn` of the frame that gets its
    /// event (the frame number before it), and the round then.
    probes: Vec<usize>,
    state: Vec<Option<Round>>,
    /// The frames that reported `Over`.
    over: Vec<usize>,
}

/// What a session reported.
struct Session {
    calls: Vec<Call>,
    /// The board `Rounds::daily` was given.
    daily_board: Option<u32>,
}

/// The frame of the first `state` reported after the first `from` calls.
fn first_round_after(host: &Host, from: usize, state: Round) -> Option<u64> {
    host.calls().get(from..)?.iter().find_map(|c| match c {
        Call::Round { frame, state: s, .. } if *s == state => Some(*frame),
        _ => None,
    })
}

/// Game time at the end of `frame` (from the round calls recorded then).
fn frame_time(host: &Host, frame: u64) -> f64 {
    host.calls().iter().rev().find_map(|c| match c {
        Call::Round { frame: f, time, .. } if *f == frame => Some(*time),
        _ => None,
    })
    .unwrap_or_else(|| host.now())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Game, input, store, sys};

    /// A round that ends after 2 s of play, with knobs for each mistake.
    #[derive(Default)]
    struct Toy {
        playing: bool,
        over_for: Option<f32>,
        t: f32,
        best: u64,
    }

    thread_local! {
        static BUG: std::cell::Cell<&'static str> = const { std::cell::Cell::new("") };
    }

    impl Game for Toy {
        fn init() -> Self {
            sys::round(Round::Idle);
            Toy { best: store::get_u64("best").unwrap_or(0), ..Toy::default() }
        }
        fn update(&mut self, dt: f32) {
            let bug = BUG.with(|b| b.get());
            for e in input::poll() {
                let ready = self.over_for.is_none_or(|t| t > 0.35 || bug == "no guard");
                if e.is_press() && !self.playing && ready {
                    self.playing = true;
                    self.over_for = None;
                    self.t = 0.0;
                    sys::round(Round::Playing);
                }
            }
            if bug == "timer start" && !self.playing && self.over_for.is_none() {
                self.t += dt;
                if self.t > 1.0 {
                    self.playing = true;
                    sys::round(Round::Playing);
                }
            }
            if self.playing {
                self.t += dt;
                if self.t > 2.0 {
                    self.playing = false;
                    self.over_for = Some(0.0);
                    if bug != "score after" {
                        store::submit_score(0, 7);
                    }
                    if self.best < 7 {
                        self.best = 7;
                        store::set_u64("best", 7);
                    }
                    sys::round(Round::Over);
                    if bug == "score after" {
                        store::submit_score(0, 7);
                    }
                }
            }
            if let Some(t) = &mut self.over_for {
                *t += dt;
            }
        }
        fn render(&self) {}
    }

    fn verdicts(bug: &'static str) -> Vec<(String, Option<bool>)> {
        BUG.with(|b| b.set(bug));
        let f = Conformance::new().findings::<Toy>();
        BUG.with(|b| b.set(""));
        f.into_iter().map(|f| (f.rule.to_string(), f.ok)).collect()
    }

    fn failed(bug: &'static str) -> Vec<String> {
        verdicts(bug).into_iter().filter(|v| v.1 == Some(false)).map(|v| v.0).collect()
    }

    #[test]
    fn a_game_that_keeps_the_rules_passes() {
        assert_eq!(failed(""), Vec::<String>::new(), "{:#?}", verdicts(""));
        Conformance::new().check::<Toy>();
    }

    #[test]
    fn the_daily_board_must_be_the_manifests() {
        use crate::rounds::Rounds;
        use crate::ui::{Theme, TitleCard};
        struct Daily(Rounds);
        impl Game for Daily {
            fn init() -> Self {
                Daily(Rounds::new(TitleCard::new("D", Theme::default())).daily(1).ending(0.0))
            }
            fn update(&mut self, dt: f32) {
                for e in input::poll() {
                    self.0.input(&e);
                }
                if self.0.playing() && self.0.age() > 1.0 {
                    self.0.end(3);
                }
                self.0.update(dt);
            }
            fn render(&self) {}
        }
        let rule = "Rounds' daily board is the manifest's `daily = true` board";
        let verdict = |toml: &str| Conformance::new().manifest(toml).findings::<Daily>().into_iter().find(|f| f.rule == rule).unwrap().ok;
        assert_eq!(verdict("[[scores]]\nboard = 0\n\n[[scores]]\nboard = 1 # today\ndaily = true\n"), Some(true));
        assert_eq!(verdict("[[scores]]\nboard = 0\n[[scores]]\nboard = 1\n[[scores]]\nboard = 2\ndaily = true\n"), Some(false), "board 2 is the daily one");
        assert_eq!(verdict("[[scores]]\nboard = 0\n"), Some(false), "no board 1 at all");
        assert_eq!(verdict("[[scores]]\nboard = 0\n[[scores]]\nboard = 1\n[remix]\ndaily = true\n"), Some(false), "a daily board the manifest doesn't mark");
        let none = Conformance::new().findings::<Toy>().into_iter().find(|f| f.rule == rule).unwrap();
        assert_eq!(none.ok, None, "no Rounds::daily");
    }

    /// A side-scroller with an intro: the press on the title starts the
    /// round and a 2 s intro (a `Sequence`), and is also the first move:
    /// holding walks the girl right. Knobs for the ways it goes wrong.
    struct Walker {
        rounds: crate::rounds::Rounds,
        intro: Option<crate::ui::Sequence>,
        intro_left: f32,
        walking: bool,
        x: f32,
        /// "locks": input waits for a release after the start.
        locked: bool,
    }

    thread_local! {
        static WALKER_BUG: std::cell::Cell<&'static str> = const { std::cell::Cell::new("") };
    }

    impl Game for Walker {
        fn init() -> Self {
            use crate::ui::{Theme, TitleCard};
            let rounds = crate::rounds::Rounds::new(TitleCard::new("CANDLE", Theme::default())).ending(0.0);
            Walker { rounds, intro: None, intro_left: 0.0, walking: false, x: 0.0, locked: false }
        }
        fn update(&mut self, dt: f32) {
            use crate::rounds::Input;
            let bug = WALKER_BUG.with(|b| b.get());
            for e in input::poll() {
                // The intro takes what it takes: a hand-rolled one that takes
                // everything (the bug), or the SDK's sequence.
                if self.intro_left > 0.0 && (bug == "swallow" || bug == "swallow, asks held") {
                    continue;
                }
                // "locks": a cold open lets the first 0.5 s through, then a
                // hand-rolled intro takes everything; the game waits for the
                // start press's release before it takes presses.
                if bug == "locks" {
                    if (0.0..1.5).contains(&self.intro_left) {
                        continue;
                    }
                    if self.locked {
                        self.locked &= !e.is_release();
                        continue;
                    }
                }
                if let Some(seq) = &mut self.intro
                    && seq.input(&e)
                {
                    continue;
                }
                match self.rounds.input(&e) {
                    Input::Start => {
                        self.x = 0.0;
                        self.intro_left = 2.0;
                        if bug == "locks" {
                            self.locked = true;
                        } else {
                            self.intro = Some(crate::ui::Sequence::new(crate::ui::Theme::default()).wait(2.0));
                            self.walking = true;
                        }
                    }
                    Input::Play if e.is_press() => self.walking = true,
                    Input::Play if e.is_release() => self.walking = false,
                    _ => {}
                }
            }
            if bug == "swallow, asks held" && !input::held().pointer(0) {
                self.walking = false;
            }
            if let Some(seq) = &mut self.intro {
                seq.update(dt);
            }
            self.intro_left -= dt;
            if self.rounds.playing() && self.intro_left <= 0.0 && self.walking {
                self.x += 60.0 * dt;
            }
            if self.rounds.playing() && self.rounds.age() > 8.0 {
                self.rounds.end(self.x as i64);
            }
            self.rounds.update(dt);
        }
        fn render(&self) {
            crate::gfx2d::circle(20.0 + self.x, 400.0, 12.0, 0xffffffff);
            self.rounds.draw();
        }
    }

    fn walker(bug: &'static str) -> Vec<(String, Option<bool>)> {
        WALKER_BUG.with(|b| b.set(bug));
        let f = Conformance::new().no_scores().findings::<Walker>();
        WALKER_BUG.with(|b| b.set(""));
        if bug == "swallow" {
            let f = f.iter().find(|f| f.ok == Some(false)).unwrap();
            assert!(f.detail.contains("held for a tap's length, a second release 3.4 s after the first") && f.detail.contains("input::held()"), "{}", f.detail);
        }
        f.into_iter().map(|f| (f.rule.to_string(), f.ok)).filter(|f| f.0.contains("let go") || f.0.contains("letting go")).collect()
    }

    #[test]
    fn a_release_during_the_intro_must_reach_the_game() {
        let held = "a press that starts the game, held through its start (an intro, a cutscene) and let go, leaves nothing held";
        let acts = "the player can act after letting go of the starting press";
        // The SDK's sequence passes releases on: nothing stuck.
        assert_eq!(walker(""), [(held.to_string(), Some(true)), (acts.to_string(), Some(true))]);
        // An intro that swallows the release: the girl walks on by herself.
        assert_eq!(walker("swallow"), [(held.to_string(), Some(false)), (acts.to_string(), Some(true))]);
        // The same intro, but the game asks `input::held()`: fine.
        assert_eq!(walker("swallow, asks held")[0], (held.to_string(), Some(true)));
        // A game whose input waits for the start press's release, which its
        // intro took: after a tap start it plays, after a held one it never does.
        assert_eq!(walker("locks"), [(held.to_string(), Some(true)), (acts.to_string(), Some(false))]);
    }

    #[test]
    fn each_broken_rule_is_named() {
        assert_eq!(failed("no guard"), ["a tap just after Over doesn't restart"]);
        assert_eq!(failed("score after"), ["the score is submitted before Over (the replay is saved on Over)"]);
        assert!(failed("timer start").contains(&"the title waits for a press (no timer, hover or sensor starts a round)".to_string()));
    }
}
