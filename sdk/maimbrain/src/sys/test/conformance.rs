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
/// - the same seed and input give the same session (what replays need).
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
}

impl Default for Conformance {
    fn default() -> Self {
        Conformance::new()
    }
}

impl Conformance {
    pub fn new() -> Conformance {
        Conformance { seed: 1, dt: 1.0 / 60.0, start: (180.0, 320.0), max_round: 300.0, scores: true, play: None, every: None }
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
        findings.push(match (first, second) {
            (Some(a), Some(b)) if a == b => Finding::new(rule, true, ""),
            (Some(a), Some(b)) => {
                let i = a.iter().zip(&b).position(|(x, y)| x != y).unwrap_or(a.len().min(b.len()));
                let at = |v: &[Call]| v.get(i).map_or("nothing".into(), |c| format!("{c:?}"));
                Finding::new(rule, false, format!("call {i} differs: {} then {} (randomness not from the seed, or iteration over a HashMap?)", at(&a), at(&b)))
            }
            _ => Finding::skipped(rule, "the session didn't finish"),
        });
        findings
    }

    /// One session; its findings and, if it ran to the end, what it reported.
    fn session<G: crate::Game>(&self) -> (Vec<Finding>, Option<Vec<Call>>) {
        let mut out = Vec::new();
        let host = Host::start(self.seed);
        let ran = catch_unwind(AssertUnwindSafe(|| self.steps::<G>(&host, &mut out)));
        if let Err(p) = ran {
            let msg = p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            out.push(Finding::new("runs without panicking", false, format!("frame {}: {msg}", host.frames())));
            return (out, None);
        }
        let calls: Vec<Call> = host.calls().into_iter().filter(|c| !matches!(c, Call::Log { .. })).collect();
        (out, Some(calls))
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
    fn each_broken_rule_is_named() {
        assert_eq!(failed("no guard"), ["a tap just after Over doesn't restart"]);
        assert_eq!(failed("score after"), ["the score is submitted before Over (the replay is saved on Over)"]);
        assert!(failed("timer start").contains(&"the title waits for a press (no timer, hover or sensor starts a round)".to_string()));
    }
}
