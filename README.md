# DocSearch 🔍

> **Cross-platform, local document indexing & search application written in Go with Bluge, OCR support, system tray daemon, and GitHub Releases self-updating.**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Go Version](https://img.shields.io/badge/Go-1.27+-00ADD8.svg)](https://golang.org)
[![CI](https://github.com/gops/DocSearch/actions/workflows/ci.yml/badge.svg)](https://github.com/gops/DocSearch/actions)

DocSearch indexes your local documents, PDFs, Word files, code, and scanned images (via OCR). It runs unobtrusively in the background system tray, only crawling user-specified directories, and provides sub-millisecond search with keyword highlighting and in-place GitHub self-updating.

---

> [!WARNING]
> ### Disclaimer
> **This codebase is mostly AI-generated code. Use at your own risk.** It is provided "as is", without warranty of any kind, express or implied, including but not limited to the warranties of merchantability, fitness for a particular purpose, and non-infringement. Please test and review thoroughly before deploying in production or critical environments.

---

## Key Features

- **Selective Folder Indexing:** Only scans and monitors folders you explicitly select. Does not index your entire drive or slow down your system.
- **Large File Protection:** Automatically skips files exceeding the configured size limit (defaults to 25 MB, customizable via CLI, UI, or config; set to 0 for unlimited) to prevent memory spikes.
- **Duplicate Document Detection:** Cryptographic SHA-256 content hashing instantly finds duplicate documents across all monitored directories, calculates wasted disk space, and lets you open or remove redundant copies via CLI and UI.
- **System Tray Daemon:** Sits in your OS system tray (Windows notification area, macOS menu bar, Linux indicator) with quick access to search, folder re-scans, and update checks.
- **Self-Updating via GitHub Releases:** Checks for latest binary releases from GitHub, verifies SHA-256 checksums, and applies atomic in-place updates.
- **Pure Go Search Core:** Powered by [Bluge](https://github.com/blugelabs/bluge) for embedded, high-performance BM25 full-text indexing, prefix matching, and hit highlights.
- **Real-Time Live Watching:** Integrated [`fsnotify`](https://github.com/fsnotify/fsnotify) detects file creations, edits, and deletions instantly.
- **Smart Change Detection:** Hash-based comparison (SHA-256) prevents re-indexing unchanged files, saving CPU cycles.
- **Broad Document Extraction:**
  - **Plain Text / Code:** `.txt`, `.md`, `.log`, `.csv`, `.json`, `.yaml`, `.go`, `.py`, `.js`, `.ts`, `.html`, etc.
  - **Word Documents:** Pure Go `.docx` stream XML extractor (zero external C dependencies).
  - **PDFs:** Digital text extraction via `pdftotext` with automatic fallback to OCR.
  - **Scanned Images (OCR):** Pluggable OCR interface supporting Tesseract or system OCR for `.png`, `.jpg`, `.jpeg`, `.tiff`.
- **Dual Interface:**
  - **Embedded Desktop / Web UI:** Responsive dark-themed search interface with live highlighted snippets, keyboard navigation, duplicate management, and one-click file opening.
  - **CLI Mode:** Instant search, directory management, duplicate reports, and status checks directly from your terminal.
- **100% Offline & Private:** Zero telemetry, no cloud API keys, all data stays locally on your machine.
- **MIT Licensed:** Free and open source.

---

## Architecture Overview

```
DocSearch
├── cmd/
│   └── docsearch/         # Application entry point & CLI commands
├── pkg/
│   ├── config/            # JSON config management & folder list
│   ├── indexer/           # Bluge search engine, BM25 ranking, highlighter
│   ├── crawler/           # Directory walker, worker pool, and fsnotify watcher
│   ├── extractor/         # Text & OCR parsers (Plain text, DOCX, PDF, OCR)
│   ├── updater/           # GitHub Releases self-updater & checksum verifier
│   ├── tray/              # Cross-platform system tray integration (fyne.io/systray)
│   └── server/            # Local HTTP server & embedded HTML/CSS/JS UI
├── .github/workflows/     # CI tests and automated GoReleaser release pipeline
├── .goreleaser.yaml       # Multi-platform release configuration
├── test_docs/             # Sample documents for verification
└── LICENSE                # MIT License
```

---

## Installation on Linux (Ubuntu / Debian / Linux Mint / Pop!_OS)

### Option A: Install via APT Repository (Auto-Updates with `apt upgrade`)
Configure the official DocSearch APT repository to receive automated updates alongside your system packages:

```bash
# 1. Register DocSearch APT source
echo "deb [trusted=yes] https://iam.patelgopal.com/DocSearch/ ./" | sudo tee /etc/apt/sources.list.d/docsearch.list

# 2. Update and install
sudo apt update
sudo apt install docsearch
```

Any future `sudo apt update && sudo apt upgrade` will automatically update DocSearch!

---

### Option B: Download .deb / .rpm (No Terminal Required)

For graphical desktop users on Linux:

1. Go to **[GitHub Releases](https://github.com/gops/DocSearch/releases)** and download the package for your distribution:
   * **Ubuntu / Debian / Linux Mint / Pop!_OS**: Download `docsearch_<version>_linux_amd64.deb`
   * **Fedora / RHEL / openSUSE**: Download `docsearch_<version>_linux_amd64.rpm`
2. In your file manager, **double-click the downloaded `.deb` file**.
3. Click the **Install** button in your App Center (or GDebi).
4. Launch **DocSearch** directly from your application launcher or application menu.
5. DocSearch will appear in your system tray and start running quietly.

---

## Quick Start & Build (CLI / Developers)

### 1. Build the Binary
```bash
go build -o docsearch ./cmd/docsearch
```
*(Produces a single, compact ~14MB executable).*

### 2. Configure Folders to Index
By default, DocSearch only indexes the folders you explicitly configure:
```bash
# Add a folder to index
./docsearch add /path/to/my/documents

# List all currently configured folders and total indexed documents
./docsearch list
```

### 3. Run with System Tray
```bash
./docsearch
```
- A search icon appears in your system tray.
- Right-click the tray icon to open the search UI, trigger folder rescans, check for updates, or quit.
- To run in background terminal mode without a tray icon, pass `--headless`:
```bash
./docsearch --headless
```

### 4. Search from the Command Line (CLI)
You can also run instant terminal searches without opening the UI:
```bash
./docsearch search "financial report"
./docsearch search "cross platform"
```

### 5. Find Duplicate Files
Scan all indexed directories for duplicate documents and wasted disk space:
```bash
./docsearch duplicates
```

### 6. Check and Apply Updates
```bash
# Check if a new version is available on GitHub
./docsearch check-update

# Automatically download, verify, and update the binary
./docsearch update
```
You can also click the **⚡ Update** button in the Web UI.

---

## Automated Releases (GoReleaser + GitHub Actions)

DocSearch uses [GoReleaser](https://goreleaser.com) and GitHub Actions to automatically cross-compile and publish binaries:

1. Push a semantic version tag:
   ```bash
   git tag -a v0.2.0 -m "Release v0.2.0"
   git push origin v0.2.0
   ```
2. The GitHub Action will automatically:
   - Build binaries for **Linux** (`amd64`, `arm64`), **macOS** (`amd64`, `arm64`), and **Windows** (`amd64`).
   - Package `.tar.gz` and `.zip` archives.
   - Generate `checksums.txt` (SHA-256).
   - Create a GitHub Release with release notes.

---

## Configuration (`config.json`)

DocSearch stores its configuration in `~/.config/docsearch/config.json` (or `./data/config.json` in portable mode):

```json
{
  "indexed_folders": [
    "/path/to/my/documents",
    "/path/to/projects/notes"
  ],
  "index_path": "~/.local/share/docsearch/bluge_index",
  "server_port": 9876,
  "max_file_size_mb": 25,
  "ocr_enabled": true,
  "ocr_command": "tesseract",
  "github_repo": "gops/DocSearch",
  "file_extensions": [
    ".txt", ".md", ".pdf", ".docx", ".csv", ".log", ".json", ".png", ".jpg"
  ],
  "excluded_dirs": [
    ".git", "node_modules", ".cache", "vendor", "tmp"
  ]
}
```

---

## Testing

Run the full automated test suite:
```bash
go test -v ./...
```

---

## License

This project is licensed under the [MIT License](LICENSE).
