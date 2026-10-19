//! The find bar: Edit > Find… opens it above the script. The current match is highlighted in
//! the script while the typing stays in the bar. Every search and replacement runs through the
//! engine's `find.*` commands, so Replace All is one Undo.

use serde_json::{Map, Value, json};

use super::{StorytellerApp, params};

#[derive(Default)]
pub(super) struct FindBar {
    pub(super) text: String,
    pub(super) replacement: String,
    pub(super) case: bool,
    pub(super) whole_word: bool,
    /// The current match: element, start, end.
    pub(super) current: Option<(usize, usize, usize)>,
    /// "2 of 5", "No matches for “x”", "Replaced 3 matches".
    pub(super) status: String,
    pub(super) focus: bool,
    /// The query the current match was found for, to search again when it changes.
    pub(super) searched: Option<(String, bool, bool)>,
}

impl FindBar {
    pub(super) fn open() -> Self {
        FindBar { focus: true, ..FindBar::default() }
    }

    pub(super) fn focus(&mut self) {
        self.focus = true;
    }

    fn query(&self) -> Map<String, Value> {
        params(&[("text", json!(self.text)), ("case", json!(self.case)), ("whole_word", json!(self.whole_word))])
    }
}

/// Which way to step.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Step {
    /// From the caret, keeping a match that starts there (a new or changed query).
    Here,
    Next,
    Previous,
}

