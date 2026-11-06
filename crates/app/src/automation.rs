//! Automation ids for every interactive widget, plus synthetic input for the control channel.

use std::collections::VecDeque;

use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, Response};
use serde_json::{Value, json};

#[derive(Clone, Debug)]
pub struct Widget {
    pub id: String,
    pub role: &'static str,
    pub label: String,
    pub rect: Rect,
    pub enabled: bool,
    pub focused: bool,
}

/// Widgets seen in the last complete frame and input waiting to be injected.
#[derive(Default)]
pub struct Automation {
    widgets: Vec<Widget>,
    building: Vec<Widget>,
    /// Batches of events, one batch per frame.
    pub injected: VecDeque<Vec<Event>>,
}

impl Automation {
    pub fn begin_frame(&mut self) {
        self.building.clear();
    }

    pub fn end_frame(&mut self) {
        self.widgets = std::mem::take(&mut self.building);
    }

    /// Record a widget under a stable automation id.
    pub fn add(&mut self, id: impl Into<String>, role: &'static str, label: impl Into<String>, response: &Response) {
        self.building.push(Widget {
            id: id.into(),
            role,
            label: label.into(),
            rect: response.rect,
            enabled: response.enabled(),
            focused: response.has_focus(),
        });
    }

    /// Record a widget not drawn this frame (a paragraph off screen), where it would be.
    pub fn add_undrawn(&mut self, id: impl Into<String>, role: &'static str, label: impl Into<String>, rect: Rect) {
        self.building.push(Widget { id: id.into(), role, label: label.into(), rect, enabled: true, focused: false });
    }

    pub fn find(&self, id: &str) -> Option<&Widget> {
        self.widgets.iter().find(|w| w.id == id)
    }

    pub fn list(&self) -> Value {
        Value::Array(
            self.widgets
                .iter()
                .map(|w| {
                    json!({
                        "id": w.id,
                        "role": w.role,
                        "label": w.label,
                        "rect": [w.rect.min.x, w.rect.min.y, w.rect.width(), w.rect.height()],
                        "enabled": w.enabled,
                        "focused": w.focused,
                    })
                })
                .collect(),
        )
    }

    /// Queue a click in the middle of a widget.
    pub fn click(&mut self, id: &str) -> Result<Value, String> {
        let widget = self.find(id).ok_or_else(|| format!("no widget `{id}` on screen"))?;
        if !widget.enabled {
            return Err(format!("`{id}` is disabled"));
        }
        let pos = widget.rect.center();
        self.click_at(pos);
        Ok(json!({ "queued": "click", "at": [pos.x, pos.y] }))
    }

    pub fn click_at(&mut self, pos: Pos2) {
        let press = Event::PointerButton { pos, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE };
        let release = Event::PointerButton { pos, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE };
        self.injected.push_back(vec![Event::PointerMoved(pos), press]);
        self.injected.push_back(vec![release]);
    }

    pub fn type_text(&mut self, text: &str) {
        self.injected.push_back(vec![Event::Text(text.to_string())]);
    }

    /// Queue a key press, e.g. `Enter` with `["shift"]`.
    pub fn key(&mut self, name: &str, modifiers: &[String]) -> Result<Value, String> {
        let key = Key::from_name(name).ok_or_else(|| format!("unknown key `{name}`"))?;
        let mut mods = Modifiers::NONE;
        for m in modifiers {
            match m.to_ascii_lowercase().as_str() {
                "shift" => mods.shift = true,
                "alt" | "option" => mods.alt = true,
                "ctrl" | "control" => mods.ctrl = true,
                "cmd" | "command" => mods.command = true,
                "mac_cmd" => mods.mac_cmd = true,
                other => return Err(format!("unknown modifier `{other}`")),
            }
        }
        if mods.command && cfg!(target_os = "macos") {
            mods.mac_cmd = true;
        } else if mods.command {
            mods.ctrl = true;
        }
        let event = |pressed| Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: mods };
        self.injected.push_back(vec![event(true)]);
        self.injected.push_back(vec![event(false)]);
        Ok(json!({ "queued": "key", "key": name }))
    }
}
