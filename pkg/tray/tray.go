package tray

import (
	"os"
	"os/exec"
	"runtime"

	"docsearch/pkg/config"
	"docsearch/pkg/crawler"
	"docsearch/pkg/indexer"
)

// Tray manages the system tray icon and event loop.
type Tray struct {
	cfg            *config.Config
	idx            *indexer.Indexer
	crawler        *crawler.Crawler
	version        string
	onQuitCallback func()
	onOpenWindow   func()
}

// New creates a new Tray controller.
func New(cfg *config.Config, idx *indexer.Indexer, c *crawler.Crawler, version string, onQuit func(), onOpen func()) *Tray {
	return &Tray{
		cfg:            cfg,
		idx:            idx,
		crawler:        c,
		version:        version,
		onQuitCallback: onQuit,
		onOpenWindow:   onOpen,
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
