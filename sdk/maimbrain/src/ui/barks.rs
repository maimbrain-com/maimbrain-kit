//! Bark pools: what a character says when something happens in play, kept
//! sparse, short and never the same line twice in a row.
//!
//! Lines go into pools by situation (a free-form tag: "taunt",
//! "part_broken", "low_health", "player_miss", "player_hit", "near_win",
//! "defeat", "gloat", whatever the game needs). The game reports what just
//! happened with [`Barks::trigger`]; the [`super::Dialogue`] that owns the
//! pools picks a line when it's allowed to speak:
//!
//! - **Event-driven.** A trigger is wanted for [`Barks::window`] (1 s) and
//!   then forgotten: a reaction that comes late is worse than none. Idle
//!   chatter is off unless [`Barks::idle`] turns it on.
//! - **One voice at a time.** Nothing starts while a bark or a cutscene line
//!   is being spoken, and two barks are at least `Dialogue::bark_cooldown`
//!   (5 s) apart.
//! - **Shuffle bags.** Each pool deals every line once, in a seeded random
//!   order, before any comes round again; and a line isn't heard again
//!   within the last [`Barks::recent`] barks (across every pool).
//! - **Cooldowns and budgets.** A pool rests [`Barks::cooldown`] (8 s) after
//!   it speaks ([`Barks::set_cooldown`] per pool), and one speaker says at
//!   most [`Barks::per_minute`] (4) barks in any minute of game time.
//! - **Priority.** [`Barks::trigger_priority`] (a defeat, a win) is never
//!   dropped: it waits until it may speak, and the speaker's budget doesn't
//!   hold it back.
//! - **Cut-in reactions.** [`Barks::react`] (the player rang the bell, "Stop
//!   that ringing!") plays in the same frame: it stops a bark or a narration
//!   line being spoken (the narration line is told again, from the top, in
//!   the next quiet stretch) and ignores the gap between barks, its pool's
//!   cooldown and the speaker's budget. Still one voice at a time: it never
//!   cuts a cutscene line or another cut-in, and two cut-ins start at least
//!   [`Barks::cut_in_gap`] (2 s) apart, so mashing doesn't make a storm.
//!
//! Everything runs on game time and a seeded [`Rng`], so it replays exactly.

use std::collections::VecDeque;

use super::dialogue::VoiceLine;
use crate::rng::Rng;
use crate::sys;

/// Seconds the per-speaker budget looks back over.
const BUDGET_SPAN: f64 = 60.0;
/// The shortest gap idle chatter may be set to.
const MIN_IDLE: f32 = 10.0;

// ---------------------------------------------------------------------------
// Scripts

/// One line of a voice script, as `mb voice script` reads it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScriptLine {
    /// The character (`assets/vo/<speaker>/…`); also the name to
    /// [`super::Dialogue::cast`] their portrait under.
    pub speaker: String,
    /// The line id (`assets/vo/<speaker>/<id>.ogg`).
    pub id: String,
    pub text: String,
    /// The `emotion` column: also the portrait expression to wear ("" neutral).
    pub expression: String,
    /// The pool: the `tag` column, or the id without its trailing number
    /// ("taunt3" → "taunt", "part_broken_2" → "part_broken").
    pub tag: String,
}

/// Parses a voice script: the `lines.csv` or `lines.toml` that
/// `mb voice script` speaks, so the lines it made can be loaded by tag.
///
/// - CSV: `character,line,text,emotion,tag`, a header row optional (with
///   one, columns go by name and may come in any order; `id` for `line`
///   and `speaker` for `character` work too), quotes around cells with
///   commas, `#` comment lines.
/// - TOML (text whose first table is `[[line]]`): `character`, `id` (or
///   `line`), `text`, `emotion`, `tag` per `[[line]]` table.
///
/// `mb voice` ignores the `tag` column, so one script serves both.
pub fn parse_script(text: &str) -> Result<Vec<ScriptLine>, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let first = text.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with('#'));
    if first.is_some_and(|l| l.starts_with('[')) { parse_toml(text) } else { parse_csv(text) }
}

