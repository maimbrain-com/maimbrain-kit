//! What's in a scene, and how big things are in the world: the objects a
//! generated painting shows (so a prop doesn't stand next to the same thing
//! painted into the background), and a real-world height for each figure
//! and prop (so a mirror isn't drawn at half a child's height).
//!
//! Both come from a small table of everyday things, matched on normalized
//! words (lowercase, singular, synonyms folded: "armchairs" is a chair,
//! "a looking glass" a mirror). `mb art` records a painting's objects
//! (`result.shows` in its sidecar: what its prompt and the model's reply
//! name, plus `--shows`) and a figure's height in meters (`result.real`:
//! `--real`, else the table's for its subject).

use serde_json::{Value, json};

/// Everyday things: a name, a typical height in meters (standing, as drawn
/// in a side view), and other words for it.
const THINGS: &[(&str, f32, &[&str])] = &[
    // People and creatures.
    ("girl", 1.3, &["little girl", "daughter"]),
    ("boy", 1.3, &["little boy", "son"]),
    ("child", 1.2, &["kid", "orphan"]),
    ("baby", 0.7, &["infant", "toddler"]),
    ("woman", 1.65, &["lady", "mother", "maid", "governess", "nanny", "housekeeper"]),
    ("man", 1.78, &["gentleman", "father", "butler"]),
    ("old woman", 1.6, &["grandmother", "granny", "crone", "hag"]),
    ("old man", 1.7, &["grandfather", "grandpa"]),
    ("witch", 1.7, &["sorceress"]),
    ("ghost", 1.7, &["spirit", "phantom", "specter", "spectre", "apparition"]),
    ("knight", 1.85, &[]),
    ("soldier", 1.8, &["guard"]),
    ("skeleton", 1.75, &[]),
    ("zombie", 1.75, &[]),
    ("cat", 0.3, &["kitten", "kitty"]),
    ("dog", 0.6, &["hound", "puppy"]),
    ("rat", 0.1, &["mouse"]),
    ("bird", 0.2, &["sparrow", "songbird"]),
    ("crow", 0.35, &["raven", "rook"]),
    ("owl", 0.45, &[]),
    ("horse", 1.6, &["pony", "stallion", "mare"]),
    ("spider", 0.05, &[]),
    ("bat", 0.15, &[]),
    ("frog", 0.08, &["toad"]),
    ("moth", 0.05, &["butterfly"]),
    ("fox", 0.4, &[]),
    ("wolf", 0.8, &[]),
    ("bear", 1.2, &[]),
    // Furniture and things that stand on a floor.
    ("rocking horse", 0.9, &["hobby horse"]),
    ("chair", 0.95, &["armchair", "seat", "high chair"]),
    ("rocking chair", 1.05, &[]),
    ("stool", 0.6, &[]),
    ("bench", 0.5, &["pew"]),
    ("sofa", 0.9, &["couch", "settee", "chaise"]),
    ("table", 0.75, &["side table", "nightstand"]),
    ("desk", 0.75, &["writing desk", "bureau"]),
    ("bed", 1.0, &["four-poster", "four poster"]),
    ("cradle", 0.9, &["crib", "bassinet"]),
    ("wardrobe", 2.0, &["armoire"]),
    ("cupboard", 1.8, &["cabinet", "dresser", "chest of drawers", "sideboard"]),
    ("bookcase", 2.0, &["bookshelf", "shelf"]),
    ("chest", 0.6, &["trunk", "toy chest", "footlocker"]),
    ("mirror", 1.6, &["looking glass", "standing mirror", "cheval glass"]),
    ("grandfather clock", 2.0, &["longcase clock", "tall clock"]),
    ("clock", 0.3, &["mantel clock", "wall clock"]),
    ("piano", 1.3, &["harpsichord"]),
    ("fireplace", 1.2, &["hearth", "mantel", "mantelpiece"]),
    ("door", 2.1, &["doorway"]),
    ("window", 1.4, &[]),
    ("curtain", 2.2, &["drape"]),
    ("rug", 0.02, &["carpet"]),
    ("lamp", 1.5, &["floor lamp", "oil lamp"]),
    ("lantern", 0.4, &[]),
    ("candle", 0.25, &["taper"]),
    ("candlestick", 0.4, &["candelabra", "candelabrum", "candle holder", "sconce"]),
    ("chandelier", 1.0, &[]),
    ("painting", 0.8, &["portrait", "picture", "framed picture"]),
    ("vase", 0.4, &["urn"]),
    ("statue", 1.8, &["sculpture"]),
    ("bust", 0.6, &[]),
    ("suit of armor", 1.9, &["suit of armour", "armor", "armour"]),
    ("barrel", 0.9, &["cask", "keg"]),
    ("crate", 0.6, &["box", "packing case"]),
    ("basket", 0.4, &[]),
    ("bucket", 0.35, &["pail"]),
    ("broom", 1.4, &["broomstick"]),
    ("cauldron", 0.6, &[]),
    ("doll", 0.4, &["puppet", "marionette"]),
    ("dollhouse", 0.8, &["doll house", "dolls house"]),
    ("teddy bear", 0.35, &["teddy"]),
    ("ball", 0.2, &[]),
    ("book", 0.25, &["tome", "diary", "journal"]),
    ("key", 0.08, &[]),
    ("bell", 0.15, &["handbell"]),
    ("cup", 0.1, &["mug", "teacup"]),
    ("teapot", 0.2, &["kettle"]),
    ("bottle", 0.3, &["flask", "potion"]),
    ("skull", 0.2, &[]),
    ("coffin", 0.6, &["casket"]),
    ("gravestone", 1.0, &["tombstone", "headstone", "grave"]),
    ("tree", 6.0, &[]),
    ("bush", 1.0, &["shrub", "hedge"]),
    ("rock", 0.5, &["boulder"]),
    ("fence", 1.2, &[]),
    ("well", 1.0, &[]),
    ("cart", 1.2, &["wagon", "wheelbarrow"]),
    ("ladder", 3.0, &[]),
    ("staircase", 3.0, &["stair", "stairway"]),
    ("pillar", 4.0, &["column"]),
    ("umbrella", 0.9, &["parasol"]),
    ("hat", 0.15, &[]),
    ("sword", 1.0, &[]),
    ("shield", 0.7, &[]),
    ("coin", 0.03, &[]),
    ("gem", 0.03, &["jewel", "crystal"]),
    ("flower", 0.3, &["rose", "lily"]),
    ("mushroom", 0.1, &["toadstool"]),
    ("music box", 0.15, &[]),
    ("spinning wheel", 1.0, &[]),
    ("birdcage", 0.5, &["cage", "bird cage"]),
    ("hourglass", 0.25, &[]),
    ("globe", 0.5, &[]),
    ("telescope", 1.4, &[]),
    ("gramophone", 0.5, &["phonograph"]),
    ("telephone", 0.2, &["phone"]),
    ("car", 1.5, &[]),
    ("bicycle", 1.0, &["bike"]),
    ("cobweb", 0.5, &["web"]),
];

