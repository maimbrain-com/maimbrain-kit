//! A project's leaderboards (docs/STUDIO-2.md §5 [rounds]): studio.toml's
//! `[[scores]]` as the manifest's boards, `round.post` to the boards past
//! board 0, and the daily board's mode.
//!
//! The first `[[scores]]` is board 0, the one `round.score` goes to. Each
//! later one has an `id` and is board 1, 2, … in order. One of them may be
//! `daily = true`: the daily board (SPEC §5.8), which `round.score` goes to
//! in a daily round, played on the day's seed (`round.daily`), its standings
//! starting over each UTC day. The engine shows DAILY on the title card and
//! the game-over card to switch to it, and SCORES on the game-over card for
//! the round's board, as the genre kits do.
//!
//! `round.post(id, value)` keeps the round's best on that board; the engine
//! submits each board's best once, when the game-over card goes up, so a
//! game posting every frame sends one score a round. `mb build` (mb-cli's
//! `studio::v2`) writes the manifest's boards from [`parse`], so the engine
//! and the manifest always agree on the numbers.

use std::cell::Cell;

use maimbrain::input::Event;
use maimbrain::rounds::{Rounds, Stage};
use maimbrain::store;
use maimbrain::ui::{Button, ButtonKind, Icon, Layout, Rect, Tap, Theme};
use toml::{Table, Value};

/// The most boards a game has (the manifest's 0–15).
pub const MAX: usize = 16;
/// A board's `id` and `label`, at most.
pub const MAX_ID: usize = 32;
pub const MAX_LABEL: usize = 32;

#[derive(Clone, Debug, PartialEq)]
pub struct Board {
    /// Its name for `round.post` (board 0's may be empty).
    pub id: String,
    /// The manifest's board number: 0 for the first, then 1, 2, … in order.
    pub board: u32,
    pub label: String,
    /// Lower is better.
    pub lower: bool,
    /// The daily board.
    pub daily: bool,
}

fn order(t: &Table, i: usize) -> Result<Option<bool>, String> {
    match t.get("order") {
        None => Ok(None),
        Some(Value::String(o)) => match o.as_str() {
            "high" | "higher" => Ok(Some(false)),
            "low" | "lower" => Ok(Some(true)),
            o => Err(format!("studio.toml: scores[{i}] order = \"{o}\" (high or low)")),
        },
        Some(_) => Err(format!("studio.toml: scores[{i}] order is \"high\" or \"low\"")),
    }
}