fn parse_csv(text: &str) -> Result<Vec<ScriptLine>, String> {
    const POSITIONAL: [&str; 5] = ["character", "line", "text", "emotion", "tag"];
    let mut columns: Option<Vec<String>> = None;
    let mut out = Vec::new();
    let mut seen_row = false;
    for (n, row) in text.lines().enumerate() {
        if row.trim().is_empty() || row.trim_start().starts_with('#') {
            continue;
        }
        let cells: Vec<String> = csv_row(row).into_iter().map(|c| c.trim().to_string()).collect();
        if !seen_row {
            seen_row = true;
            // A header: every cell a column name, `text` among them.
            const NAMES: [&str; 11] = ["character", "speaker", "who", "line", "id", "text", "emotion", "expression", "tag", "pool", "situation"];
            let known = |c: &String| NAMES.iter().any(|n| c.eq_ignore_ascii_case(n));
            if cells.iter().all(known) && cells.iter().any(|c| c.eq_ignore_ascii_case("text")) {
                columns = Some(cells.iter().map(|c| c.to_ascii_lowercase()).collect());
                continue;
            }
        }
        if cells.len() < 3 {
            return Err(format!("line {}: want character,line,text[,emotion[,tag]]", n + 1));
        }
        let names: Vec<&str> = match &columns {
            Some(c) => c.iter().map(String::as_str).collect(),
            None => POSITIONAL.to_vec(),
        };
        let fields = names.iter().zip(&cells).map(|(k, v)| (*k, v.as_str()));
        out.push(record(fields).map_err(|e| format!("line {}: {e}", n + 1))?);
    }
    Ok(out)
}

/// A CSV row's cells (quotes around commas, `""` for a quote), as `mb voice` reads them.
fn csv_row(row: &str) -> Vec<String> {
    let mut cells = vec![String::new()];
    let mut quoted = false;
    let mut chars = row.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                chars.next();
                cells.last_mut().unwrap().push('"');
            }
            '"' => quoted = !quoted,
            ',' if !quoted => cells.push(String::new()),
            _ => cells.last_mut().unwrap().push(c),
        }
    }
    cells
}

/// The `[[line]]` tables of a TOML script (other tables are skipped; values
/// are one-line strings, or bare words and numbers).
fn parse_toml(text: &str) -> Result<Vec<ScriptLine>, String> {
    let mut out = Vec::new();
    let mut table: Option<(usize, Vec<(String, String)>)> = None;
    let flush = |table: &mut Option<(usize, Vec<(String, String)>)>, out: &mut Vec<ScriptLine>| -> Result<(), String> {
        if let Some((n, fields)) = table.take() {
            out.push(record(fields.iter().map(|(k, v)| (k.as_str(), v.as_str()))).map_err(|e| format!("[[line]] at line {n}: {e}"))?);
        }
        Ok(())
    };
    for (n, raw) in text.lines().enumerate() {
        let row = raw.trim();
        if row.is_empty() || row.starts_with('#') {
            continue;
        }
        if row.starts_with('[') {
            flush(&mut table, &mut out)?;
            let name: String = row.split('#').next().unwrap_or("").chars().filter(|c| !c.is_whitespace()).collect();
            table = (name == "[[line]]").then(|| (n + 1, Vec::new()));
            continue;
        }
        let Some((key, value)) = row.split_once('=') else {
            return Err(format!("line {}: want key = \"value\"", n + 1));
        };
        let value = toml_value(value.trim()).ok_or_else(|| format!("line {}: can't read the value {:?}", n + 1, value.trim()))?;
        if let Some((_, fields)) = &mut table {
            fields.push((key.trim().trim_matches('"').to_ascii_lowercase(), value));
        }
    }
    flush(&mut table, &mut out)?;
    Ok(out)
}

/// A one-line TOML value as text: a basic or literal string, or a bare
/// word or number up to a comment.
fn toml_value(v: &str) -> Option<String> {
    let mut chars = v.chars();
    let (s, rest) = match chars.next()? {
        '"' => {
            let mut s = String::new();
            loop {
                match chars.next()? {
                    '"' => break,
                    '\\' => s.push(match chars.next()? {
                        'n' => '\n',
                        't' => '\t',
                        'r' => '\r',
                        'u' => {
                            let hex: String = chars.by_ref().take(4).collect();
                            char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?
                        }
                        c @ ('"' | '\\') => c,
                        _ => return None,
                    }),
                    c => s.push(c),
                }
            }
            (s, chars.as_str())
        }
        '\'' => {
            let body = chars.as_str();
            let end = body.find('\'')?;
            (body[..end].to_string(), &body[end + 1..])
        }
        _ => return Some(v.split('#').next().unwrap_or("").trim().to_string()),
    };
    let rest = rest.trim();
    (rest.is_empty() || rest.starts_with('#')).then_some(s)
}

/// A script line from its named fields.
fn record<'a>(fields: impl Iterator<Item = (&'a str, &'a str)>) -> Result<ScriptLine, String> {
    let mut l = ScriptLine::default();
    for (k, v) in fields {
        let v = v.trim().to_string();
        match k {
            "character" | "speaker" | "who" => l.speaker = v,
            "line" | "id" => l.id = v,
            "text" => l.text = v,
            "emotion" | "expression" => l.expression = v,
            "tag" | "pool" | "situation" => l.tag = v,
            _ => {}
        }
    }
    for (name, v) in [("character", &l.speaker), ("line id", &l.id), ("text", &l.text)] {
        if v.is_empty() {
            return Err(format!("no {name}"));
        }
    }
    if l.tag.is_empty() {
        l.tag = tag_of(&l.id);
    }
    Ok(l)
}

