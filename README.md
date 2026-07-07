# clipygo

[![Build](https://github.com/it-atelier-gn/clipygo/actions/workflows/ci.yml/badge.svg)](https://github.com/it-atelier-gn/clipygo/actions)
[![Rust](https://img.shields.io/badge/rust-1.80%2B-orange?logo=rust)](https://www.rust-lang.org/)
[![Tauri](https://img.shields.io/badge/tauri-2.x-blue?logo=tauri)](https://tauri.app/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

Clipboard monitor that watches for specific content patterns and lets you route them to configured targets.

It sits in your system tray, monitors the clipboard for regex matches (meeting links, Code With Me sessions, etc.), and pops up a compact window where you pick a target and hit Enter. Plugins handle the actual delivery.

**History** stores plain text, rich text (HTML/RTF), images (PNG), and copied files. When a copy carries several representations at once (e.g. text + HTML + RTF from Word or a browser), all of them are captured together and restored together, so re-copying an item pastes formatted where formatting is supported and as plain text elsewhere. You can filter, search, pin, and re-copy or re-send any item. Default hotkey: Ctrl+Shift+H.

**Morph** rewrites clipboard text using rules or an on‑demand picker (Default hotkey: Ctrl+Shift+M). Built‑in transforms include URL tracking removal, JSON/XML formatting, Base64/URL encoding, case changes, slugify, accent/quote normalization, and line tools. Custom rules can be created and tested in Settings.

**Execute** runs external commands on the clipboard. Configure commands (path, args, working directory, optional match regex) in Settings, then press a hotkey (Default hotkey: Ctrl+Shift+E). If one command matches, it runs; otherwise you pick from a list. Use {clipboard} in arguments or pipe clipboard content to stdin.

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

Everything is configured through the in-app settings window (tray icon → Settings). Config is stored at `%APPDATA%\clipygo\config.json`.

---

## Plugins

clipygo is extended through plugins — any executable that speaks JSON over stdin/stdout. Plugins provide targets, handle delivery, and can push real-time events back to clipygo. See the [plugin docs](docs/plugins.md) for the full protocol reference, examples, and how to write your own.

---

## Contributing

Contributions are welcome. For substantial changes, open an issue first to discuss the approach.

```sh
cd src-tauri && cargo check && cargo clippy
```

---

## License

MIT © 2026 Georg Nelles
