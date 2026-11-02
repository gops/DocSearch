//go:build darwin && !cgo

package tray

import (
	"log"
	"os"
	"os/signal"
	"syscall"
)

// Run runs as a background service on macOS when cross-compiled without CGO.
func (t *Tray) Run() {
	log.Println("[Tray] Running in daemon mode (macOS native menu bar requires local compilation with CGO).")
	sigChan := make(chan os.Signal, 1)
	signal.Notify(sigChan, os.Interrupt, syscall.SIGTERM)
	<-sigChan

	if t.onQuitCallback != nil {
		t.onQuitCallback()
	}
}