/// A line id's pool: the id without a trailing number ("taunt3" → "taunt").
pub fn tag_of(id: &str) -> String {
    let t = id.trim_end_matches(|c: char| c.is_ascii_digit()).trim_end_matches(['_', '-']);
    if t.is_empty() { id.to_string() } else { t.to_string() }
}

// ---------------------------------------------------------------------------
// Barks

/// A bark picked to be spoken.
#[derive(Clone, Debug)]
pub struct BarkLine {
    pub speaker: String,
    pub line: VoiceLine,
    pub expression: String,
}

#[derive(Clone, Debug)]
struct Pool {
    tag: String,
    lines: Vec<BarkLine>,
    /// Indices into `lines` not dealt yet this round.
    bag: Vec<usize>,
    cooldown: Option<f32>,
    last: Option<f64>,
}

#[derive(Clone, Debug)]
struct Request {
    tag: String,
    at: f64,
    priority: bool,
    cut_in: bool,
}

/// Situation-tagged bark pools with shuffle bags, a recency window,
/// cooldowns, a per-speaker budget and priority barks (module docs). A
/// [`super::Dialogue`] owns one as `dialogue.barks` and speaks what it picks.
#[derive(Clone, Debug)]
pub struct Barks {
    /// A line isn't heard again within this many barks, across every pool
    /// (unless its pool has nothing else left). Default 3.
    pub recent: usize,
    /// Seconds a [`Barks::trigger`] stays wanted; a bark that can't start by
    /// then is dropped. Default 1.
    pub window: f32,
    /// Most barks one speaker says in any 60 s of game time. Priority barks
    /// aren't held back by it, but count. Default 4.
    pub per_minute: u32,
    /// Seconds a pool rests after it speaks, unless [`Barks::set_cooldown`]
    /// gave it its own. Default 8.
    pub cooldown: f32,
    /// Seconds between the starts of two cut-ins ([`Barks::react`]), so a
    /// player mashing the thing a character reacts to doesn't get a line on
    /// every tap. Default 2.
    pub cut_in_gap: f32,
    last_cut_in: Option<f64>,
    idle: Option<(String, f32)>,
    pools: Vec<Pool>,
    pending: Vec<Request>,
    /// Keys of the lines heard, newest last.
    history: VecDeque<usize>,
    /// (speaker, game time) of each bark in the last minute.
    said: VecDeque<(String, f64)>,
    quiet_since: Option<f64>,
    rng: Rng,
}

impl Default for Barks {
    fn default() -> Self {
        Barks::new(0)
    }
}

impl Barks {
    /// No pools yet, shuffled from `seed` (seed it from
    /// [`crate::sys::rand_seed`] so replays pick the same lines; a
    /// `Dialogue` does).
    pub fn new(seed: u64) -> Barks {
        Barks {
            recent: 3,
            window: 1.0,
            per_minute: 4,
            cooldown: 8.0,
            cut_in_gap: 2.0,
            last_cut_in: None,
            idle: None,
            pools: Vec::new(),
            pending: Vec::new(),
            history: VecDeque::new(),
            said: VecDeque::new(),
            quiet_since: None,
            rng: Rng::new(seed),
        }
    }

    fn pool_mut(&mut self, tag: &str) -> &mut Pool {
        let i = match self.pools.iter().position(|p| p.tag == tag) {
            Some(i) => i,
            None => {
                self.pools.push(Pool { tag: tag.to_string(), lines: Vec::new(), bag: Vec::new(), cooldown: None, last: None });
                self.pools.len() - 1
            }
        };
        &mut self.pools[i]
    }

    /// Adds a line to the `tag` pool: `speaker` says `line` wearing `expression`.
    pub fn add(&mut self, tag: &str, speaker: &str, line: &VoiceLine, expression: &str) -> &mut Barks {
        let p = self.pool_mut(tag);
        p.lines.push(BarkLine { speaker: speaker.to_string(), line: line.clone(), expression: expression.to_string() });
        p.bag.clear();
        self
    }

