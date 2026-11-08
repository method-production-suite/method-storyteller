//! Autocomplete for character cues, scene headings and transitions.
//!
//! Suggestions are complete replacement texts for the element, best first.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::model::{ElementKind, Script, character_name};

/// Most entries kept in one list.
pub const MAX_LIST: usize = 2_000;

/// The six lists a script keeps for autocomplete: what it has learned, and what the writer added.
/// A list only adds suggestions; what the script itself uses is always offered too.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Lists {
    pub characters: Vec<String>,
    pub extensions: Vec<String>,
    pub intros: Vec<String>,
    pub locations: Vec<String>,
    pub times: Vec<String>,
    pub transitions: Vec<String>,
    /// What follows a scene intro, as `.fdx` says it (`. `). Kept for writing back.
    pub intro_separator: Option<String>,
    /// What comes before a time of day, as `.fdx` says it (` - `). Kept for writing back.
    pub time_separator: Option<String>,
}

/// The lists by name, as the commands and files call them.
pub const LIST_NAMES: [&str; 6] = ["characters", "extensions", "intros", "locations", "times", "transitions"];

impl Lists {
    pub fn is_empty(&self) -> bool {
        *self == Lists::default()
    }

    pub fn list(&self, name: &str) -> Option<&Vec<String>> {
        match name {
            "characters" => Some(&self.characters),
            "extensions" => Some(&self.extensions),
            "intros" => Some(&self.intros),
            "locations" => Some(&self.locations),
            "times" => Some(&self.times),
            "transitions" => Some(&self.transitions),
            _ => None,
        }
    }

    pub fn list_mut(&mut self, name: &str) -> Option<&mut Vec<String>> {
        match name {
            "characters" => Some(&mut self.characters),
            "extensions" => Some(&mut self.extensions),
            "intros" => Some(&mut self.intros),
            "locations" => Some(&mut self.locations),
            "times" => Some(&mut self.times),
            "transitions" => Some(&mut self.transitions),
            _ => None,
        }
    }

    /// Adds `entry` to a list, in capitals, once. Returns whether it was new.
    pub fn add(list: &mut Vec<String>, entry: &str) -> bool {
        let entry = entry.trim().to_uppercase();
        if entry.is_empty() || list.len() >= MAX_LIST || list.iter().any(|e| e.to_uppercase() == entry) {
            return false;
        }
        list.push(entry);
        true
    }

    /// Adds what `script` uses that the lists do not have yet. Returns whether anything was new.
    pub fn learn(&mut self, script: &Script) -> bool {
        // What each list holds, so a long script is read in one pass however often it repeats.
        let held = |list: &[String]| -> HashSet<String> { list.iter().map(|entry| entry.to_uppercase()).collect() };
        let mut characters = held(&self.characters);
        let mut extensions = held(&self.extensions);
        let mut intros = held(&self.intros);
        let mut locations = held(&self.locations);
        let mut times = held(&self.times);
        let mut transitions = held(&self.transitions);
        let mut new = false;
        for element in &script.elements {
            let text = element.text.trim();
            match element.kind {
                ElementKind::Character => {
                    new |= add_unheld(&mut self.characters, &mut characters, character_name(text));
                    // Each bracketed part is an extension of its own: `(V.O.) (CONT'D)` is two.
                    let mut rest = text;
                    while let Some(open) = rest.find('(') {
                        let inner = rest.get(open + 1..).unwrap_or_default();
                        let Some(close) = inner.find(')') else { break };
                        new |= add_unheld(&mut self.extensions, &mut extensions, inner.get(..close).unwrap_or_default());
                        rest = inner.get(close + 1..).unwrap_or_default();
                    }
                }
                ElementKind::SceneHeading => {
                    let upper = text.to_uppercase();
                    let (prefix, location, time) = split_heading(&upper);
                    if let Some(prefix) = prefix {
                        new |= add_unheld(&mut self.intros, &mut intros, prefix);
                        new |= add_unheld(&mut self.locations, &mut locations, location);
                        new |= time.is_some_and(|time| add_unheld(&mut self.times, &mut times, time));
                    }
                }
                ElementKind::Transition => new |= add_unheld(&mut self.transitions, &mut transitions, text),
                _ => {}
            }
        }
        new
    }

    /// Empties the lists and fills them from `script` alone, keeping the separators.
    pub fn rebuild(&mut self, script: &Script) {
        for name in LIST_NAMES {
            if let Some(list) = self.list_mut(name) {
                list.clear();
            }
        }
        self.learn(script);
    }

