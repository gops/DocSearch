package desktop

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strconv"

	"docsearch/pkg/config"
	"docsearch/pkg/crawler"
	"docsearch/pkg/extractor"
	"docsearch/pkg/indexer"
	"docsearch/pkg/updater"

	wailsruntime "github.com/wailsapp/wails/v2/pkg/runtime"
)

// App manages the desktop window state, RPC bindings, and in-process API handler.
type App struct {
	ctx     context.Context
	cfg     *config.Config
	idx     *indexer.Indexer
	crawler *crawler.Crawler
	version string
}

// NewApp creates a new desktop application controller.
func NewApp(cfg *config.Config, idx *indexer.Indexer, c *crawler.Crawler, version string) *App {
	return &App{
		cfg:     cfg,
		idx:     idx,
		crawler: c,
		version: version,
	}
}

// Startup is called by Wails when the desktop application initializes.
func (a *App) Startup(ctx context.Context) {
	a.ctx = ctx
}

// Show unhides and focuses the native desktop search window.
func (a *App) Show() {
	if a.ctx != nil {
		wailsruntime.WindowShow(a.ctx)
		wailsruntime.WindowUnminimise(a.ctx)
		wailsruntime.WindowCenter(a.ctx)
	}
}

// Hide minimizes the native window to the system tray.
func (a *App) Hide() {
	if a.ctx != nil {
		wailsruntime.WindowHide(a.ctx)
	}
}

// Quit terminates the desktop application.
func (a *App) Quit() {
	if a.ctx != nil {
		wailsruntime.Quit(a.ctx)
	}
}

// HTTPHandler returns an internal in-process HTTP handler for the frontend asset server.
// This allows the desktop window to invoke API routes without opening any network ports.
func (a *App) HTTPHandler() http.Handler {
	mux := http.NewServeMux()

	mux.HandleFunc("/api/search", a.handleSearch)
	mux.HandleFunc("/api/folders", a.handleFolders)
	mux.HandleFunc("/api/reindex", a.handleReindex)
	mux.HandleFunc("/api/status", a.handleStatus)
	mux.HandleFunc("/api/open", a.handleOpen)
	mux.HandleFunc("/api/update/check", a.handleUpdateCheck)
	mux.HandleFunc("/api/update/apply", a.handleUpdateApply)
	mux.HandleFunc("/api/duplicates", a.handleDuplicates)
	mux.HandleFunc("/api/file", a.handleDeleteFile)
	mux.HandleFunc("/api/config", a.handleConfig)

	return mux
}

func (a *App) handleSearch(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	query := r.URL.Query().Get("q")
	limitStr := r.URL.Query().Get("limit")

	limit := 25
	if limitStr != "" {
		if l, err := strconv.Atoi(limitStr); err == nil && l > 0 {
			limit = l
		}
	}

	if query == "" {
		_ = json.NewEncoder(w).Encode(&indexer.SearchResult{
			Query: "",
			Items: []indexer.SearchResultItem{},
		})
		return
	}

	results, err := a.idx.Search(query, limit)
	if err != nil {
		http.Error(w, fmt.Sprintf(`{"error": %q}`, err.Error()), http.StatusInternalServerError)
		return
	}

	_ = json.NewEncoder(w).Encode(results)
}

func (a *App) handleFolders(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")

	switch r.Method {
	case http.MethodGet:
		folders := a.cfg.GetFolders()
		_ = json.NewEncoder(w).Encode(folders)

	case http.MethodPost:
		var req struct {
			Path string `json:"path"`
		}
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			http.Error(w, `{"error": "invalid request body"}`, http.StatusBadRequest)
			return
		}

		added, err := a.cfg.AddFolder(req.Path)
		if err != nil {
			http.Error(w, fmt.Sprintf(`{"error": %q}`, err.Error()), http.StatusBadRequest)
			return
		}

		if added {
			_ = a.cfg.Save()
			go a.crawler.ScanAll()
		}

		_ = json.NewEncoder(w).Encode(map[string]bool{"added": added})

	case http.MethodDelete:
		var req struct {
			Path string `json:"path"`
		}
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			http.Error(w, `{"error": "invalid request body"}`, http.StatusBadRequest)
			return
		}

		removed := a.cfg.RemoveFolder(req.Path)
		if removed {
			_ = a.cfg.Save()
		}

		_ = json.NewEncoder(w).Encode(map[string]bool{"removed": removed})

	default:
		http.Error(w, `{"error": "method not allowed"}`, http.StatusMethodNotAllowed)
	}
}

func (a *App) handleReindex(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, `{"error": "method not allowed"}`, http.StatusMethodNotAllowed)
		return
	}
	go a.crawler.ScanAll()
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(map[string]string{"status": "scanning started"})
}

func (a *App) handleStatus(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")

	crawlerStatus := a.crawler.GetStatus()
	docCount, _ := a.idx.DocCount()
	ocrAvail := extractor.IsOCRAvailable(a.cfg.OCRCommand)

	resp := map[string]interface{}{
		"is_indexing":      crawlerStatus.IsIndexing,
		"indexed_count":    crawlerStatus.IndexedCount,
		"total_files":      crawlerStatus.TotalFiles,
		"doc_count":        docCount,
		"ocr_available":    ocrAvail,
		"max_file_size_mb": a.cfg.MaxFileSizeMB,
		"indexed_folders":  a.cfg.GetFolders(),
	}

	_ = json.NewEncoder(w).Encode(resp)
}