/// Words a scene's description uses that aren't objects in it.
const NEGATIONS: &[&str] = &["no", "without", "not", "never", "nor", "except"];

/// A word made singular: horses → horse, shelves → shelf, boxes → box.
pub fn singular(w: &str) -> String {
    let irregular = [("mice", "mouse"), ("children", "child"), ("women", "woman"), ("men", "man"), ("people", "person"), ("teeth", "tooth"), ("feet", "foot"), ("geese", "goose"), ("wolves", "wolf"), ("knives", "knife"), ("leaves", "leaf")];
    if let Some((_, s)) = irregular.iter().find(|(p, _)| *p == w) {
        return s.to_string();
    }
    let n = w.len();
    if n <= 3 || w.ends_with("ss") || w.ends_with("us") || w.ends_with("is") || !w.ends_with('s') {
        return w.to_string();
    }
    if w.ends_with("ies") && n > 4 {
        return format!("{}y", &w[..n - 3]);
    }
    if w.ends_with("ves") && n > 4 {
        return format!("{}f", &w[..n - 3]);
    }
    for end in ["sses", "ches", "shes", "xes", "zes"] {
        if w.ends_with(end) {
            return w[..n - 2].to_string();
        }
    }
    w[..n - 1].to_string()
}

/// Lowercase singular words: "Two dusty Rocking-Horses" → [two, dusty, rocking, horse].
pub fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .map(|w| w.trim_matches('\'').trim_end_matches("'s").to_lowercase())
        .filter(|w| !w.is_empty())
        .map(|w| singular(&w))
        .collect()
}

