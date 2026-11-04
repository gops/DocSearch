//go:build (linux || windows) || (darwin && cgo)

package tray

import (
	_ "embed"
	"fmt"
	"log"

	"docsearch/pkg/updater"

	"fyne.io/systray"
)

//go:embed icon.png
var defaultIcon []byte

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

	port := t.cfg.ServerPort
	if port <= 0 {
		port = 9876
	}
	uiURL := fmt.Sprintf("http://127.0.0.1:%d", port)

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