    /// Adds every line of a voice script ([`parse_script`]), each loading
    /// from `assets/vo/<character>/<id>`, to the pool of its tag. Returns how
    /// many. Embed the script: `include_str!("../art/lines.csv")`. A line
    /// whose files are missing shows its script text as the subtitle
    /// ([`VoiceLine::text_if_missing`]).
    pub fn load_script(&mut self, text: &str) -> Result<usize, String> {
        let lines = parse_script(text)?;
        for l in &lines {
            let line = VoiceLine::load(&format!("assets/vo/{}/{}", l.speaker, l.id)).text_if_missing(&l.text);
            self.add(&l.tag, &l.speaker, &line, &l.expression);
        }
        Ok(lines.len())
    }

    /// Seconds the `tag` pool rests after it speaks.
    pub fn set_cooldown(&mut self, tag: &str, seconds: f32) -> &mut Barks {
        self.pool_mut(tag).cooldown = Some(seconds);
        self
    }

    /// Turns on idle chatter: after `every` seconds (at least 10) with no
    /// bark, a line from the `tag` pool. Off by default; in a fight,
    /// reactions to what just happened are what land.
    pub fn idle(&mut self, tag: &str, every: f32) -> &mut Barks {
        self.idle = Some((tag.to_string(), every.max(MIN_IDLE)));
        self
    }

    pub fn idle_off(&mut self) -> &mut Barks {
        self.idle = None;
        self
    }

    /// How many lines the `tag` pool has.
    pub fn lines(&self, tag: &str) -> usize {
        self.pools.iter().find(|p| p.tag == tag).map_or(0, |p| p.lines.len())
    }

    /// Something happened in play: a bark from the `tag` pool, if one may
    /// start within [`Barks::window`]. Triggering a tag already waiting just
    /// renews it. False (and a warning) for a tag with no lines.
    pub fn trigger(&mut self, tag: &str) -> bool {
        self.request(tag, false, false)
    }

    /// A bark that must be heard (a defeat, a win): it waits until a bark
    /// may start, never expires, goes before ordinary ones and isn't held
    /// back by the speaker's budget.
    pub fn trigger_priority(&mut self, tag: &str) -> bool {
        self.request(tag, true, false)
    }

    /// A reaction to what the player just did, heard at once: in the same
    /// frame, it cuts in over a bark or a narration line being spoken (the
    /// narration line is told again later), ignoring the gap between barks,
    /// the pool's cooldown and the speaker's budget. It never cuts a
    /// cutscene line or another cut-in still speaking (it waits for them up
    /// to [`Barks::window`], then is dropped), and two cut-ins start at least
    /// [`Barks::cut_in_gap`] apart. For the line that answers the player's
    /// own action ("Stop that ringing!"); everything else is a
    /// [`Barks::trigger`].
    pub fn react(&mut self, tag: &str) -> bool {
        self.request(tag, false, true)
    }

    fn request(&mut self, tag: &str, priority: bool, cut_in: bool) -> bool {
        if self.lines(tag) == 0 {
            sys::log(sys::Level::Warn, &format!("Barks: no lines tagged {tag:?}"));
            return false;
        }
        let at = sys::time();
        match self.pending.iter_mut().find(|r| r.tag == tag && r.priority == priority && r.cut_in == cut_in) {
            Some(r) if !priority => r.at = at,
            _ => self.pending.push(Request { tag: tag.to_string(), at, priority, cut_in }),
        }
        true
    }

    /// Barks waiting to start.
    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    /// Forgets every waiting bark (priority ones too): say, at the end of a round.
    pub fn clear(&mut self) {
        self.pending.clear();
    }

    /// The next bark to speak at game time `now`, if one is wanted and
    /// `free` (nothing being spoken, the gap since the last bark passed).
    /// [`super::Dialogue::update`] calls it; call it yourself only to show
    /// barks your own way.
    pub fn take(&mut self, now: f64, free: bool) -> Option<BarkLine> {
        while self.said.front().is_some_and(|(_, t)| now - t >= BUDGET_SPAN) {
            self.said.pop_front();
        }
        let window = self.window as f64;
        self.pending.retain(|r| r.priority || now - r.at <= window);
        let quiet_since = *self.quiet_since.get_or_insert(now);
        if let Some((tag, every)) = &self.idle
            && self.pending.is_empty()
            && now - quiet_since >= *every as f64
            && self.lines(tag) > 0
        {
            self.pending.push(Request { tag: tag.clone(), at: now, priority: false, cut_in: false });
        }
        if !free {
            return None;
        }
        let mut order: Vec<usize> = (0..self.pending.len()).filter(|&i| !self.pending[i].cut_in).collect();
        order.sort_by_key(|&i| !self.pending[i].priority);
        for i in order {
            let (tag, priority) = (self.pending[i].tag.clone(), self.pending[i].priority);
            let Some(p) = self.pools.iter().position(|p| p.tag == tag) else { continue };
            let rest = self.pools[p].cooldown.unwrap_or(self.cooldown) as f64;
            if self.pools[p].last.is_some_and(|t| now - t < rest) {
                continue;
            }
            if let Some(bark) = self.deal(p, now, priority) {
                self.pending.remove(i);
                return Some(bark);
            }
        }
        None
    }

