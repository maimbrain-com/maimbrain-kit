//! Phase 4c (docs/STUDIO-2.md §9.5) natively, on Tally (fixtures/tally):
//! `round.post` to a second board, the daily board and its mode, progress
//! and a checkpoint carried into a later session. The server's side (the
//! same game's runs verified by a real replay) is in the server's
//! test/integration/studio/progress2_test.rb.

use std::path::Path;

use maimbrain::sys::Round;
use maimbrain::sys::test::{Conformance, Host};
use maimbrain::ui::Layout;

use crate::project::Files;
use crate::tests::{read, with_main};
use crate::{Studio2, use_project};

const DT: f32 = 1.0 / 60.0;

fn tally() -> Files {
    read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/tally"))
}

/// A number progress keeps (save.rs's byte format: `n` and an f64).
fn number(host: &Host, key: &str) -> Option<f64> {
    let b = host.saved(&format!("progress/{key}"))?;
    (b.first() == Some(&b'n')).then(|| f64::from_le_bytes(b[1..9].try_into().unwrap()))
}

/// Where DAILY is on the title card of a 360 × 640 screen with no insets.
fn daily_button() -> (f32, f32) {
    let r = crate::boards::title_row(&Layout::from_screen(&Default::default()))[1];
    (r.cx(), r.cy())
}

/// Taps `n` times a third of a second apart (the first starts the round).
fn climb(host: &Host, g: &mut Studio2, n: usize) {
    for _ in 0..n {
        host.tap(180.0, 384.0);
        host.run(g, 0.3, DT);
    }
}

#[test]
fn tally_posts_its_taps_once_and_keeps_its_progress() {
    let host = Host::start(4);
    use_project(tally());
    let mut g: Studio2 = host.init();
    host.run(&mut g, 1.0, DT);
    assert!(g.error.is_none(), "{:?}", g.error);
    // The title card's attract mode posts nothing and keeps no progress.
    assert!(host.scores().is_empty());
    climb(&host, &mut g, 4);
    assert!(g.error.is_none(), "{:?}", g.error);
    // A climb in progress is checkpointed.
    assert!(host.saved("progress/run").is_some_and(|b| !b.is_empty()));
    host.run(&mut g, 3.0, DT);
    assert!(host.rounds().contains(&Round::Over));
    let scores = host.scores();
    // Board 0 has round.score; board 1 the round's best taps, sent once.
    let height = scores.iter().find(|s| s.0 == 0).expect("board 0's score").1;
    assert!([4, 8, 12].contains(&height), "4 floors at 1, 2 or 3 each: {height}");
    assert_eq!(scores.iter().filter(|s| s.0 == 1).collect::<Vec<_>>(), [&(1, 4)], "{scores:?}");
    assert!(!scores.iter().any(|s| s.0 == 2), "not a daily round: {scores:?}");
    assert_eq!((number(&host, "runs"), number(&host, "floor")), (Some(1.0), Some(4.0)));
    assert!(host.saved("progress/run").is_none_or(|b| b.is_empty()), "the run ended: its checkpoint is cleared");
    assert!(host.saved("progress/tower").is_none(), "floor 5 wasn't reached");
}

/// A run longer than the platform's catch-up: the player leaves mid-climb,
/// and a later session's round carries on from the checkpoint.
#[test]
fn a_checkpoint_carries_a_climb_into_a_later_session() {
    let host = Host::start(5);
    use_project(tally());
    let mut g: Studio2 = host.init();
    host.run(&mut g, 1.0, DT);
    climb(&host, &mut g, 3);
    drop(g); // left mid-round, on floor 3
    assert!(number(&host, "runs").is_none(), "the round never ended");

    let mut g: Studio2 = host.init();
    host.run(&mut g, 1.0, DT);
    climb(&host, &mut g, 2); // floors 4 and 5
    host.run(&mut g, 4.0, DT);
    assert!(g.error.is_none(), "{:?}", g.error);
    assert_eq!((number(&host, "runs"), number(&host, "floor")), (Some(1.0), Some(5.0)), "floors 1–3 came from the checkpoint");
    assert!(host.saved("progress/tower").is_some(), "floor 5 unlocks the tower");
    // The taps board counts this round's taps only.
    assert!(host.scores().contains(&(1, 2)), "{:?}", host.scores());
}