func (a *App) handleOpen(w http.ResponseWriter, r *http.Request) {
	var req struct {
		Path   string `json:"path"`
		Reveal bool   `json:"reveal"`
	}

	if err := json.NewDecoder(r.Body).Decode(&req); err != nil || req.Path == "" {
		http.Error(w, `{"error": "invalid path"}`, http.StatusBadRequest)
		return
	}

	var cmd *exec.Cmd
	target := req.Path
	if req.Reveal {
		target = filepath.Dir(req.Path)
	}

	switch runtime.GOOS {
	case "windows":
		if req.Reveal {
			cmd = exec.Command("explorer", "/select,", req.Path)
		} else {
			cmd = exec.Command("cmd", "/c", "start", "", target)
		}
	case "darwin":
		if req.Reveal {
			cmd = exec.Command("open", "-R", req.Path)
		} else {
			cmd = exec.Command("open", target)
		}
	default:
		cmd = exec.Command("xdg-open", target)
	}

	if err := cmd.Start(); err != nil {
		http.Error(w, fmt.Sprintf(`{"error": %q}`, err.Error()), http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(map[string]bool{"success": true})
}

func (a *App) handleUpdateCheck(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")

	repo := a.cfg.GitHubRepo
	if repo == "" {
		repo = "gops/DocSearch"
	}

	rel, hasUpdate, err := updater.CheckLatestRelease(repo, a.version)
	if err != nil {
		http.Error(w, fmt.Sprintf(`{"error": %q}`, err.Error()), http.StatusInternalServerError)
		return
	}

	resp := map[string]interface{}{
		"current_version": a.version,
		"has_update":      hasUpdate,
		"release":         rel,
	}
	_ = json.NewEncoder(w).Encode(resp)
}

func (a *App) handleUpdateApply(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, `{"error": "method not allowed"}`, http.StatusMethodNotAllowed)
		return
	}

	w.Header().Set("Content-Type", "application/json")

	repo := a.cfg.GitHubRepo
	if repo == "" {
		repo = "gops/DocSearch"
	}

	rel, hasUpdate, err := updater.CheckLatestRelease(repo, a.version)
	if err != nil {
		http.Error(w, fmt.Sprintf(`{"error": %q}`, err.Error()), http.StatusInternalServerError)
		return
	}

	if !hasUpdate || rel == nil {
		http.Error(w, `{"error": "no update available"}`, http.StatusBadRequest)
		return
	}

	if err := updater.DownloadAndApply(rel); err != nil {
		http.Error(w, fmt.Sprintf(`{"error": %q}`, err.Error()), http.StatusInternalServerError)
		return
	}

	_ = json.NewEncoder(w).Encode(map[string]interface{}{
		"success": true,
		"message": fmt.Sprintf("Successfully updated to %s. Please restart DocSearch.", rel.TagName),
	})
}

func (a *App) handleDuplicates(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	report, err := a.idx.FindDuplicates()
	if err != nil {
		http.Error(w, fmt.Sprintf(`{"error": %q}`, err.Error()), http.StatusInternalServerError)
		return
	}
	_ = json.NewEncoder(w).Encode(report)
}

func (a *App) handleDeleteFile(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodDelete {
		http.Error(w, `{"error": "method not allowed"}`, http.StatusMethodNotAllowed)
		return
	}

	w.Header().Set("Content-Type", "application/json")

	var req struct {
		Path string `json:"path"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil || req.Path == "" {
		http.Error(w, `{"error": "invalid path"}`, http.StatusBadRequest)
		return
	}

	if _, err := os.Stat(req.Path); err != nil {
		http.Error(w, `{"error": "file not found"}`, http.StatusNotFound)
		return
	}

	if err := os.Remove(req.Path); err != nil {
		http.Error(w, fmt.Sprintf(`{"error": %q}`, err.Error()), http.StatusInternalServerError)
		return
	}

	_ = a.idx.DeleteDocument(req.Path)

	_ = json.NewEncoder(w).Encode(map[string]bool{"success": true})
}

func (a *App) handleConfig(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")

	switch r.Method {
	case http.MethodGet:
		resp := map[string]interface{}{
			"max_file_size_mb": a.cfg.MaxFileSizeMB,
			"indexed_folders":  a.cfg.GetFolders(),
			"ocr_enabled":      a.cfg.OCREnabled,
			"ocr_command":      a.cfg.OCRCommand,
		}
		_ = json.NewEncoder(w).Encode(resp)

	case http.MethodPost:
		var req struct {
			MaxFileSizeMB *int `json:"max_file_size_mb"`
		}
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			http.Error(w, `{"error": "invalid request"}`, http.StatusBadRequest)
			return
		}

		if req.MaxFileSizeMB != nil {
			a.cfg.SetMaxFileSizeMB(*req.MaxFileSizeMB)
			_ = a.cfg.Save()
		}

		_ = json.NewEncoder(w).Encode(map[string]interface{}{
			"success":          true,
			"max_file_size_mb": a.cfg.MaxFileSizeMB,
		})

	default:
		http.Error(w, `{"error": "method not allowed"}`, http.StatusMethodNotAllowed)
	}
}