    /// Whether a cut-in may start at `now`: the last one started at least
    /// [`Barks::cut_in_gap`] ago.
    pub(crate) fn cut_in_rested(&self, now: f64) -> bool {
        self.last_cut_in.is_none_or(|t| now - t >= self.cut_in_gap as f64)
    }

    /// A cut-in started at `now` (picked here, or one line by hand).
    pub(crate) fn cut_in_started(&mut self, now: f64) {
        self.last_cut_in = Some(now);
        self.quiet_since = Some(now);
    }

    /// The reaction ([`Barks::react`]) to speak at game time `now`, if one
    /// is wanted and `may` (no cutscene line, no cut-in still speaking).
    /// The newest wins (older ones are dropped); the pool's cooldown and the speaker's budget don't
    /// hold it back. [`super::Dialogue::update`] calls it before
    /// [`Barks::take`].
    pub fn take_cut_in(&mut self, now: f64, may: bool) -> Option<BarkLine> {
        let window = self.window as f64;
        self.pending.retain(|r| !r.cut_in || now - r.at <= window);
        if !may || !self.cut_in_rested(now) {
            return None;
        }
        let i = (0..self.pending.len()).filter(|&i| self.pending[i].cut_in).max_by(|&a, &b| self.pending[a].at.total_cmp(&self.pending[b].at))?;
        let tag = self.pending.remove(i).tag;
        // The newest reaction answers the latest action; older ones are moot.
        self.pending.retain(|r| !r.cut_in);
        let p = self.pools.iter().position(|p| p.tag == tag)?;
        let bark = self.deal(p, now, true)?;
        self.cut_in_started(now);
        Some(bark)
    }

    /// Deals the next line of pool `p`: in its bag's order, the first that
    /// wasn't heard lately and whose speaker has budget left (failing that,
    /// the one heard longest ago).
    fn deal(&mut self, p: usize, now: f64, priority: bool) -> Option<BarkLine> {
        if self.pools[p].bag.is_empty() {
            self.refill(p);
        }
        let mut pos = self.choose(p, priority);
        // Everything left this round was just heard (a line shared with
        // another pool, or a pool barely bigger than the window): start the
        // next round early rather than repeat it.
        if pos.is_none_or(|(_, age)| age < self.recent) && self.pools[p].bag.len() < self.pools[p].lines.len() {
            let left = std::mem::take(&mut self.pools[p].bag);
            self.refill(p);
            let again = self.choose(p, priority);
            if again.map(|a| a.1) > pos.map(|a| a.1) {
                pos = again;
            } else {
                self.pools[p].bag = left;
            }
        }
        let (pos, _) = pos?;
        let pool = &mut self.pools[p];
        let i = pool.bag.remove(pos);
        pool.last = Some(now);
        let bark = pool.lines[i].clone();
        self.history.push_back(bark.line.key());
        while self.history.len() > self.recent.max(1) {
            self.history.pop_front();
        }
        self.said.push_back((bark.speaker.clone(), now));
        self.quiet_since = Some(now);
        Some(bark)
    }

    /// A new round for pool `p`: every line, shuffled.
    fn refill(&mut self, p: usize) {
        let n = self.pools[p].lines.len();
        let mut bag: Vec<usize> = (0..n).collect();
        for i in (1..n).rev() {
            bag.swap(i, self.rng.next_u32() as usize % (i + 1));
        }
        self.pools[p].bag = bag;
    }

    /// The best place in pool `p`'s bag and how many barks ago its line was
    /// heard (capped at `recent`: further back is as good as never).
    fn choose(&self, p: usize, priority: bool) -> Option<(usize, usize)> {
        let pool = &self.pools[p];
        let budget = |speaker: &str| priority || (self.said.iter().filter(|(s, _)| s == speaker).count() as u32) < self.per_minute;
        let age = |i: usize| {
            let key = pool.lines[i].line.key();
            self.history.iter().rev().position(|&k| k == key).unwrap_or(usize::MAX).min(self.recent)
        };
        pool.bag
            .iter()
            .enumerate()
            .filter(|&(_, &i)| budget(&pool.lines[i].speaker))
            .map(|(pos, &i)| (pos, age(i)))
            .max_by_key(|&(pos, age)| (age, std::cmp::Reverse(pos)))
    }
}