    /// A scene intro as it is offered: `INT` with a `. ` separator reads `INT.`.
    fn intro(&self, entry: &str) -> String {
        let entry = entry.trim().to_uppercase();
        let separator = self.intro_separator.as_deref().unwrap_or("").trim_end();
        if entry.ends_with(separator) { entry } else { format!("{entry}{separator}") }
    }
}

/// [`Lists::add`], with what the list holds already known.
fn add_unheld(list: &mut Vec<String>, held: &mut HashSet<String>, entry: &str) -> bool {
    let entry = entry.trim().to_uppercase();
    if entry.is_empty() || list.len() >= MAX_LIST || !held.insert(entry.clone()) {
        return false;
    }
    list.push(entry);
    true
}

fn weighted(list: &[String]) -> impl Iterator<Item = (String, usize)> + '_ {
    list.iter().map(|entry| (entry.trim().to_uppercase(), 1))
}

/// Most suggestions returned.
pub const MAX_SUGGESTIONS: usize = 10;

pub const SCENE_PREFIXES: [&str; 6] = ["INT.", "EXT.", "INT./EXT.", "EXT./INT.", "I/E", "EST."];
/// Times of day, in the order Final Draft's autocomplete offers them.
pub const TIMES_OF_DAY: [&str; 14] = [
    "DAY",
    "NIGHT",
    "AFTERNOON",
    "MORNING",
    "EVENING",
    "LATER",
    "MOMENTS LATER",
    "CONTINUOUS",
    "THE NEXT DAY",
    "MAGIC HOUR",
    "DAWN",
    "DUSK",
    "SAME",
    "SAME TIME",
];
pub const TRANSITIONS: [&str; 9] =
    ["CUT TO:", "FADE IN:", "FADE OUT.", "FADE TO BLACK.", "DISSOLVE TO:", "SMASH CUT TO:", "MATCH CUT TO:", "JUMP CUT TO:", "BACK TO:"];
pub const EXTENSIONS: [&str; 4] = ["V.O.", "O.S.", "O.C.", "CONT'D"];

/// Ranked, de-duplicated candidates starting with `prefix` (but not equal to it): heaviest
/// first, then in the order they were first offered (so built-in lists keep their order).
fn rank(prefix: &str, candidates: impl IntoIterator<Item = (String, usize)>) -> Vec<String> {
    let mut counts: HashMap<String, (usize, usize)> = HashMap::new();
    for (order, (candidate, weight)) in candidates.into_iter().enumerate() {
        if candidate.starts_with(prefix) && candidate != prefix && !candidate.is_empty() {
            let entry = counts.entry(candidate).or_insert((0, order));
            entry.0 += weight;
        }
    }
    let mut ranked: Vec<(String, (usize, usize))> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.0.cmp(&a.1.0).then_with(|| a.1.1.cmp(&b.1.1)));
    ranked.into_iter().take(MAX_SUGGESTIONS).map(|(text, _)| text).collect()
}

/// In a two-hander, who speaks next: the speaker before last, when the last two speeches in
/// this scene alternate between two people (Final Draft's "guess next character").
pub fn next_speaker(script: &Script, index: usize) -> Option<String> {
    let mut speakers: Vec<String> = Vec::new();
    for element in script.elements.get(..index).unwrap_or(&[]).iter().rev() {
        match element.kind {
            ElementKind::SceneHeading => break,
            ElementKind::Character => {
                speakers.push(character_name(&element.text).to_uppercase());
                if speakers.len() == 2 {
                    break;
                }
            }
            _ => {}
        }
    }
    match speakers.as_slice() {
        [last, before] if last != before && !before.is_empty() => Some(before.clone()),
        _ => None,
    }
}

/// [`suggest`] for the element at `index`: an empty cue offers the guessed next speaker.
pub fn suggest_at(script: &Script, index: usize, kind: ElementKind, text: &str) -> Vec<String> {
    if kind == ElementKind::Character && text.trim().is_empty() {
        return next_speaker(script, index).into_iter().collect();
    }
    suggest(script, kind, text)
}