/// DAILY on the title card plays the day's seed on the daily board, with
/// the day's own best; the next UTC day's seed starts a new best.
#[test]
fn the_daily_board_plays_the_days_seed_and_its_best_is_the_days() {
    let host = Host::start(6);
    host.set_daily_seed(0xd1);
    use_project(tally());
    let mut g: Studio2 = host.init();
    host.run(&mut g, 1.0, DT);
    let (x, y) = daily_button();
    host.tap(x, y);
    host.run(&mut g, 0.3, DT);
    assert!(g.rounds.is_daily());
    assert!(!host.rounds().contains(&Round::Playing), "DAILY doesn't start a round");
    climb(&host, &mut g, 3);
    assert!(g.round.as_ref().unwrap().host().boards.daily, "round.daily");
    host.run(&mut g, 3.0, DT);
    let scores = host.take_calls();
    let scores: Vec<(u32, i64)> = scores.into_iter().filter_map(|c| if let maimbrain::sys::test::Call::Score { board, value, .. } = c { Some((board, value)) } else { None }).collect();
    let today = scores.iter().find(|s| s.0 == 2).expect("the daily board's score").1;
    assert!(!scores.iter().any(|s| s.0 == 0), "a daily round's score is the daily board's: {scores:?}");
    assert!(scores.contains(&(1, 3)), "round.post's boards still count: {scores:?}");
    assert!(host.saved("daily:00000000000000d1").is_some(), "the day's best");
    assert!(host.saved("best").is_none());

    // The same day again: the same course, so the same climb scores the same.
    let mut g: Studio2 = host.init();
    host.run(&mut g, 1.0, DT);
    host.tap(x, y);
    host.run(&mut g, 0.3, DT);
    assert_eq!(g.rounds.best(), today, "today's best");
    climb(&host, &mut g, 3);
    host.run(&mut g, 3.0, DT);
    assert!(host.scores().contains(&(2, today)), "{:?}", host.scores());

    // Past midnight UTC the platform hands the next day's seed: a new best.
    host.set_daily_seed(0xd2);
    let mut g: Studio2 = host.init();
    host.run(&mut g, 1.0, DT);
    host.tap(x, y);
    host.run(&mut g, 0.3, DT);
    assert!(g.rounds.is_daily());
    assert_eq!(g.rounds.best(), 0, "a new day's board starts empty");
    assert!(host.saved("daily:00000000000000d1").is_some(), "yesterday's best stays where it was");
}

/// The platform's rules hold with the daily board's buttons on the cards,
/// and Rounds' daily board is the manifest's.
#[test]
fn tally_keeps_the_platform_rules() {
    use_project(tally());
    let manifest = "abi = 0\nid = \"dev.maimbrain.studio2.tally\"\nname = \"Tally\"\nversion = \"0.1.0\"\ncreator = \"@maimbrain\"\norientation = \"portrait\"\nlogical_size = [360, 640]\ninputs = [\"touch\"]\nperf_tier = \"lite\"\ncapabilities = [\"store\", \"score\", \"progress\"]\n[[scores]]\nboard = 0\nlabel = \"Height\"\norder = \"higher\"\n[[scores]]\nboard = 1\nlabel = \"Taps\"\norder = \"higher\"\n[[scores]]\nboard = 2\nlabel = \"Today\"\norder = \"higher\"\ndaily = true\n";
    Conformance::new().max_round(10.0).manifest(manifest).check::<Studio2>();
}

#[test]
fn round_post_says_what_went_wrong() {
    let boards = "\n[[scores]]\nid = \"coins\"\n[[scores]]\nid = \"day\"\ndaily = true\n";
    let error = |lua: &str| -> String {
        let mut f = with_main(&format!("function love.update() {lua} end"), &[]);
        let toml = String::from_utf8(f["studio.toml"].clone()).unwrap() + boards;
        f.insert("studio.toml".into(), toml.into_bytes());
        let p = std::rc::Rc::new(crate::project::Project::parse(&f).unwrap());
        let mut v = crate::vm::Vm::new(&p, 7, crate::vm::Mode::Round, &[], &[], &[], (0.0, 0.0, 360.0, 640.0)).unwrap();
        v.step(DT, &[], &Default::default(), false, false).expect_err(lua)
    };
    assert!(error("round.post('gems', 1)").contains("studio.toml has no board \"gems\" (its boards with ids: \"coins\", \"day\")"));
    assert!(error("round.post('day', 1)").contains("is the daily board"));
    assert!(error("round.post('coins', 'lots')").contains("round.post"));
    assert!(error("round.board('gems')").contains("round.board: studio.toml has no board"));
    assert!(error("round.post('coins', 0/0)").contains("value"));
}