#[cfg(test)]
mod tests {
    use super::super::dialogue::{Dialogue, Lips};
    use super::super::theme::Theme;
    use super::*;
    use crate::audio::Sound;

    fn sub(text: &str) -> VoiceLine {
        VoiceLine::subtitle(text)
    }

    /// A pool "hit" of `n` lines by "Rocko", no cooldown, no budget limit.
    fn pool(n: usize) -> (Barks, Vec<String>) {
        let mut b = Barks::new(42);
        b.cooldown = 0.0;
        b.per_minute = 1000;
        let names: Vec<String> = (0..n).map(|i| format!("line {i}")).collect();
        for t in &names {
            b.add("hit", "Rocko", &sub(t), "");
        }
        (b, names)
    }

    fn deal(b: &mut Barks, n: usize) -> Vec<String> {
        sys::test::reset();
        (0..n)
            .map(|i| {
                sys::test::set_time(i as f64);
                assert!(b.trigger("hit"));
                b.take(i as f64, true).unwrap().line.text()
            })
            .collect()
    }

    #[test]
    fn a_shuffle_bag_deals_every_line_before_repeating() {
        let (mut b, names) = pool(5);
        b.recent = 0;
        let got = deal(&mut b, 15);
        for round in got.chunks(5) {
            let mut r = round.to_vec();
            r.sort();
            assert_eq!(r, names, "each round of five has every line once");
        }
        assert_ne!(got[..5], got[5..10], "and each round is shuffled anew");
        // Seeded: the same seed deals the same order.
        let (mut again, _) = pool(5);
        again.recent = 0;
        assert_eq!(got, deal(&mut again, 15));
    }

    #[test]
    fn the_recency_window_holds_across_rounds() {
        let (mut b, _) = pool(4);
        b.recent = 3;
        let got = deal(&mut b, 200);
        for (i, w) in got.windows(4).enumerate() {
            let mut w = w.to_vec();
            w.sort();
            w.dedup();
            assert_eq!(w.len(), 4, "no line within 3 barks of itself at {i}: {got:?}");
        }
        // Across pools: a line shared by two pools isn't heard twice running.
        let mut b = Barks::new(1);
        b.cooldown = 0.0;
        b.per_minute = 1000;
        let ha = sub("Ha!");
        b.add("hit", "Rocko", &ha, "").add("hit", "Rocko", &sub("Ow!"), "");
        b.add("miss", "Rocko", &ha, "").add("miss", "Rocko", &sub("Missed!"), "");
        sys::test::reset();
        let mut last = String::new();
        for i in 0..40 {
            let tag = if i % 2 == 0 { "hit" } else { "miss" };
            sys::test::set_time(i as f64);
            b.trigger(tag);
            let got = b.take(i as f64, true).unwrap().line.text();
            assert!(!(got == "Ha!" && last == "Ha!"), "Ha! twice in a row at {i}");
            last = got;
        }
    }

    #[test]
    fn a_pool_cooldown_blocks_then_releases() {
        let (mut b, _) = pool(3);
        b.set_cooldown("hit", 2.0);
        sys::test::reset();
        b.trigger("hit");
        assert!(b.take(0.0, true).is_some());
        sys::test::set_time(1.0);
        b.trigger("hit");
        assert!(b.take(1.0, true).is_none(), "cooling down");
        assert!(b.take(1.9, true).is_none());
        assert!(b.take(2.05, true).is_none(), "the event went stale (window 1 s) while it waited");
        sys::test::set_time(2.1);
        b.trigger("hit");
        assert!(b.take(2.1, true).is_some(), "rested");
        // The default cooldown is 8 s for pools without their own.
        let mut b = Barks::new(3);
        b.add("taunt", "Rocko", &sub("a"), "").add("taunt", "Rocko", &sub("b"), "");
        sys::test::reset();
        b.trigger_priority("taunt");
        assert!(b.take(0.0, true).is_some());
        b.trigger_priority("taunt");
        assert!(b.take(7.9, true).is_none());
        assert!(b.take(8.0, true).is_some(), "a waiting priority bark goes once the pool has rested");
    }

    #[test]
    fn event_barks_expire_and_priority_ones_wait() {
        let (mut b, _) = pool(3);
        sys::test::reset();
        b.trigger("hit");
        b.trigger_priority("hit");
        assert_eq!(b.pending(), 2);
        assert!(b.take(0.5, false).is_none(), "something is speaking");
        assert!(b.take(1.5, false).is_none());
        assert_eq!(b.pending(), 1, "the ordinary event went stale");
        assert!(b.take(30.0, false).is_none());
        assert!(b.take(30.0, true).is_some(), "the priority one waited 30 s and was heard");
        assert_eq!(b.pending(), 0);
        // Unknown tags are refused.
        assert!(!b.trigger("nope"));
        // Re-triggering a waiting event renews it rather than piling up.
        sys::test::set_time(40.0);
        b.trigger("hit");
        sys::test::set_time(40.8);
        b.trigger("hit");
        assert_eq!(b.pending(), 1);
        assert!(b.take(41.5, false).is_none());
        assert_eq!(b.pending(), 1, "still fresh from 40.8");
    }

