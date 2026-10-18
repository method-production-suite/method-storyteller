# Control protocol

The desktop app (`method-storyteller-app --control <port>`) and the headless server
(`method-storyteller serve --control <port> [file]`) speak JSON lines on `127.0.0.1:<port>`. Only loopback is
bound. Each request is one line, each reply one line:

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
| `ui.state` | | Focused element, autocomplete suggestions, dialogs, zoom, status and error |
| `ui.screenshot` | `path` | Saves a PNG of the window; replies with its path and size |
| `ui.window` | optional `width`, `height` (points), `theme` (`light` or `dark`), `page_view` (bool) | Sizes the window, sets the theme and the view, and clears the status line, for repeatable pictures |

Synthetic input is delivered over the next frames; the app holds later requests until it has
been delivered, so a `ui.state` after `ui.type` sees the result.

Widget ids: `menu.file`, `menu.item.<command id>`, `editor.element.<n>`, `navigator.scene.<n>`,
`autocomplete.item.<n>`, `status.element_kind`, `status.page`, `dialog.title_page.<field>`,
`dialog.title_page.ok`, `prompt.save`, `prompt.discard`, `prompt.cancel`.

## Example

```python
import json, socket
s = socket.create_connection(("127.0.0.1", 9876)); f = s.makefile("rw")
def call(method, **params):
    f.write(json.dumps({"id": 1, "method": method, "params": params}) + "\n"); f.flush()
    return json.loads(f.readline())
call("ui.focus", element=0)
call("ui.type", text="int. kitchen - night")
call("ui.key", key="Enter")
call("ui.screenshot", path="/tmp/kitchen.png")
```