/// studio.toml's `[[scores]]` (none without it). The rules are in the
/// module docs; a problem is an error naming the board.
pub fn parse(t: &Table) -> Result<Vec<Board>, String> {
    let Some(list) = t.get("scores") else { return Ok(Vec::new()) };
    let Some(list) = list.as_array() else { return Err("studio.toml: scores is a list of boards ([[scores]] tables)".into()) };
    if list.len() > MAX {
        return Err(format!("studio.toml: {} [[scores]] boards; a game has at most {MAX}", list.len()));
    }
    let mut out: Vec<Board> = Vec::new();
    for (i, b) in list.iter().enumerate() {
        let Some(b) = b.as_table() else { return Err(format!("studio.toml: scores[{i}] is a table (a [[scores]] board)")) };
        let daily = match b.get("daily") {
            None => false,
            Some(Value::Boolean(d)) => *d,
            Some(_) => return Err(format!("studio.toml: scores[{i}] daily is true or false")),
        };
        let id = match b.get("id") {
            None => String::new(),
            Some(Value::String(s)) => s.clone(),
            Some(_) => return Err(format!("studio.toml: scores[{i}] id is a name, like \"coins\"")),
        };
        let lower = order(b, i)?;
        if i == 0 {
            if daily {
                return Err("studio.toml: the first [[scores]] board is round.score's (board 0), which can't be the daily one: add the daily board after it".into());
            }
        } else if id.is_empty() {
            return Err(format!("studio.toml: scores[{i}] needs an id (round.post's name for it, like \"coins\"); only the first board, round.score's, goes without"));
        }
        if !id.is_empty() {
            if id.len() > MAX_ID || !id.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'-') {
                return Err(format!("studio.toml: scores[{i}] id = \"{id}\": lowercase letters, digits, _ and -, at most {MAX_ID}"));
            }
            if out.iter().any(|o| o.id == id) {
                return Err(format!("studio.toml: two [[scores]] boards are called \"{id}\""));
            }
        }
        let first = out.first().is_some_and(|o| o.lower);
        if daily {
            if out.iter().any(|o| o.daily) {
                return Err("studio.toml: one [[scores]] board can be daily = true".into());
            }
            // The daily board ranks round.score, so it's ordered as board 0 is.
            if lower.is_some_and(|l| l != first) {
                return Err(format!("studio.toml: the daily board \"{id}\" ranks round.score, so its order is board 0's: leave order out"));
            }
        }
        let label = match b.get("label") {
            None => match (i, daily) {
                (0, _) => "Score".to_string(),
                (_, true) => "Today".to_string(),
                _ => id.clone(),
            },
            Some(Value::String(s)) if !s.trim().is_empty() => s.trim().to_string(),
            Some(_) => return Err(format!("studio.toml: scores[{i}] label is the board's name, like \"Coins\"")),
        };
        out.push(Board {
            id,
            board: i as u32,
            label: label.chars().take(MAX_LABEL).collect(),
            lower: if daily { first } else { lower.unwrap_or(false) },
            daily,
        });
    }
    Ok(out)
}

/// The daily board, if the project has one.
pub fn daily(boards: &[Board]) -> Option<&Board> {
    boards.iter().find(|b| b.daily)
}

thread_local! {
    /// Whether the next round's VM is a daily round (`round.daily`): set
    /// by the engine just before it makes the VM.
    static NEXT_DAILY: Cell<bool> = const { Cell::new(false) };
}

pub fn set_next_daily(on: bool) {
    NEXT_DAILY.with(|d| d.set(on));
}

/// Whether the round being made is a daily one (read once by `Vm::new`).
pub fn take_next_daily() -> bool {
    NEXT_DAILY.with(|d| d.replace(false))
}

/// A VM's side of the boards: what `round.post` kept this round.
#[derive(Clone, Debug, Default)]
pub struct RoundBoards {
    /// The title card's attract mode: posts and `round.board` do nothing.
    pub title: bool,
    /// A daily round (`round.daily`).
    pub daily: bool,
    /// Each board's best this round, in the order first posted.
    pub best: Vec<(u32, i64)>,
    /// The round's posts went up (its game-over card is showing): later
    /// ones are ignored.
    pub sent: bool,
}

impl RoundBoards {
    /// Keeps `value` if it's the round's best on `b` so far.
    pub fn post(&mut self, b: &Board, value: i64) {
        if self.title || self.sent {
            return;
        }
        match self.best.iter_mut().find(|(n, _)| *n == b.board) {
            Some((_, v)) => {
                if if b.lower { value < *v } else { value > *v } {
                    *v = value;
                }
            }
            None => self.best.push((b.board, value)),
        }
    }

    /// Submits the round's bests, once.
    pub fn send(&mut self) {
        if self.title || self.sent {
            return;
        }
        self.sent = true;
        for (b, v) in self.best.drain(..) {
            store::submit_score(b, v);
        }
    }
}

/// DAILY on the title card and the game-over card, and SCORES beside it
/// on the game-over card, for a project with a daily board.
pub struct DailyUi {
    pub board: u32,
    pub label: String,
    daily: Button,
    scores: Button,
}

/// The two buttons' places at an area's bottom edges: `layout.card` on the
/// title, `layout.results` on the game-over card (the kits' `button_row`).
fn row(area: Rect) -> [Rect; 2] {
    [Rect::new(area.x, area.bottom() - 46.0, 112.0, 44.0), Rect::new(area.right() - 112.0, area.bottom() - 46.0, 112.0, 44.0)]
}