impl StorytellerApp {
    pub(super) fn find_bar(&mut self, ui: &mut egui::Ui) {
        self.drop_stale_match();
        let Some(bar) = &mut self.find else { return };
        let mut step = None;
        let mut replace = false;
        let mut replace_all = false;
        let mut close = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let find = ui.add(egui::TextEdit::singleline(&mut bar.text).hint_text("Find").desired_width(220.0));
            self.automation.add("find.text", "textbox", "Find", &find);
            if bar.focus {
                find.request_focus();
                bar.focus = false;
            }
            let enter = find.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if enter {
                step = Some(if ui.input(|i| i.modifiers.shift) { Step::Previous } else { Step::Next });
                find.request_focus();
            }
            // egui drops focus on Escape before the field sees the key.
            if find.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                close = true;
            }
            let with = ui.add(egui::TextEdit::singleline(&mut bar.replacement).hint_text("Replace with").desired_width(180.0));
            self.automation.add("find.replacement", "textbox", "Replace with", &with);
            let case = ui.toggle_value(&mut bar.case, "Aa").on_hover_text("Match case");
            self.automation.add("find.case", "checkbox", "Match case", &case);
            let word = ui.toggle_value(&mut bar.whole_word, "Word").on_hover_text("Whole words only");
            self.automation.add("find.whole_word", "checkbox", "Whole words", &word);
            let previous = ui.button("‹").on_hover_text("Previous match (Shift+Enter)");
            self.automation.add("find.previous", "button", "Previous", &previous);
            let next = ui.button("›").on_hover_text("Next match (Enter)");
            self.automation.add("find.next", "button", "Next", &next);
            if previous.clicked() {
                step = Some(Step::Previous);
            }
            if next.clicked() {
                step = Some(Step::Next);
            }
            let one = ui.button("Replace");
            self.automation.add("find.replace", "button", "Replace", &one);
            let all = ui.button("Replace All");
            self.automation.add("find.replace_all", "button", "Replace All", &all);
            replace = one.clicked();
            replace_all = all.clicked();
            ui.weak(&bar.status);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let done = ui.button("Done");
                self.automation.add("find.close", "button", "Done", &done);
                close |= done.clicked();
            });
        });
        // A changed query searches again from where the caret is.
        let query = (bar.text.clone(), bar.case, bar.whole_word);
        if step.is_none() && bar.searched.as_ref() != Some(&query) {
            step = Some(Step::Here);
        }
        if close {
            self.close_find();
        } else if replace_all {
            self.replace_all();
        } else if replace {
            self.replace_current();
        } else if let Some(step) = step {
            self.find_step(step);
        }
    }

    /// An edit, Undo or Redo can move the text under the highlighted match: forget it once the
    /// text there no longer matches.
    fn drop_stale_match(&mut self) {
        let Some(bar) = &mut self.find else { return };
        let Some((element, start, end)) = bar.current else { return };
        let query = storyteller_engine::find::Query { text: bar.text.clone(), case: bar.case, whole_word: bar.whole_word, kind: None };
        let still = self.session.element(element).is_some_and(|e| storyteller_engine::find::find_in(&e.text, &query).contains(&(start, end)));
        if !still {
            bar.current = None;
            bar.status.clear();
            self.editor.highlight = None;
        }
    }

    pub(super) fn close_find(&mut self) {
        self.find = None;
        self.editor.highlight = None;
        self.editor.pending_focus = Some((self.session.caret.element, self.session.caret.offset));
    }

    /// Finds the next or previous match and highlights it.
    pub(super) fn find_step(&mut self, step: Step) {
        let Some(bar) = &mut self.find else { return };
        bar.searched = Some((bar.text.clone(), bar.case, bar.whole_word));
        if bar.text.is_empty() {
            bar.current = None;
            bar.status.clear();
            self.editor.highlight = None;
            return;
        }
        let mut query = bar.query();
        let (element, offset) = match (step, bar.current) {
            (Step::Next, Some((e, _, end))) => (e, end),
            (Step::Previous, Some((e, start, _))) => (e, start),
            _ => (self.session.caret.element, self.session.caret.offset),
        };
        let from = match step {
            // Back up to the start of a match the caret sits just after, so it is found again.
            Step::Here => bar.current.filter(|c| (c.0, c.2) == (element, offset)).map_or(offset, |c| c.1),
            _ => offset,
        };
        query.insert("index".into(), json!(element));
        query.insert("offset".into(), json!(from));
        query.insert("backwards".into(), json!(step == Step::Previous));
        let found = self.registry.run("find.next", &mut self.session, &query);
        let Some(bar) = &mut self.find else { return };
        match found.map(|o| o.value) {
            Ok(value) if value.get("element").is_some() => {
                let number = |key: &str| value.get(key).and_then(Value::as_u64).and_then(|v| usize::try_from(v).ok()).unwrap_or(0);
                let hit = (number("element"), number("start"), number("end"));
                bar.current = Some(hit);
                bar.status = format!("{} of {}", number("number"), number("total"));
                self.editor.highlight = Some(hit);
                self.editor.scroll_to = Some(hit.0);
            }
            Ok(_) => {
                bar.current = None;
                bar.status = crate::messages::nothing_found(&bar.text);
                self.editor.highlight = None;
            }
            Err(error) => bar.status = error.to_string(),
        }
    }

    fn replace_current(&mut self) {
        let Some(bar) = &self.find else { return };
        let Some((element, start, _)) = bar.current else {
            self.find_step(Step::Next);
            return;
        };
        let mut query = bar.query();
        query.insert("replacement".into(), json!(bar.replacement));
        query.insert("index".into(), json!(element));
        query.insert("start".into(), json!(start));
        if let Err(error) = self.registry.run("find.replace", &mut self.session, &query) {
            if let Some(bar) = &mut self.find {
                bar.status = error.to_string();
            }
            return;
        }
        // The caret now sits after the replacement; find the next match from there.
        if let Some(bar) = &mut self.find {
            bar.current = None;
        }
        self.find_step(Step::Here);
    }

    fn replace_all(&mut self) {
        let Some(bar) = &self.find else { return };
        let mut query = bar.query();
        query.insert("replacement".into(), json!(bar.replacement));
        let outcome = self.registry.run("find.replace_all", &mut self.session, &query);
        let Some(bar) = &mut self.find else { return };
        bar.current = None;
        self.editor.highlight = None;
        bar.status = match outcome {
            Ok(outcome) => outcome.message.unwrap_or_default(),
            Err(error) => error.to_string(),
        };
        // Leave the status showing until the query changes.
        bar.searched = Some((bar.text.clone(), bar.case, bar.whole_word));
    }
}
