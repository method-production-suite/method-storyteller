# Control protocol

The desktop app (`method-storyteller-app --control <port>`) and the headless server
(`method-storyteller serve --control <port> [file]`) speak JSON lines on `127.0.0.1:<port>`. Only loopback is
bound, and a connection is closed at its first line that is not a JSON request (so an HTTP request sent
to the port, with whatever its body holds, goes no further).

## The token

Each launch has a token, and a connection's first request must be `auth` with it; anything else,
or the wrong token, closes the connection. The launch prints its token on standard error:

```
method-storyteller-app: control channel on 127.0.0.1:9876, token 3f9c…
```

A program that starts the app can choose the token instead, by setting `STORYTELLER_CONTROL_TOKEN` in
the app's environment (any non-empty text; make it long and random):

```json
{"id": 0, "method": "auth", "params": {"token": "3f9c…"}}
{"id": 0, "result": "ok"}
```

After that, each request is one line, each reply one line:

```json
{"id": 1, "method": "command.run", "params": {"command": "flow.enter", "params": {}}}
{"id": 1, "result": {}}
{"id": 2, "error": "unknown command `nope`"}
```

Requests on one connection are answered in order. Lines over 1 MiB are refused.

## Document methods (app and headless)

| Method | Params | Result |
| --- | --- | --- |
| `app.ping` | | `"pong"` |
| `app.version` | | `"Method Storyteller 0.1.0"` |
| `command.list` | | Every command: `id`, `label`, `menu`, `shortcut`, `params`, `enabled` |
| `command.run` | `command`, `params` | The command's `message` and `value` |
| `doc.inspect` | | Path, format, dirty flag, caret, title page, page count, every element with its page |
| `doc.paginate` | `text` (bool) | Page count, scenes with pages, optionally the rendered pages |

Commands are listed by `command.list` (or `method-storyteller commands`). Element indices are 0-based; text
offsets are UTF-8 byte offsets.

## UI methods (desktop app only)

| Method | Params | Does |
| --- | --- | --- |
| `ui.elements` | | Widgets from the last frame: `id`, `role`, `label`, `rect` `[x, y, w, h]` in points, `enabled`, `focused` |
| `ui.click` | `id` | Clicks the middle of a widget |
| `ui.type` | `text`, optional `id` | Types text (into `id` after clicking it) |
| `ui.key` | `key` (egui key name such as `Enter`, `Tab`, `S`), `modifiers` (`cmd`, `shift`, `alt`, `ctrl`) | Presses and releases a key |
| `ui.focus` | `element`, optional `offset` | Puts the caret in a script element |
| `ui.command` | `id` | Runs a command the way its menu item does (including file dialogs) |
| `ui.state` | | Focused element, autocomplete suggestions, dialogs, open Help windows (`help_windows`), zoom, status and error |
| `ui.screenshot` | `path` | Saves a PNG of the window; replies with its path and size |

Synthetic input is delivered over the next frames; the app holds later requests until it has
been delivered, so a `ui.state` after `ui.type` sees the result.

Widget ids: `menu.file`, `menu.item.<command id>`, `editor.element.<n>`, `navigator.scene.<n>`,
`autocomplete.item.<n>`, `status.element_kind`, `status.page`, `dialog.title_page.<field>`,
`dialog.title_page.ok`, `prompt.save`, `prompt.discard`, `prompt.cancel`, and for the Help windows
`dialog.about.website`, `dialog.about.suite`, `dialog.about.licenses`, `dialog.licenses.tab.<n>`
and `dialog.<window>.close` (`about`, `shortcuts`, `whats_new`, `licenses`).

The Help menu's commands (`help.about`, `help.user_guide`, `help.shortcuts`, `help.whats_new`,
`help.check_updates`, `prefs.check_updates_automatically`, `help.report_bug`, `help.licenses`,
`help.website`) belong to the app, so they run through `ui.command`, not `command.run`.
`help.user_guide`, `help.report_bug` and `help.website` open the browser.

## Example

```python
import json, os, socket
s = socket.create_connection(("127.0.0.1", 9876)); f = s.makefile("rw")
def call(method, **params):
    f.write(json.dumps({"id": 1, "method": method, "params": params}) + "\n"); f.flush()
    return json.loads(f.readline())
call("auth", token=os.environ["STORYTELLER_CONTROL_TOKEN"])  # or the token the app printed
call("ui.focus", element=0)
call("ui.type", text="int. kitchen - night")
call("ui.key", key="Enter")
call("ui.screenshot", path="/tmp/kitchen.png")
```