    #[test]
    fn each_speaker_has_a_budget_per_minute() {
        let mut b = Barks::new(9);
        b.cooldown = 0.0;
        b.per_minute = 2;
        for i in 0..4 {
            b.add("hit", "Rocko", &sub(&format!("r{i}")), "");
            b.add("granny", "Granny", &sub(&format!("g{i}")), "");
        }
        sys::test::reset();
        let mut rocko = Vec::new();
        for i in 0..120 {
            let t = i as f64;
            sys::test::set_time(t);
            b.trigger("hit");
            if b.take(t, true).is_some() {
                rocko.push(t);
            }
        }
        assert_eq!(rocko, [0.0, 1.0, 60.0, 61.0], "two a minute, sliding");
        // Another speaker has a budget of their own.
        b.trigger("granny");
        assert_eq!(b.take(119.5, true).unwrap().speaker, "Granny");
        // Priority barks go over budget.
        b.trigger("hit");
        b.trigger_priority("hit");
        assert_eq!(b.take(121.5, true).unwrap().speaker, "Rocko");
    }

    #[test]
    fn a_reaction_skips_the_cooldowns_but_keeps_its_own_gap() {
        let mut b = Barks::new(4);
        b.per_minute = 1;
        b.add("bell", "Mother", &sub("Stop that ringing!"), "").add("bell", "Mother", &sub("Enough!"), "");
        sys::test::reset();
        b.trigger("bell");
        assert!(b.take(0.0, true).is_some());
        // The pool rests 8 s and Mother's budget is spent, but a reaction goes now.
        sys::test::set_time(0.5);
        b.react("bell");
        assert!(b.take(0.5, true).is_none(), "ordinary barks don't take reactions");
        assert!(b.take_cut_in(0.5, false).is_none(), "not while it may not cut in");
        assert!(b.take_cut_in(0.6, true).is_some());
        // Two reactions start at least cut_in_gap (2 s) apart; a stale one is dropped.
        sys::test::set_time(1.0);
        b.react("bell");
        assert!(b.take_cut_in(1.0, true).is_none());
        assert!(b.take_cut_in(2.1, true).is_none() && b.pending() == 0, "went stale waiting");
        sys::test::set_time(2.7);
        b.react("bell");
        assert!(b.take_cut_in(2.7, true).is_some());
    }

    #[test]
    fn idle_chatter_is_off_unless_asked_and_rate_limited() {
        let mut b = Barks::new(5);
        b.add("idle", "Rocko", &sub("Hum."), "").add("idle", "Rocko", &sub("Bored."), "");
        sys::test::reset();
        assert!((0..600).all(|i| b.take(i as f64 * 0.1, true).is_none()), "off by default");
        b.idle("idle", 2.0);
        b.cooldown = 0.0;
        let mut fired = 0;
        for i in 600..1800 {
            fired += b.take(i as f64 * 0.1, true).is_some() as u32;
        }
        assert!((1..=12).contains(&fired), "at most one every 10 s (asked for 2): {fired}");
    }

