//! The control interface: requests from a test or script, and screenshots.

use super::*;

impl StorytellerApp {
    pub(super) fn handle_control(&mut self, ctx: &egui::Context) {
        let Some(receiver) = self.control.take() else { return };
        // One request at a time while synthetic input is still being delivered.
        while self.automation.injected.is_empty() && self.queued.is_empty() {
            let Ok(request) = receiver.try_recv() else { break };
            let parsed: Value = serde_json::from_str(&request.line).unwrap_or(Value::Null);
            if parsed.get("method").and_then(Value::as_str) == Some("ui.screenshot") {
                let id = parsed.get("id").cloned().unwrap_or(Value::Null);
                let path = parsed
                    .pointer("/params/path")
                    .and_then(Value::as_str)
                    .map(PathBuf::from)
                    .unwrap_or_else(|| std::env::temp_dir().join("method-storyteller-screenshot.png"));
                let slot = std::sync::Arc::clone(&self.screenshot);
                let repaint = ctx.clone();
                ctx.request_screenshot(move |image| {
                    if let Ok(mut slot) = slot.lock() {
                        *slot = Some(image);
                    }
                    repaint.request_repaint();
                });
                self.pending_screenshot = Some((id, request.reply, path));
                break;
            }
            let reply = {
                let mut host = AppHost {
                    automation: &mut self.automation,
                    editor: &mut self.editor,
                    queued: &mut self.queued,
                    status: &self.status,
                    error: self.error.as_deref(),
                    navigator: self.prefs.get().view.navigator,
                    zoom: self.prefs.get().zoom(),
                    dialogs: (self.title_dialog.is_some(), self.prompt.is_some()),
                    help_windows: self.help.open(),
                };
                handle_line(&request.line, &mut self.session, &self.registry, &mut host)
            };
            let _ = request.reply.send(reply);
            ctx.request_repaint();
        }
        self.control = Some(receiver);
        ctx.request_repaint_after(Duration::from_millis(50));
    }

    pub(super) fn handle_screenshot(&mut self, ctx: &egui::Context) {
        if self.pending_screenshot.is_none() {
            return;
        }
        let image = self.screenshot.lock().ok().and_then(|mut slot| slot.take());
        let Some(image) = image else {
            ctx.request_repaint_after(Duration::from_millis(50));
            return;
        };
        let Some((id, reply, path)) = self.pending_screenshot.take() else { return };
        let message = match encode_png(&image) {
            Ok(bytes) => match craft_atomic::atomic_write(&path, &bytes) {
                Ok(()) => json!({ "id": id, "result": { "path": path.display().to_string(), "width": image.size[0], "height": image.size[1] } }),
                Err(error) => json!({ "id": id, "error": format!("could not write {}: {error}", path.display()) }),
            },
            Err(error) => json!({ "id": id, "error": error }),
        };
        let _ = reply.send(message.to_string());
    }
}

fn encode_png(image: &egui::ColorImage) -> Result<Vec<u8>, String> {
    let [width, height] = image.size;
    let mut rgba = Vec::new();
    rgba.try_reserve(width.saturating_mul(height).saturating_mul(4)).map_err(|_| "screenshot too large".to_string())?;
    for pixel in &image.pixels {
        rgba.extend_from_slice(&pixel.to_srgba_unmultiplied());
    }
    let mut out = Vec::new();
    {
        let width = u32::try_from(width).map_err(|_| "screenshot too wide")?;
        let height = u32::try_from(height).map_err(|_| "screenshot too tall")?;
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(&rgba).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

/// `ui.*` methods of the control channel.
struct AppHost<'a> {
    automation: &'a mut Automation,
    editor: &'a mut EditorState,
    queued: &'a mut Vec<String>,
    status: &'a str,
    error: Option<&'a str>,
    navigator: bool,
    zoom: f32,
    dialogs: (bool, bool),
    help_windows: Vec<&'static str>,
}

impl UiHost for AppHost<'_> {
    fn ui(&mut self, method: &str, params: &Value) -> Result<Value, String> {
        let text = |name: &str| params.get(name).and_then(Value::as_str).map(str::to_string);
        match method {
            "ui.elements" => Ok(self.automation.list()),
            "ui.click" => self.automation.click(&text("id").ok_or("missing `id`")?),
            "ui.type" => {
                if let Some(id) = text("id") {
                    self.automation.click(&id)?;
                }
                self.automation.type_text(&text("text").ok_or("missing `text`")?);
                Ok(json!({ "queued": "type" }))
            }
            "ui.key" => {
                let modifiers: Vec<String> = params
                    .get("modifiers")
                    .and_then(Value::as_array)
                    .map(|m| m.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
                    .unwrap_or_default();
                self.automation.key(&text("key").ok_or("missing `key`")?, &modifiers)
            }
            "ui.focus" => {
                let index = params.get("element").and_then(Value::as_u64).and_then(|v| usize::try_from(v).ok()).ok_or("missing `element`")?;
                let offset = params.get("offset").and_then(Value::as_u64).and_then(|v| usize::try_from(v).ok()).unwrap_or(usize::MAX);
                self.editor.pending_focus = Some((index, offset));
                Ok(json!({ "queued": "focus" }))
            }
            "ui.command" => {
                let id = text("id").ok_or("missing `id`")?;
                self.queued.push(id.clone());
                Ok(json!({ "queued": id }))
            }
            "ui.state" => Ok(json!({
                "focused_element": self.editor.focused,
                "completion": if self.editor.completion.dismissed { Vec::new() } else { self.editor.completion.items.clone() },
                "navigator": self.navigator,
                "zoom": self.zoom,
                "title_page_dialog": self.dialogs.0,
                "unsaved_prompt": self.dialogs.1,
                "help_windows": self.help_windows,
                "status": self.status,
                "error": self.error,
            })),
            other => Err(format!("unknown method `{other}`")),
        }
    }
}
