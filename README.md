# DocSearch 🔍

> **Cross-platform, local document indexing & search application written in Rust with Slint Native GUI, Tantivy BM25 search engine, native OS folder picker, OCR support, and system tray daemon.**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.75+-orange.svg)](https://www.rust-lang.org)
[![Slint](https://img.shields.io/badge/GUI-Slint-blueviolet.svg)](https://slint.dev)

DocSearch indexes your local documents, PDFs, Word files, code, and scanned images (via OCR). It features a **native desktop GUI built with Slint** (replacing embedded web browsers) alongside a complete CLI and background system tray daemon.

---

> [!WARNING]
> ### Disclaimer
> **This codebase is mostly AI-generated code. Use at your own risk.** It is provided "as is", without warranty of any kind, express or implied, including but not limited to the warranties of merchantability, fitness for a particular purpose, and non-infringement. Please test and review thoroughly before deploying in production or critical environments.

---

## Key Features

- **Native Slint Desktop GUI:** Fast, lightweight, GPU/software-rendered native desktop interface with responsive search-as-you-type, snippet highlighting, and dark mode.
- **Native OS Folder Picker:** Uses the native operating system folder dialog (`rfd` / XDG Portal) to add directories directly without typing paths manually.
- **Selective Folder Indexing:** Only scans and monitors folders you explicitly select. Does not index your entire drive or slow down your system.
- **Large File Protection:** Automatically skips files exceeding the configured size limit (defaults to 25 MB, customizable via CLI, UI, or config; set to 0 for unlimited) to prevent memory spikes.
- **Duplicate Document Detection:** Cryptographic SHA-256 content hashing instantly finds duplicate documents across all monitored directories, calculates wasted disk space, and lets you open or remove redundant copies via CLI and UI.
- **System Tray Daemon:** Sits in your OS system tray (StatusNotifierItem / SNI on Linux, Windows, macOS) with quick access to search, folder re-scans, and application exit.
- **Pure Rust Search Core:** Powered by [Tantivy](https://github.com/quickwit-oss/tantivy) for embedded, high-performance BM25 full-text indexing, field boosting, and snippet highlighting.
- **Real-Time Live Watching:** Integrated [`notify`](https://github.com/notify-rs/notify) detects file creations, edits, and deletions instantly with debouncing.
- **Smart Change Detection:** Hash-based comparison (SHA-256) prevents re-indexing unchanged files, saving CPU cycles.
- **Broad Document Extraction:**
  - **Plain Text / Code:** `.txt`, `.md`, `.log`, `.csv`, `.json`, `.yaml`, `.rs`, `.go`, `.py`, `.js`, `.ts`, `.html`, etc.
  - **Word Documents:** Pure Rust `.docx` stream XML extractor (zero external C dependencies).
  - **PDFs:** Digital text extraction via `pdftotext` with automatic fallback to OCR.
  - **Scanned Images (OCR):** Pluggable OCR interface supporting Tesseract for `.png`, `.jpg`, `.jpeg`, `.tiff`, `.bmp`, `.webp`.
- **Dual Interface:**
  - **Native Slint GUI:** Dark-themed search interface with live highlighted snippets, folder management drawer, and duplicate inspector.
  - **CLI Mode:** Instant search, directory management, duplicate reports, and status checks directly from your terminal.
- **100% Offline & Private:** Zero telemetry, no cloud API keys, all data stays locally on your machine.
- **MIT Licensed:** Free and open source.

---

## Architecture Overview

```
DocSearch
├── Cargo.toml             # Rust package manifest & dependencies
├── build.rs               # Slint UI compiler build script
├── ui/
│   └── app.slint          # Native Slint GUI definitions and dark theme layout
├── src/
│   ├── main.rs            # Application entry point, Slint bindings, & CLI dispatch
│   ├── lib.rs             # Core library exports
│   ├── config.rs          # JSON configuration management & folder tracking
│   ├── cli.rs             # Clap CLI command line parser
│   ├── indexer/           # Tantivy search engine, BM25 ranking, & duplicate scanner
│   ├── crawler/           # Directory walker, live notify watcher, & SHA-256 hashing
│   ├── extractor/         # Text, DOCX, PDF, and OCR parsers
│   └── tray.rs            # Background system tray daemon (ksni)
├── test_docs/             # Sample documents for verification
├── tests/                 # Integration test suites (extractors, indexer, config)
└── LICENSE                # MIT License
```

---

## Quick Start & Build (Rust / Cargo)

### 1. Build the Binary
```bash
cargo build --release
```
*(Produces a standalone, optimized native binary at `target/release/docsearch`).*

### 2. Configure Folders to Index
By default, DocSearch only indexes the folders you explicitly configure:
```bash
# Add a folder to index
./target/release/docsearch add /path/to/my/documents

# List all currently configured folders and total indexed documents
./target/release/docsearch list
```

### 3. Run with Native Slint GUI & System Tray
```bash
./target/release/docsearch
```
- Launches the native Slint desktop window.
- DocSearch sits in your system tray for quick access.
- Click **"📂 + Add Folder"** to open your native OS folder chooser dialog.
- To run in background daemon mode without opening the GUI window, pass `--headless`:
```bash
./target/release/docsearch --headless
```

### 4. Search from the Command Line (CLI)
You can run instant terminal searches without opening the GUI:
```bash
./target/release/docsearch search "financial report"
./target/release/docsearch search "cross platform"
```

### 5. Find Duplicate Files
Scan all indexed directories for duplicate documents and wasted disk space:
```bash
./target/release/docsearch duplicates
```

---

## Configuration (`config.json`)

DocSearch stores its configuration in `~/.config/docsearch/config.json` (or `./data/config.json` in portable mode):

```json
{
  "indexed_folders": [
    "/path/to/my/documents",
    "/path/to/projects/notes"
  ],
  "index_path": "~/.local/share/docsearch/tantivy_index",
  "max_file_size_mb": 25,
  "ocr_enabled": true,
  "ocr_command": "tesseract",
  "github_repo": "gops/DocSearch",
  "file_extensions": [
    ".txt", ".md", ".pdf", ".docx", ".csv", ".log", ".json", ".png", ".jpg", ".rs", ".go"
  ],
  "excluded_dirs": [
    ".git", "node_modules", ".cache", "vendor", "tmp", "target"
  ]
}
```

---

## Testing

Run the full automated test suite:
```bash
cargo test
```

---

## License

This project is licensed under the [MIT License](LICENSE).