    #[test]
    fn the_script_loader_reads_mb_voice_scripts() {
        let csv = "# rocko's lines\ncharacter,line,text,emotion,tag\nrocko,t1,\"Ha, too slow!\",smug,taunt\nrocko,ouch2,Not the paint job!,angry,part_broken\ngranny,gloat1,Back in my day…,,\n";
        let l = parse_script(csv).unwrap();
        assert_eq!(l.len(), 3);
        assert_eq!(l[0], ScriptLine { speaker: "rocko".into(), id: "t1".into(), text: "Ha, too slow!".into(), expression: "smug".into(), tag: "taunt".into() });
        assert_eq!(l[1].tag, "part_broken");
        assert_eq!((l[2].tag.as_str(), l[2].expression.as_str()), ("gloat", ""), "no tag: the id without its number");
        // Headerless, as mb voice also reads it; columns by name in any order.
        let l = parse_script("rocko,low_health_2,I'm hit!,scared\n").unwrap();
        assert_eq!((l[0].tag.as_str(), l[0].expression.as_str()), ("low_health", "scared"));
        let l = parse_script("tag,text,id,speaker\nnear_win,One more!,nw,rocko\n").unwrap();
        assert_eq!((l[0].speaker.as_str(), l[0].id.as_str(), l[0].tag.as_str()), ("rocko", "nw", "near_win"));
        assert!(parse_script("rocko,t1\n").is_err());
        let toml = "# the duel\n[[line]]\ncharacter = \"rocko\"\nid = \"taunt1\"\ntext = \"Is that all you got?\" # a comment\nemotion = 'smug'\n\n[[line]]\ncharacter = \"granny\"\nline = \"defeat1\"\ntext = \"Say \\\"uncle\\\"!\"\ntag = \"defeat\"\n";
        let l = parse_script(toml).unwrap();
        assert_eq!(l.len(), 2);
        assert_eq!((l[0].tag.as_str(), l[0].text.as_str(), l[0].expression.as_str()), ("taunt", "Is that all you got?", "smug"));
        assert_eq!((l[1].id.as_str(), l[1].text.as_str(), l[1].tag.as_str()), ("defeat1", "Say \"uncle\"!", "defeat"));
        assert!(parse_script("[[line]]\ncharacter = \"rocko\"\n").is_err(), "no id or text");
        // Loading makes the pools.
        let mut b = Barks::new(0);
        assert_eq!(b.load_script(csv).unwrap(), 3);
        assert_eq!((b.lines("taunt"), b.lines("part_broken"), b.lines("gloat")), (1, 1, 1));
    }

    fn voiced(text: &str, dur: f32) -> VoiceLine {
        VoiceLine::new(Some(Sound(1)), Lips::parse(format!("mblips 1\ndur {dur}\ntext {text}\n").as_bytes()).unwrap())
    }

    /// Which barks have started, and when.
    #[derive(Default)]
    struct Watch {
        starts: Vec<f64>,
        last: Option<String>,
    }

    fn run(d: &mut Dialogue, seconds: f32, w: &mut Watch) {
        for _ in 0..(seconds * 60.0).round() as usize {
            sys::test::advance(1.0 / 60.0);
            d.update(1.0 / 60.0);
            let now = d.current_bark().map(|s| s.text);
            if now.is_some() && now != w.last {
                w.starts.push(sys::time());
            }
            w.last = now;
        }
    }

    #[test]
    fn the_dialogue_speaks_one_voice_at_a_time_with_a_gap() {
        sys::test::reset();
        let mut d = Dialogue::new(Theme::default());
        assert_eq!(d.bark_cooldown, 5.0, "five seconds between barks by default");
        d.barks.cooldown = 0.0;
        for i in 0..4 {
            d.barks.add("hit", "Rocko", &voiced(&format!("Hit {i}"), 1.2), "");
        }
        d.barks.add("defeat", "Rocko", &voiced("Nooo!", 1.2), "");
        let w = &mut Watch::default();
        d.barks.trigger("hit");
        run(&mut d, 0.1, w);
        assert!(d.current_bark().is_some(), "an event bark starts at once");
        // An event 2 s later, inside the gap: dropped.
        run(&mut d, 2.0, w);
        d.barks.trigger("hit");
        run(&mut d, 1.5, w);
        assert_eq!(d.barks.pending(), 0, "stale after its window");
        // A priority bark inside the gap waits for it.
        d.barks.trigger_priority("defeat");
        run(&mut d, 3.0, w);
        assert_eq!(w.starts.len(), 2, "{:?}", w.starts);
        assert!((w.starts[1] - w.starts[0] - 5.0).abs() < 0.05, "the priority bark came once the gap was up: {:?}", w.starts);
        assert_eq!(w.last.as_deref(), Some("Nooo!"));
        // Never over a cutscene line, never over another bark.
        d.say("Rocko", &voiced("Listen up.", 2.0), "");
        run(&mut d, 0.2, w);
        d.barks.trigger_priority("hit");
        run(&mut d, 1.0, w);
        assert_eq!(w.starts.len(), 2, "the cutscene holds barks back");
        assert!(!d.bark("Rocko", &voiced("Ha!", 1.0), ""));
        d.skip_all();
        run(&mut d, 2.5, w);
        assert_eq!(w.starts.len(), 3, "the waiting priority bark goes once the cutscene is over and the gap is up");
        // A bark being spoken isn't replaced by another.
        sys::test::advance(10.0);
        d.bark_cooldown = 0.0;
        assert!(d.bark("Rocko", &voiced("First", 1.0), ""));
        run(&mut d, 0.3, w);
        assert!(!d.bark("Rocko", &voiced("Second", 1.0), ""), "still speaking");
        run(&mut d, 1.0, w);
        assert!(d.bark("Rocko", &voiced("Second", 1.0), ""), "done: the next may replace its bubble");
    }
}
