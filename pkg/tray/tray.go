package tray

import (
	_ "embed"
	"fmt"
	"log"
	"os"
	"os/exec"
	"runtime"

	"docsearch/pkg/config"
	"docsearch/pkg/crawler"
	"docsearch/pkg/indexer"
	"docsearch/pkg/updater"

	"fyne.io/systray"
)

//go:embed icon.png
var defaultIcon []byte

// Tray manages the system tray icon and event loop.
type Tray struct {
	cfg            *config.Config
	idx            *indexer.Indexer
	crawler        *crawler.Crawler
	version        string
	onQuitCallback func()
}

// New creates a new Tray controller.
func New(cfg *config.Config, idx *indexer.Indexer, c *crawler.Crawler, version string, onQuit func()) *Tray {
	return &Tray{
		cfg:            cfg,
		idx:            idx,
		crawler:        c,
		version:        version,
		onQuitCallback: onQuit,
	}
}

// Run launches the system tray loop. This call blocks the main thread.
func (t *Tray) Run() {
	systray.Run(t.onReady, t.onExit)
}

func (t *Tray) onReady() {
	systray.SetIcon(defaultIcon)
	systray.SetTitle("DocSearch")
	systray.SetTooltip("DocSearch — Local Document Indexer")

	mSearch := systray.AddMenuItem("🔍 Open Search UI", "Open search in default browser")
	mDuplicates := systray.AddMenuItem("📑 Find Duplicates", "View and manage duplicate documents")
	mRescan := systray.AddMenuItem("🔄 Rescan Folders", "Scan and index configured folders")
	mUpdate := systray.AddMenuItem("⚡ Check for Updates...", "Check GitHub Releases for new version")

	systray.AddSeparator()

	mStatus := systray.AddMenuItem("Docs Indexed: ...", "Current index status")
	mStatus.Disable()

	systray.AddSeparator()
	mQuit := systray.AddMenuItem("❌ Quit DocSearch", "Exit application")

	// Update status item
	go func() {
		if count, err := t.idx.DocCount(); err == nil {
			mStatus.SetTitle(fmt.Sprintf("Docs Indexed: %d", count))
		}
	}()

	uiURL := fmt.Sprintf("http://127.0.0.1:%d", t.cfg.ServerPort)

	// Event handling loop
	go func() {
		for {
			select {
			case <-mSearch.ClickedCh:
				OpenBrowser(uiURL)

			case <-mDuplicates.ClickedCh:
				OpenBrowser(uiURL + "#duplicates")

			case <-mRescan.ClickedCh:
				log.Println("[Tray] Rescan requested from tray menu.")
				go t.crawler.ScanAll()

			case <-mUpdate.ClickedCh:
				log.Println("[Tray] Checking for updates...")
				go func() {
					repo := t.cfg.GitHubRepo
					if repo == "" {
						repo = "gops/DocSearch"
					}
					rel, hasUpdate, err := updater.CheckLatestRelease(repo, t.version)
					if err != nil {
						log.Printf("[Tray] Update check failed: %v\n", err)
						return
					}
					if hasUpdate {
						log.Printf("[Tray] New version %s available at %s\n", rel.TagName, rel.HTMLURL)
						OpenBrowser(rel.HTMLURL)
					} else {
						log.Printf("[Tray] Already on the latest version (%s).\n", t.version)
					}
				}()

			case <-mQuit.ClickedCh:
				log.Println("[Tray] Quit selected.")
				systray.Quit()
				return
			}
		}
	}()
}

func (t *Tray) onExit() {
	if t.onQuitCallback != nil {
		t.onQuitCallback()
	}
}

// OpenBrowser opens a URL in the system's default web browser.
func OpenBrowser(url string) {
	var cmd *exec.Cmd
	switch runtime.GOOS {
	case "windows":
		cmd = exec.Command("rundll32", "url.dll,FileProtocolHandler", url)
	case "darwin":
		cmd = exec.Command("open", url)
	default:
		cmd = exec.Command("xdg-open", url)
	}
	_ = cmd.Start()
}

// HasDisplayEnvironment returns true if a graphical display environment is detected.
func HasDisplayEnvironment() bool {
	if runtime.GOOS == "windows" || runtime.GOOS == "darwin" {
		return true
	}
	// On Linux, verify DISPLAY or WAYLAND_DISPLAY is set
	return os.Getenv("DISPLAY") != "" || os.Getenv("WAYLAND_DISPLAY") != ""
}
