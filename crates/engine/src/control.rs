//! The JSON-lines control protocol (`docs/control-protocol.md`).
//!
//! Each request is one line: `{"id": 1, "method": "command.run", "params": {...}}`; each reply
//! is one line: `{"id": 1, "result": ...}` or `{"id": 1, "error": "..."}`. The server listens on
//! 127.0.0.1 only, and a connection ends at its first line that is not a JSON request.
//! Connection threads forward requests to the owner of the session (the UI thread or the
//! headless loop) over a channel, so all document access stays on one thread.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use serde_json::{Map, Value, json};
use storyteller_core::paginate::render_text;

use crate::commands::Registry;
use crate::session::Session;

/// Longest accepted request line (1 MiB).
pub const MAX_LINE_BYTES: u64 = 1024 * 1024;
/// Most connections served at once; more are closed as they arrive.
pub const MAX_CONNECTIONS: usize = 8;
/// How long a connection waits for the session owner to answer.
pub const REPLY_TIMEOUT: Duration = Duration::from_secs(30);

/// The part of the protocol that needs a user interface (`ui.*`).
pub trait UiHost {
    fn ui(&mut self, method: &str, params: &Value) -> Result<Value, String>;
}

/// A host with no user interface: every `ui.*` request fails politely.
pub struct Headless;

impl UiHost for Headless {
    fn ui(&mut self, method: &str, _params: &Value) -> Result<Value, String> {
        Err(format!("`{method}` needs the desktop app; this is a headless session"))
    }
}

fn params_object(params: &Value) -> Map<String, Value> {
    params.as_object().cloned().unwrap_or_default()
}

/// Describe the document for `doc.inspect`.
pub fn inspect(session: &mut Session) -> Value {
    let pages = session.paginated().page_count();
    let element_pages = session.paginated().element_pages.clone();
    let elements: Vec<Value> = session
        .script
        .elements
        .iter()
        .enumerate()
        .map(|(index, e)| {
            json!({
                "index": index,
                "kind": e.kind.id(),
                "text": e.text,
                "scene_number": e.scene_number,
                "dual": e.dual,
                "page": element_pages.get(index).copied().unwrap_or(0),
            })
        })
        .collect();
    json!({
        "path": session.path.as_ref().map(|p| p.display().to_string()),
        "format": session.format,
        "dirty": session.dirty,
        "caret": session.caret,
        "title_page": session.script.title_page,
        "pages": pages,
        "elements": elements,
    })
}

/// Handle one request against a session. Returns the result or an error message.
pub fn dispatch(method: &str, params: &Value, session: &mut Session, registry: &Registry, host: &mut dyn UiHost) -> Result<Value, String> {
    match method {
        "app.ping" => Ok(json!("pong")),
        "app.version" => Ok(json!(crate::app_version())),
        "command.list" => serde_json::to_value(registry.list(session)).map_err(|e| e.to_string()),
        "command.run" => {
            let id = params.get("command").and_then(Value::as_str).ok_or("missing `command`")?;
            let command_params = params.get("params").map(params_object).unwrap_or_default();
            let outcome = registry.run(id, session, &command_params).map_err(|e| e.to_string())?;
            serde_json::to_value(outcome).map_err(|e| e.to_string())
        }
        "doc.inspect" => Ok(inspect(session)),
        "doc.paginate" => {
            let with_text = params.get("text").and_then(Value::as_bool).unwrap_or(false);
            let paginated = session.paginated().clone();
            let mut value = json!({ "pages": paginated.page_count(), "scenes": paginated.scenes });
            if with_text && let Some(object) = value.as_object_mut() {
                object.insert("text".into(), json!(render_text(&paginated)));
            }
            Ok(value)
        }
        m if m.starts_with("ui.") => host.ui(m, params),
        other => Err(format!("unknown method `{other}`")),
    }
}

/// Parse a request line, dispatch it and format the reply line (without a newline).
pub fn handle_line(line: &str, session: &mut Session, registry: &Registry, host: &mut dyn UiHost) -> String {
    let request: Value = match serde_json::from_str(line) {
        Ok(value) => value,
        Err(error) => return json!({ "id": Value::Null, "error": format!("invalid JSON: {error}") }).to_string(),
    };
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let Some(method) = request.get("method").and_then(Value::as_str) else {
        return json!({ "id": id, "error": "missing `method`" }).to_string();
    };
    let params = request.get("params").cloned().unwrap_or(Value::Null);
    match catch_unwind(AssertUnwindSafe(|| dispatch(method, &params, session, registry, host))) {
        Ok(Ok(result)) => json!({ "id": id, "result": result }).to_string(),
        Ok(Err(error)) => json!({ "id": id, "error": error }).to_string(),
        Err(_) => json!({ "id": id, "error": "internal error while handling the request" }).to_string(),
    }
}

/// A request waiting for the session owner.
pub struct ControlRequest {
    pub line: String,
    pub reply: Sender<String>,
}

/// Start the server on `127.0.0.1:port`. Requests arrive on the returned channel; a `wake`
/// callback (for example, a repaint request) runs after each one is queued.
pub fn serve(port: u16, wake: impl Fn() + Send + Sync + Clone + 'static) -> std::io::Result<Receiver<ControlRequest>> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    let (sender, receiver) = mpsc::channel();
    let open = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    thread::Builder::new().name("control-accept".into()).spawn(move || {
        for mut stream in listener.incoming().flatten() {
            if open.fetch_add(1, std::sync::atomic::Ordering::SeqCst) >= MAX_CONNECTIONS {
                open.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                let _ = writeln!(stream, "{}", json!({ "id": Value::Null, "error": "too many connections" }));
                continue;
            }
            let sender = sender.clone();
            let wake = wake.clone();
            let slot = open.clone();
            let spawned = thread::Builder::new().name("control-conn".into()).spawn(move || {
                let _ = catch_unwind(AssertUnwindSafe(|| connection(stream, &sender, &wake)));
                slot.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            });
            if spawned.is_err() {
                open.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            }
        }
    })?;
    Ok(receiver)
}

fn connection(stream: TcpStream, sender: &Sender<ControlRequest>, wake: &dyn Fn()) {
    let Ok(mut writer) = stream.try_clone() else { return };
    let mut reader = BufReader::new(stream);
    loop {
        let mut line = String::new();
        match (&mut reader).take(MAX_LINE_BYTES).read_line(&mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        if !line.ends_with('\n') && line.len() as u64 >= MAX_LINE_BYTES {
            let _ = writeln!(writer, "{}", json!({ "id": Value::Null, "error": "request line too long" }));
            return;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Only requests of this protocol, one JSON object a line: anything else (a web page's
        // HTTP request, say, whose body could otherwise carry a request) ends the connection
        // before a later line is read.
        if !serde_json::from_str::<Value>(line).is_ok_and(|v| v.is_object()) {
            let _ = writeln!(writer, "{}", json!({ "id": Value::Null, "error": "not a request of this protocol; closing" }));
            return;
        }
        let (reply_sender, reply_receiver) = mpsc::channel();
        if sender.send(ControlRequest { line: line.to_string(), reply: reply_sender }).is_err() {
            return;
        }
        wake();
        let reply = reply_receiver
            .recv_timeout(REPLY_TIMEOUT)
            .unwrap_or_else(|_| json!({ "id": Value::Null, "error": "the app did not answer in time" }).to_string());
        if writeln!(writer, "{reply}").is_err() {
            return;
        }
    }
}