impl DailyUi {
    pub fn new(b: &Board) -> DailyUi {
        DailyUi {
            board: b.board,
            label: b.label.to_uppercase(),
            daily: Button::new(Rect::new(0.0, 0.0, 112.0, 44.0), "DAILY").kind(ButtonKind::Secondary).with_icon(Icon::STAR),
            scores: Button::new(Rect::new(0.0, 0.0, 112.0, 44.0), "SCORES").kind(ButtonKind::Secondary).with_icon(Icon::TROPHY),
        }
    }

    /// Offers an event to the buttons before the round flow sees it; true
    /// if one took it. On the game-over card they wait out its guard, as
    /// its own buttons do.
    pub fn input(&mut self, e: &Event, rounds: &mut Rounds) -> bool {
        match rounds.stage() {
            Stage::Playing | Stage::Ending => false,
            Stage::Title => match self.daily.handle(e) {
                Some(Tap::Clicked) => {
                    rounds.set_daily(!rounds.is_daily());
                    true
                }
                Some(_) => true,
                None => false,
            },
            Stage::Over => {
                if !rounds.card_ready() {
                    return false;
                }
                if let Some(t) = self.daily.handle(e) {
                    if t == Tap::Clicked {
                        // Back to the title, in the other mode.
                        rounds.set_daily(!rounds.is_daily());
                        self.scores.reset();
                    }
                    return true;
                }
                if let Some(t) = self.scores.handle(e) {
                    if t == Tap::Clicked {
                        store::show_scores(if rounds.is_daily() { self.board } else { 0 });
                    }
                    return true;
                }
                false
            }
        }
    }

    /// Places and animates the buttons for where the round is.
    pub fn update(&mut self, dt: f32, rounds: &Rounds) {
        let l = &rounds.layout;
        let r = if rounds.stage() == Stage::Over { row(l.results) } else { row(l.card) };
        self.daily.rect = r[1];
        self.scores.rect = r[0];
        self.daily.kind = if rounds.is_daily() { ButtonKind::Primary } else { ButtonKind::Secondary };
        if rounds.playing() {
            self.daily.reset();
            self.scores.reset();
        }
        self.daily.update(dt);
        self.scores.update(dt);
    }

    /// Over the cards: DAILY on both, SCORES on the game-over card; in a
    /// daily round, a DAILY pill under the pause pill. They sit over the
    /// game's attract mode and ending, not on a panel, so a secondary
    /// button gets the identity's panel behind it (its own secondary is a
    /// faint wash meant for panels, which painted art swallowed).
    pub fn draw(&self, rounds: &Rounds, theme: &Theme) {
        let theme = &backed(theme);
        match rounds.stage() {
            Stage::Title => self.daily.draw(theme),
            Stage::Over => {
                self.daily.draw(theme);
                self.scores.draw(theme);
            }
            Stage::Playing | Stage::Ending if rounds.is_daily() => {
                let z = rounds.layout.pill_zone();
                maimbrain::ui::pill("DAILY", z.x + 52.0, z.bottom() + 18.0, 22.0, maimbrain::ui::with_alpha(theme.outline, 0.5), 0xffffffff, theme.body_font, Some(Icon::STAR));
            }
            _ => {}
        }
    }
}

/// The card buttons' theme: a secondary one on the identity's panel colour,
/// its label in a text colour that reads on it.
pub fn backed(theme: &Theme) -> Theme {
    let panel = maimbrain::ui::with_alpha(theme.panel_top, 0.92);
    let on = maimbrain::ui::color::readable(theme.text, theme.panel_top | 0xff, 4.5);
    Theme { secondary: panel, on_secondary: on, ..*theme }
}