/// Split a heading into (prefix, location, time of day).
pub fn split_heading(text: &str) -> (Option<&str>, &str, Option<&str>) {
    let upper_starts = |p: &str| text.get(..p.len()).is_some_and(|head| head.eq_ignore_ascii_case(p));
    // Longest prefix first so `INT./EXT.` wins over `INT.`.
    let mut prefixes = SCENE_PREFIXES;
    prefixes.sort_by_key(|p| std::cmp::Reverse(p.len()));
    let (prefix, rest) = match prefixes.iter().find(|p| upper_starts(p)) {
        Some(p) => (text.get(..p.len()), text.get(p.len()..).unwrap_or("")),
        None => (None, text),
    };
    let rest = rest.trim_start();
    match rest.rsplit_once(" - ") {
        Some((location, time)) => (prefix, location.trim(), Some(time.trim())),
        None => (prefix, rest.trim_end(), None),
    }
}

/// Suggestions for an element of `kind` whose current text is `text`.
pub fn suggest(script: &Script, kind: ElementKind, text: &str) -> Vec<String> {
    let typed = text.to_uppercase();
    if typed.trim().is_empty() {
        return Vec::new();
    }
    match kind {
        ElementKind::Character => {
            if let Some(open) = typed.rfind('(') {
                let base = typed.get(..open).unwrap_or("").trim_end();
                let partial = typed.get(open + 1..).unwrap_or("");
                return rank(partial, weighted(&script.lists.extensions).chain(EXTENSIONS.iter().map(|e| (e.to_string(), 1))))
                    .into_iter()
                    .map(|e| format!("{base} ({e})"))
                    .collect();
            }
            let names = script.elements.iter().filter(|e| e.kind == ElementKind::Character).map(|e| (character_name(&e.text).to_uppercase(), 1));
            rank(&typed, names.chain(weighted(&script.lists.characters)))
        }
        ElementKind::SceneHeading => {
            let (prefix, location, time) = split_heading(&typed);
            let headings: Vec<String> =
                script.elements.iter().filter(|e| e.kind == ElementKind::SceneHeading).map(|e| e.text.to_uppercase()).collect();
            match (prefix, time) {
                (None, _) => {
                    let stored = script.lists.intros.iter().map(|intro| (script.lists.intro(intro), 1));
                    rank(&typed, SCENE_PREFIXES.iter().map(|p| (p.to_string(), 1)).chain(stored))
                }
                (Some(prefix), None) => {
                    let locations = headings.iter().filter_map(|h| {
                        let (_, location, _) = split_heading(h);
                        (!location.is_empty()).then(|| (location.to_string(), 1))
                    });
                    rank(location, locations.chain(weighted(&script.lists.locations))).into_iter().map(|l| format!("{prefix} {l}")).collect()
                }
                (Some(prefix), Some(time)) => {
                    let used = headings.iter().filter_map(|h| split_heading(h).2.map(|t| (t.to_string(), 2)));
                    let builtin = TIMES_OF_DAY.iter().map(|t| (t.to_string(), 1));
                    rank(time, used.chain(builtin).chain(weighted(&script.lists.times)))
                        .into_iter()
                        .map(|t| format!("{prefix} {location} - {t}"))
                        .collect()
                }
            }
        }
        ElementKind::Transition => {
            let used = script.elements.iter().filter(|e| e.kind == ElementKind::Transition).map(|e| (e.text.to_uppercase(), 2));
            rank(typed.trim_start(), used.chain(TRANSITIONS.iter().map(|t| (t.to_string(), 1))).chain(weighted(&script.lists.transitions)))
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Element;

    fn script() -> Script {
        Script {
            elements: vec![
                Element::new(ElementKind::SceneHeading, "INT. KITCHEN - NIGHT"),
                Element::new(ElementKind::Character, "MARGARET"),
                Element::new(ElementKind::Dialogue, "Hi."),
                Element::new(ElementKind::Character, "MARGARET (V.O.)"),
                Element::new(ElementKind::Dialogue, "Again."),
                Element::new(ElementKind::Character, "MARCUS"),
                Element::new(ElementKind::Dialogue, "Yes."),
                Element::new(ElementKind::SceneHeading, "EXT. KITCHEN GARDEN - DAY"),
            ],
            ..Script::default()
        }
    }

    #[test]
    fn stored_lists_add_suggestions_and_learn_what_the_script_uses() {
        let mut lists = Lists { characters: vec!["MARTHA".into()], extensions: vec!["PRELAP".into()], ..Lists::default() };
        lists.intros.push("INT".into());
        lists.intro_separator = Some(". ".into());
        lists.locations.push("KIOSK".into());
        lists.times.push("GOLDEN HOUR".into());
        lists.transitions.push("SMASH TO:".into());
        let script = Script { lists, ..script() };
        assert_eq!(suggest(&script, ElementKind::Character, "mart"), ["MARTHA"]);
        assert_eq!(suggest(&script, ElementKind::Character, "NORA (pre"), ["NORA (PRELAP)"]);
        assert_eq!(suggest(&script, ElementKind::SceneHeading, "int. ki"), ["INT. KITCHEN", "INT. KITCHEN GARDEN", "INT. KIOSK"]);
        assert_eq!(suggest(&script, ElementKind::SceneHeading, "INT. HALL - go"), ["INT. HALL - GOLDEN HOUR"]);
        assert!(suggest(&script, ElementKind::Transition, "SM").iter().any(|t| t == "SMASH TO:"));
        // With INT. stored as `INT` and a separator, the intro is offered once.
        assert_eq!(suggest(&script, ElementKind::SceneHeading, "i"), ["INT.", "INT./EXT.", "I/E"]);

        // What the script uses is learned once, in capitals, after what was there.
        let mut lists = Lists { characters: vec!["MARCUS".into()], ..Lists::default() };
        assert!(lists.learn(&script));
        assert_eq!(lists.characters, ["MARCUS", "MARGARET"]);
        assert_eq!(lists.extensions, ["V.O."]);
        assert_eq!(lists.locations, ["KITCHEN", "KITCHEN GARDEN"]);
        assert_eq!(lists.intros, ["INT.", "EXT."]);
        assert_eq!(lists.times, ["NIGHT", "DAY"]);
        assert!(!lists.learn(&script), "nothing new the second time");
        let mut two = script.clone();
        two.elements.push(crate::model::Element::new(ElementKind::Character, "NORA (O.S.) (CONT'D)"));
        assert!(lists.learn(&two));
        assert_eq!(lists.extensions, ["V.O.", "O.S.", "CONT'D"], "each bracketed part on its own");
        lists.characters.push("GONE".into());
        lists.rebuild(&script);
        assert_eq!(lists.characters, ["MARGARET", "MARCUS"], "a rebuild keeps only what the script uses");
    }

    #[test]
    fn characters_by_frequency() {
        assert_eq!(suggest(&script(), ElementKind::Character, "mar"), ["MARGARET", "MARCUS"]);
        assert_eq!(suggest(&script(), ElementKind::Character, "MARCUS (v"), ["MARCUS (V.O.)"]);
        assert!(suggest(&script(), ElementKind::Character, "").is_empty());
    }

    #[test]
    fn scene_heading_parts() {
        assert_eq!(suggest(&script(), ElementKind::SceneHeading, "i"), ["INT.", "INT./EXT.", "I/E"]);
        let times = suggest(&Script::default(), ElementKind::SceneHeading, "INT. HALL - ");
        assert_eq!(times.get(..4).unwrap_or_default(), ["INT. HALL - DAY", "INT. HALL - NIGHT", "INT. HALL - AFTERNOON", "INT. HALL - MORNING"]);
        assert_eq!(suggest(&script(), ElementKind::SceneHeading, "int. kit"), ["INT. KITCHEN", "INT. KITCHEN GARDEN"]);
        assert_eq!(suggest(&script(), ElementKind::SceneHeading, "INT. HALL - N")[0], "INT. HALL - NIGHT");
    }

    #[test]
    fn guesses_the_next_speaker_in_a_two_hander() {
        let mut script = script();
        script.elements.push(Element::new(ElementKind::Character, ""));
        let last = script.elements.len() - 1;
        // MARGARET, MARGARET (V.O.)... the fixture's last two speakers are the same person.
        assert!(suggest_at(&script, last, ElementKind::Character, "").is_empty());
        let mut exchange = Script::default();
        for name in ["ANNA", "BEN", "ANNA"] {
            exchange.elements.push(Element::new(ElementKind::Character, name));
            exchange.elements.push(Element::new(ElementKind::Dialogue, "Line."));
        }
        exchange.elements.push(Element::new(ElementKind::Character, ""));
        assert_eq!(suggest_at(&exchange, 6, ElementKind::Character, ""), ["BEN"]);
    }

    #[test]
    fn transitions() {
        assert_eq!(suggest(&script(), ElementKind::Transition, "sm"), ["SMASH CUT TO:"]);
    }
}
