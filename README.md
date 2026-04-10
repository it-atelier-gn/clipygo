# clipygo

[![Build](https://github.com/it-atelier-gn/clipygo/actions/workflows/ci.yml/badge.svg)](https://github.com/it-atelier-gn/clipygo/actions)
[![Rust](https://img.shields.io/badge/rust-1.80%2B-orange?logo=rust)](https://www.rust-lang.org/)
[![Tauri](https://img.shields.io/badge/tauri-2.x-blue?logo=tauri)](https://tauri.app/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/it-atelier-gn/clipygo)

Clipboard monitor that watches for specific content patterns and lets you route them to configured targets — instantly, with a single keypress.

It sits in your system tray, monitors the clipboard for regex matches (meeting links, Code With Me sessions, etc.), and pops up a compact window where you pick a target and hit Enter. Plugins handle the actual delivery — they're just executables that speak JSON over stdin/stdout.

---

## Quick Start

### Prerequisites

- [Rust](https://rustup.rs/) 1.80+
- [Node.js](https://nodejs.org/) 18+ with npm
- [Tauri CLI](https://tauri.app/start/): `cargo install tauri-cli`
- Windows 10/11 (primary target; macOS and Linux experimental)
- [WebView2 runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) — pre-installed on Windows 11 and most Windows 10 systems; required for the portable build

### Build & Run

```sh
git clone https://github.com/it-atelier-gn/clipygo.git
cd clipygo
npm install
cargo tauri dev

# or build a release binary
cargo tauri build
```

---

## Configuration

Settings are persisted to `%APPDATA%\clipygo\config.json` and managed through the in-app settings window (tray icon → Settings).

| Setting | Default | Description |
|---|---|---|
| `autostart` | `true` | Launch clipygo on system boot |
| `global_shortcut` | `Ctrl+F10` | Hotkey to show the popup |
| `regex_list` | see below | Patterns that trigger the popup automatically |
| `registry_url` | registry.json | URL of the plugin registry |

### Default regex patterns

```text
https://code-with-me\.jetbrains\.com/[a-zA-Z0-9\-_]+   # JetBrains Code With Me
https://[a-z0-9\-]+\.zoom\.us/j/[0-9]+                  # Zoom meeting links
https://meet\.google\.com/[a-z]{3}-[a-z]{4}-[a-z]{3}    # Google Meet
https://teams\.microsoft\.com/l/meetup-join/[^\s]+       # Microsoft Teams meetings
```

Add your own patterns in the settings window under **Pattern Recognition**.

---

## Plugin System

clipygo uses a persistent subprocess model for target providers. A plugin is any executable that reads JSON requests from stdin and writes JSON responses to stdout — one JSON object per line. The process stays alive for the lifetime of the session.

### Adding a plugin

Open Settings → **Plugins** → enter a name and the command to run:

```
Name:    My Plugin
Command: node C:\plugins\my-plugin\index.js
```

The command can be any executable or interpreter — compiled binaries, Node.js scripts, Python scripts, etc.

### Protocol

Every request is a single line of JSON. Every response is a single line of JSON.

| Command | Required | Description |
|---|---|---|
| `get_info` | Yes | Plugin name, version, description, author, link |
| `get_targets` | Yes | List of targets the plugin provides |
| `send` | Yes | Deliver clipboard content to a target |
| `get_config_schema` | No | JSON Schema + current values + instructions for the settings UI |
| `set_config` | No | Apply config values saved by the user |

#### `get_info` — called on startup to verify the plugin

```json
{"command":"get_info"}
```
```json
{"name":"My Plugin","version":"1.0.0","description":"...","author":"...","link":"https://github.com/..."}
```

The optional `link` field provides a URL shown next to the plugin name in settings (e.g. repo page, docs).

#### `get_targets` — returns all available targets for this plugin

```json
{"command":"get_targets"}
```
```json
{
  "targets": [
    {
      "id": "unique-target-id",
      "provider": "My Plugin",
      "formats": ["text"],
      "title": "Target Display Name",
      "description": "Short description",
      "image": "<base64 PNG>"
    }
  ]
}
```

#### `send` — send clipboard content to a target

```json
{"command":"send","target_id":"unique-target-id","content":"clipboard text here","format":"text"}
```
```json
{"success":true}
```
```json
{"success":false,"error":"Something went wrong"}
```

#### `get_config_schema` *(optional)* — return a JSON Schema and current values for the settings UI

```json
{"command":"get_config_schema"}
```
```json
{
  "instructions": "Setup instructions shown above the config fields (plain text with newlines).",
  "schema": {
    "type": "object",
    "title": "My Plugin",
    "properties": {
      "api_key": { "type": "string", "title": "API Key", "description": "Your secret API key", "format": "password" },
      "verbose": { "type": "boolean", "title": "Verbose Logging" },
      "mode": { "type": "string", "title": "Mode", "enum": ["fast","slow"], "enumTitles": ["Fast","Slow"], "default": "fast" },
      "fast_option": { "type": "string", "title": "Fast Option", "visibleIf": { "mode": "fast" } }
    },
    "required": ["api_key"]
  },
  "values": { "api_key": "", "verbose": false, "mode": "fast", "fast_option": "" }
}
```

If this command is implemented, clipygo shows a **Configure** button next to the plugin in Settings.

Supported field types: `string` (text input), `string` with `format: "password"` (password input with visibility toggle), `string` with `enum` (select), `boolean` (toggle).

Optional field features:
- `instructions` — plain text shown above the config fields (supports newlines)
- `visibleIf` — conditionally show a field based on another field's value: `"visibleIf": {"field": "value"}` or `"visibleIf": {"field": ["a","b"]}` for multiple values
- `readOnly` — display-only field that cannot be edited by the user (excluded from `set_config` values)

#### `set_config` *(optional)* — apply configuration values saved by the user

```json
{"command":"set_config","values":{"api_key":"secret","verbose":true,"mode":"fast"}}
```
```json
{"success":true}
```

The plugin is responsible for persisting the values (e.g. to its own config file).

### Plugin-initiated events

Plugins can push unsolicited events to clipygo by writing JSON lines to stdout at any time. Lines with a top-level `"event"` field are treated as events and forwarded to clipygo's event system.

```json
{"event":"incoming_message","data":{"from_name":"Alice","from_id":"abc123","content":"Hello!","format":"text","timestamp":1711900000}}
```

```json
{"event":"connection_status","data":{"status":"connected"}}
```

When an `incoming_message` event is received, clipygo shows a notification window with the sender name, content preview, and Copy/Dismiss actions.

### Error handling

clipygo auto-restarts a crashed plugin on the next request. After 3 consecutive failures the plugin is marked as errored and paused. The settings page shows a health indicator per plugin, and the popup shows a warning banner when any plugin fails to load targets.

### Demo plugin

[clipygo-plugin-demo](https://github.com/it-atelier-gn/clipygo-plugin-demo) is a minimal reference implementation. Pre-built binaries are available on its releases page, or install it directly from the plugin registry in Settings.

### Writing a plugin in Node.js

```js
const readline = require('readline');
const rl = readline.createInterface({ input: process.stdin });

rl.on('line', (line) => {
  const req = JSON.parse(line);

  if (req.command === 'get_info') {
    respond({ name: 'My Plugin', version: '1.0.0', description: '...', author: '...', link: 'https://...' });

  } else if (req.command === 'get_targets') {
    respond({
      targets: [{
        id: 'my-target',
        provider: 'My Plugin',
        formats: ['text'],
        title: 'My Target',
        description: 'Does something useful',
        image: ''
      }]
    });

  } else if (req.command === 'send') {
    // do something with req.target_id, req.content, req.format
    respond({ success: true });
  }
});

function respond(obj) {
  process.stdout.write(JSON.stringify(obj) + '\n');
}
```

### Writing a plugin in Python

```python
import sys, json

for line in sys.stdin:
    req = json.loads(line.strip())

    if req['command'] == 'get_info':
        print(json.dumps({'name': 'My Plugin', 'version': '1.0.0', 'description': '...', 'author': '...', 'link': 'https://...'}), flush=True)

    elif req['command'] == 'get_targets':
        print(json.dumps({'targets': [{'id': 'my-target', 'provider': 'My Plugin', 'formats': ['text'], 'title': 'My Target', 'description': '...', 'image': ''}]}), flush=True)

    elif req['command'] == 'send':
        # do something with req['target_id'], req['content'], req['format']
        print(json.dumps({'success': True}), flush=True)
```

---

## Plugin Registry

The built-in registry browser (Settings → Plugin Registry) lets you browse and install plugins with one click. The default registry is hosted at [it-atelier-gn/clipygo-plugins](https://github.com/it-atelier-gn/clipygo-plugins).

To publish a plugin to the registry, see the [registry README](https://github.com/it-atelier-gn/clipygo-plugins).

---

## Contributing

Contributions are welcome. For substantial changes, open an issue first to discuss the approach.

```sh
cd src-tauri && cargo check && cargo clippy
```

---

## License

MIT © 2026 Georg Nelles