/// The layout the buttons use, for tests.
pub fn title_row(l: &Layout) -> [Rect; 2] {
    row(l.card)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boards(toml: &str) -> Result<Vec<Board>, String> {
        parse(&toml.parse::<Table>().unwrap())
    }

    /// The card buttons read over any art: a solid panel under a label that reads on it.
    #[test]
    fn the_card_buttons_are_backed_by_the_panel() {
        for name in ["storybook", "neon", "comic", "bubblegum", "saloon", "riso", "crt", "swiss", "gothic", "field"] {
            let t = backed(&Theme::preset(name));
            assert!(t.secondary & 0xff >= 0xe0, "{name}: {:08x}", t.secondary);
            assert!(maimbrain::ui::contrast(t.on_secondary, t.secondary | 0xff) >= 4.5, "{name}");
        }
    }

    #[test]
    fn boards_are_numbered_in_order_after_round_scores() {
        let b = boards("[[scores]]\nlabel = \"Lanterns\"\norder = \"low\"\n[[scores]]\nid = \"coins\"\nlabel = \"Coins\"\n[[scores]]\nid = \"daily\"\ndaily = true\n[[scores]]\nid = \"runs\"\norder = \"low\"\n").unwrap();
        let got: Vec<(&str, u32, &str, bool, bool)> = b.iter().map(|b| (b.id.as_str(), b.board, b.label.as_str(), b.lower, b.daily)).collect();
        assert_eq!(got, [("", 0, "Lanterns", true, false), ("coins", 1, "Coins", false, false), ("daily", 2, "Today", true, true), ("runs", 3, "runs", true, false)]);
        assert!(boards("").unwrap().is_empty());
        assert_eq!(boards("[[scores]]\n").unwrap()[0].label, "Score");
    }

    #[test]
    fn what_a_board_list_cant_be() {
        for (toml, says) in [
            ("[[scores]]\nid = \"d\"\ndaily = true\n", "can't be the daily one"),
            ("[[scores]]\n[[scores]]\nlabel = \"Coins\"\n", "needs an id"),
            ("[[scores]]\n[[scores]]\nid = \"Coins!\"\n", "lowercase letters"),
            ("[[scores]]\n[[scores]]\nid = \"a\"\n[[scores]]\nid = \"a\"\n", "two [[scores]] boards are called \"a\""),
            ("[[scores]]\n[[scores]]\nid = \"a\"\ndaily = true\n[[scores]]\nid = \"b\"\ndaily = true\n", "one [[scores]] board can be daily"),
            ("[[scores]]\n[[scores]]\nid = \"a\"\ndaily = true\norder = \"low\"\n", "its order is board 0's"),
            ("[[scores]]\norder = \"sideways\"\n", "(high or low)"),
            ("scores = 3\n", "a list of boards"),
        ] {
            let e = boards(toml).expect_err(toml);
            assert!(e.contains(says), "{toml}: {e}");
        }
        let many: String = (0..17).map(|i| if i == 0 { "[[scores]]\n".to_string() } else { format!("[[scores]]\nid = \"b{i}\"\n") }).collect();
        assert!(boards(&many).unwrap_err().contains("at most 16"));
    }

    #[test]
    fn a_round_keeps_each_boards_best_and_sends_it_once() {
        let coins = Board { id: "coins".into(), board: 1, label: "Coins".into(), lower: false, daily: false };
        let time = Board { id: "time".into(), board: 2, label: "Time".into(), lower: true, daily: false };
        let mut r = RoundBoards::default();
        for v in [3, 9, 4] {
            r.post(&coins, v);
        }
        for v in [40, 31, 35] {
            r.post(&time, v);
        }
        assert_eq!(r.best, [(1, 9), (2, 31)]);
        let mut title = RoundBoards { title: true, ..Default::default() };
        title.post(&coins, 5);
        assert!(title.best.is_empty(), "the attract mode posts nothing");
        let host = maimbrain::sys::test::Host::start(1);
        r.send();
        r.send();
        r.post(&coins, 99);
        assert_eq!(host.scores(), [(1, 9), (2, 31)]);
    }
}