/// A thing in the table, found in a text: its name, and where it ends (in words).
#[derive(Clone, Debug, PartialEq)]
struct Found {
    name: &'static str,
    end: usize,
    negated: bool,
}

/// Every thing of the table a text names, in order; the longest name wins
/// where names overlap ("rocking horse", not "horse").
fn find(text: &str) -> Vec<Found> {
    let ws = words(text);
    let mut names: Vec<(Vec<String>, &'static str)> = Vec::new();
    for (name, _, aliases) in THINGS {
        for n in std::iter::once(name).chain(aliases.iter()) {
            names.push((words(n), name));
        }
    }
    names.sort_by_key(|(w, _)| std::cmp::Reverse(w.len()));
    let mut out = Vec::new();
    let mut i = 0;
    while i < ws.len() {
        match names.iter().find(|(n, _)| ws.len() - i >= n.len() && ws[i..i + n.len()] == n[..]) {
            Some((n, name)) => {
                // "no candles", "without a mirror", "not a chair": a few words back.
                let negated = ws[i.saturating_sub(3)..i].iter().any(|w| NEGATIONS.contains(&w.as_str()));
                out.push(Found { name, end: i + n.len(), negated });
                i += n.len();
            }
            None => i += 1,
        }
    }
    out
}

/// The things a description of a painting says it shows (not what it says it
/// doesn't: "no candles"), each once, in order.
pub fn objects_in(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for f in find(text).into_iter().filter(|f| !f.negated) {
        if !out.iter().any(|o| o == f.name) {
            out.push(f.name.to_string());
        }
    }
    out
}

/// The words that start what a subject says about its subject: "a girl
/// holding a candle" is a girl.
const QUALIFIERS: &[&str] = &[
    "holding", "with", "carrying", "wearing", "in", "on", "under", "beside", "next", "near", "by", "that", "which", "who", "whose", "while", "and", "against", "behind", "lit", "from", "at", "for", "of",
];

/// The thing a sprite or frames sheet is of: the last thing its subject
/// names before what it says about it ("a wooden rocking horse that rocks"
/// is a rocking horse; "a girl holding a candle" a girl). "of" stays in a
/// name ("suit of armor") but ends the subject otherwise ("a pile of books").
pub fn subject_thing(subject: &str) -> Option<String> {
    let ws = words(subject);
    let cut = ws.iter().position(|w| QUALIFIERS.contains(&w.as_str())).unwrap_or(ws.len());
    let found = find(subject);
    // A name running past the cut that begins before it ("suit of armor") still counts.
    found.iter().rfind(|f| f.end <= cut || (f.name.contains(' ') && f.end - words(f.name).len() < cut)).or(found.first()).map(|f| f.name.to_string())
}

/// A free-form object (from `--shows`): the table's name for it when it has
/// one, else its own words.
pub fn canonical(object: &str) -> String {
    objects_in(object).into_iter().next().unwrap_or_else(|| words(object).join(" "))
}

/// Whether a painting's object is the thing a subject is of: the same thing
/// in the table, or (for an object the table doesn't know) all its words in
/// the subject's.
pub fn same_thing(object: &str, subject: &str) -> bool {
    match subject_thing(subject) {
        Some(t) if THINGS.iter().any(|(n, _, _)| *n == object) => t == object,
        _ => {
            let ow = words(object);
            let sw = words(subject);
            !ow.is_empty() && ow.iter().all(|w| sw.contains(w))
        }
    }
}

/// A typical height in meters for a subject's thing, and the thing.
pub fn typical_height(subject: &str) -> Option<(f32, String)> {
    let t = subject_thing(subject)?;
    THINGS.iter().find(|(n, _, _)| *n == t).map(|(_, h, _)| (*h, t))
}

/// `--real`: a height in meters (1.8m, 180cm, 6ft, 14in; a bare number is meters).
pub fn parse_real(s: &str) -> Result<f32, String> {
    let t = s.trim().to_lowercase();
    let bad = || format!("--real {s}: a real-world height, like 1.8m, 40cm, 6ft or 14in");
    let (num, k) = if let Some(n) = t.strip_suffix("cm") {
        (n, 0.01)
    } else if let Some(n) = t.strip_suffix("mm") {
        (n, 0.001)
    } else if let Some(n) = t.strip_suffix('m') {
        (n, 1.0)
    } else if let Some(n) = t.strip_suffix("ft") {
        (n, 0.3048)
    } else if let Some(n) = t.strip_suffix("in") {
        (n, 0.0254)
    } else {
        (t.as_str(), 1.0)
    };
    let v: f32 = num.trim().parse().map_err(|_| bad())?;
    let m = (v * k * 10000.0).round() / 10000.0;
    if !(0.005..=100.0).contains(&m) {
        return Err(bad());
    }
    Ok(m)
}

/// A figure's real height for its sidecar: `{"m": 1.6, "from": "given"}` or
/// `{"m": 1.6, "from": "mirror"}` (the table's, for that thing).
pub fn real_json(given: Option<f32>, subject: &str) -> Value {
    match (given, typical_height(subject)) {
        (Some(m), _) => json!({ "m": round3(m), "from": "given" }),
        (None, Some((m, t))) => json!({ "m": round3(m), "from": t }),
        _ => Value::Null,
    }
}

fn round3(m: f32) -> f64 {
    (m as f64 * 1000.0).round() / 1000.0
}

/// A painting's objects for its sidecar: those its prompt names, those the
/// model said it drew, and `--shows`, each once.
pub fn shows_json(subject: &str, model_text: Option<&str>, given: &[String]) -> Vec<String> {
    let mut out: Vec<String> = objects_in(subject);
    for o in model_text.map(objects_in).unwrap_or_default().into_iter().chain(given.iter().map(|g| canonical(g))) {
        if !o.is_empty() && !out.contains(&o) {
            out.push(o);
        }
    }
    out
}

/// Things smaller than this (meters) are carried, not stood beside: games
/// draw them bigger than life so they read.
pub const HANDHELD: f32 = 0.3;

/// How a figure's drawn height compares with the scene's scale (units a
/// meter): None if it's plausible, else what it should be about.
pub fn implausible(height: f32, real: f32, units_per_m: f32) -> Option<f32> {
    let want = real * units_per_m;
    if real >= HANDHELD {
        let r = height / want.max(1e-3);
        (!(0.75..=1.33).contains(&r)).then_some(want)
    } else {
        // A pickup may be drawn several times life size, but not as tall as a 0.75 m thing.
        (height > (want * 4.0).max(0.75 * units_per_m)).then_some(want.max(16.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_are_singular_and_plain() {
        assert_eq!(words("Two dusty Rocking-Horses, shelves & boxes"), ["two", "dusty", "rocking", "horse", "shelf", "box"]);
        assert_eq!(singular("glasses"), "glass");
        assert_eq!(singular("candles"), "candle");
        assert_eq!(singular("benches"), "bench");
        assert_eq!(singular("bus"), "bus");
        assert_eq!(singular("dress"), "dress");
        assert_eq!(singular("chimneys"), "chimney");
        assert_eq!(singular("mice"), "mouse");
        assert_eq!(words("the witch's broom"), ["the", "witch", "broom"]);
    }

    #[test]
    fn a_paintings_objects_come_from_its_description() {
        let attic = "a dusty attic with a rocking horse, old trunks, a looking glass and no candles — the nearest layer";
        assert_eq!(objects_in(attic), ["rocking horse", "chest", "mirror"]);
        assert_eq!(objects_in("a moonlit corridor lined with portraits and armchairs"), ["painting", "chair"]);
        assert_eq!(objects_in("a grandfather clock by the stairs"), ["grandfather clock", "staircase"]);
        assert!(objects_in("rolling hills at dusk, the farthest layer").is_empty());
        assert_eq!(shows_json("an attic with a rocking horse", Some("Here is your attic, with a dusty mirror."), &["Toy Chests".into(), "a cracked urn".into(), "weird lamp thing".into()]), [
            "rocking horse",
            "mirror",
            "chest",
            "vase",
            "lamp"
        ]);
        assert_eq!(canonical("Spooky Gargoyles"), "spooky gargoyle");
    }

    #[test]
    fn a_sprite_is_the_thing_its_subject_starts_with() {
        assert_eq!(subject_thing("a wooden rocking horse that rocks by itself").as_deref(), Some("rocking horse"));
        assert_eq!(subject_thing("a girl in a white nightgown holding a candle").as_deref(), Some("girl"));
        assert_eq!(subject_thing("a tall standing mirror with a gilt frame").as_deref(), Some("mirror"));
        assert_eq!(subject_thing("an empty suit of armor").as_deref(), Some("suit of armor"));
        assert_eq!(subject_thing("a toppled armchair").as_deref(), Some("chair"));
        assert_eq!(subject_thing("a hunched old witch creeping").as_deref(), Some("witch"));
        assert_eq!(subject_thing("a glowing orb").as_deref(), None);
        // Duplicates: the painting's rocking horse is the prop's; its mirror isn't the girl.
        assert!(same_thing("rocking horse", "a rocking horse, painted red"));
        assert!(!same_thing("horse", "a rocking horse"));
        assert!(!same_thing("candle", "a girl holding a candle"));
        assert!(same_thing("chest", "an old steamer trunk"));
        assert!(same_thing("spooky gargoyle", "a spooky stone gargoyle"));
    }

    #[test]
    fn real_heights_parse_and_come_from_the_table() {
        assert_eq!(parse_real("1.8m").unwrap(), 1.8);
        assert_eq!(parse_real("40cm").unwrap(), 0.4);
        assert!((parse_real("6ft").unwrap() - 1.8288).abs() < 1e-4);
        assert_eq!(parse_real("2").unwrap(), 2.0);
        assert!(parse_real("tall").is_err() && parse_real("0m").is_err());
        assert_eq!(typical_height("a tall standing mirror"), Some((1.6, "mirror".into())));
        assert_eq!(real_json(None, "a brass bell")["from"], "bell");
        assert_eq!(real_json(Some(1.4), "a brass bell")["from"], "given");
        assert!(real_json(None, "a glowing orb").is_null());
    }

    #[test]
    fn implausible_sizes_are_found() {
        // The girl is 60 units for 1.3 m: 46 units a meter. A 1.6 m mirror at 30 units is half what it should be.
        let k = 60.0 / 1.3;
        assert!((implausible(30.0, 1.6, k).unwrap() - 73.8).abs() < 0.1);
        assert_eq!(implausible(72.0, 1.6, k), None);
        // A bell may be drawn bigger than life, but not as tall as a chair.
        assert_eq!(implausible(18.0, 0.15, k), None);
        assert!(implausible(40.0, 0.15, k).is_some());
    }
}
