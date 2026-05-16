# clipygo

[![Build](https://github.com/it-atelier-gn/clipygo/actions/workflows/ci.yml/badge.svg)](https://github.com/it-atelier-gn/clipygo/actions)
[![Rust](https://img.shields.io/badge/rust-1.80%2B-orange?logo=rust)](https://www.rust-lang.org/)
[![Tauri](https://img.shields.io/badge/tauri-2.x-blue?logo=tauri)](https://tauri.app/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/it-atelier-gn/clipygo)

<p align="center">
  <a href="https://discord.gg/AJxzNdkw">
    <img src="https://img.shields.io/badge/Join%20the%20clipygo%20Discord-5865F2?style=for-the-badge&logo=discord&logoColor=white" alt="Join the clipygo Discord" height="40">
  </a>
</p>

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
