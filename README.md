# clipygo

[![Build](https://github.com/it-atelier-gn/clipygo/actions/workflows/ci.yml/badge.svg)](https://github.com/it-atelier-gn/clipygo/actions)
[![Rust](https://img.shields.io/badge/rust-1.80%2B-orange?logo=rust)](https://www.rust-lang.org/)
[![Tauri](https://img.shields.io/badge/tauri-2.x-blue?logo=tauri)](https://tauri.app/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

Clipboard monitor that watches for specific content patterns and lets you route them to configured targets.

It sits in your system tray, monitors the clipboard for regex matches (meeting links, Code With Me sessions, etc.), and pops up a compact window where you pick a target and hit Enter. Plugins handle the actual delivery.

An optional clipboard history functionality is also included. It captures plain text, rich text (HTML and RTF), images (PNG), and copied files, and lets you filter by type, search, pin, and re-copy or re-send any entry. Multi-line entries show a multi-line preview with a line count so they are easy to tell apart at a glance.

**Morph** transforms clipboard text in place: rule-based automatic rewrites (regex match → built-in transform or regex replace) plus an on-demand, keyboard-driven picker (default `Ctrl+Shift+M`) for one-off transformations. Built-ins cover URL tracking strip, JSON/XML formatting, Base64/URL encoding, case conversion, slugify, accent/quote normalization, and line operations. Rules can be authored and tested live in Settings.

**Execute** launches external commands against the clipboard. Configure commands (executable path, arguments, working directory, optional matching regex) in Settings, then press the hotkey (default `Ctrl+Shift+E`): if exactly one command's pattern matches the clipboard it runs directly, otherwise a keyboard-driven picker opens. The clipboard can be injected into arguments via a `{clipboard}` placeholder and/or piped to the command's standard input.

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
