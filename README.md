# Smart Clipboard

A portable Windows clipboard manager built with [Rust](https://www.rust-lang.org/) and [egui](https://egui.rs/).

The app stays pinned on top of every window, watches your clipboard, and keeps everything you copy (including formatting) in an always-available list. Click any entry to paste it into the field you were last typing in.

## Features

- Always-on-top window that stays above any window opened before or after it
- Resizable window
- Automatically captures text copied from any app; newest copies appear at the top
- Preserves formatting (HTML/RTF) alongside plain text
- Click a row to paste the content into the last focused input field
- Per-row `X` button to remove a single entry
- `Clear All` button to wipe every saved entry
- `Format` toggle: ON pastes with formatting, OFF pastes as plain text
- Data persists across restarts in a local text file (`smart_clipboard_data.json`)
- Portable — the exe and its data file travel together in one folder

## Requirements

- Windows 10 or Windows 11
- [Rust](https://www.rust-lang.org/tools/install) (stable, MSVC toolchain: `x86_64-pc-windows-msvc`)
- Windows SDK (for `rc.exe`) — required to embed the app icon into the exe; installed by default with Visual Studio Build Tools or the Windows SDK

## Clone

```sh
git clone https://github.com/harman-codes/smart-clipboard.git
cd smart-clipboard
```

Or with SSH:

```sh
git clone git@github.com:harman-codes/smart-clipboard.git
cd smart-clipboard
```

## Build the exe

From the project folder:

```sh
cargo build --release
```

The optimized exe is written to:

```
target\release\smart-clipboard.exe
```

Copy/rename it anywhere you like — it is fully portable and creates its data file (`smart_clipboard_data.json`) next to itself on first use.

## Run it

1. Launch `Smart Clipboard.exe`.
2. Copy text in any app — it appears in the list instantly.
3. Click an entry to paste it into the input field you were last using.
4. Use the `Format` toggle to control whether pasting keeps or drops formatting.

To close, use the window's close button; all entries are saved automatically.
