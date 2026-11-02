package server

import (
	"embed"
	"encoding/json"
	"fmt"
	"io/fs"
	"log"
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
)

//go:embed static/*
var staticFS embed.FS

// Server hosts the local search UI and REST API.
type Server struct {
	cfg     *config.Config
	idx     *indexer.Indexer
	crawler *crawler.Crawler
	port    int
	version string
}

// NewServer initializes the HTTP server.
func NewServer(cfg *config.Config, idx *indexer.Indexer, c *crawler.Crawler, version string) *Server {
	port := cfg.ServerPort
	if port == 0 {
		port = 9876
	}
	return &Server{
		cfg:     cfg,
		idx:     idx,
		crawler: c,
		port:    port,
		version: version,
	}
}

// Start launches the HTTP server listening on 127.0.0.1.
func (s *Server) Start() error {
	mux := http.NewServeMux()

	// Static asset handler
	subFS, err := fs.Sub(staticFS, "static")
	if err != nil {
		return err
	}
	mux.Handle("/", http.FileServer(http.FS(subFS)))

	// API routes
	mux.HandleFunc("/api/search", s.handleSearch)
	mux.HandleFunc("/api/folders", s.handleFolders)
	mux.HandleFunc("/api/reindex", s.handleReindex)
	mux.HandleFunc("/api/status", s.handleStatus)
	mux.HandleFunc("/api/open", s.handleOpen)
	mux.HandleFunc("/api/update/check", s.handleUpdateCheck)
	mux.HandleFunc("/api/update/apply", s.handleUpdateApply)
	mux.HandleFunc("/api/duplicates", s.handleDuplicates)
	mux.HandleFunc("/api/file", s.handleDeleteFile)

	addr := fmt.Sprintf("127.0.0.1:%d", s.port)
	log.Printf("[Server] DocSearch UI active at http://%s\n", addr)
	return http.ListenAndServe(addr, mux)
}

func (s *Server) handleSearch(w http.ResponseWriter, r *http.Request) {
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

	results, err := s.idx.Search(query, limit)
	if err != nil {
		http.Error(w, fmt.Sprintf(`{"error": %q}`, err.Error()), http.StatusInternalServerError)
		return
	}

	_ = json.NewEncoder(w).Encode(results)
}

func (s *Server) handleFolders(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")

	switch r.Method {
	case http.MethodGet:
		folders := s.cfg.GetFolders()
		_ = json.NewEncoder(w).Encode(folders)

	case http.MethodPost:
		var req struct {
			Path string `json:"path"`
		}
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			http.Error(w, `{"error": "invalid request body"}`, http.StatusBadRequest)
			return
		}

		added, err := s.cfg.AddFolder(req.Path)
		if err != nil {
			http.Error(w, fmt.Sprintf(`{"error": %q}`, err.Error()), http.StatusBadRequest)
			return
		}

		if added {
			_ = s.cfg.Save()
			// Rescan in background
			go s.crawler.ScanAll()
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

		removed := s.cfg.RemoveFolder(req.Path)
		if removed {
			_ = s.cfg.Save()
		}

		_ = json.NewEncoder(w).Encode(map[string]bool{"removed": removed})

	default:
		http.Error(w, `{"error": "method not allowed"}`, http.StatusMethodNotAllowed)
	}
}

func (s *Server) handleReindex(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, `{"error": "method not allowed"}`, http.StatusMethodNotAllowed)
		return
	}
	go s.crawler.ScanAll()
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(map[string]string{"status": "scanning started"})
}

func (s *Server) handleStatus(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")

	crawlerStatus := s.crawler.GetStatus()
	docCount, _ := s.idx.DocCount()
	ocrAvail := extractor.IsOCRAvailable(s.cfg.OCRCommand)

	resp := map[string]interface{}{
		"is_indexing":    crawlerStatus.IsIndexing,
		"indexed_count":  crawlerStatus.IndexedCount,
		"total_files":    crawlerStatus.TotalFiles,
		"doc_count":      docCount,
		"ocr_available":  ocrAvail,
		"indexed_folders": s.cfg.GetFolders(),
	}

	_ = json.NewEncoder(w).Encode(resp)
}

func (s *Server) handleOpen(w http.ResponseWriter, r *http.Request) {
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
	default: // Linux and others
		cmd = exec.Command("xdg-open", target)
	}

	if err := cmd.Start(); err != nil {
		log.Printf("[Server] Failed to open path %s: %v\n", target, err)
		http.Error(w, fmt.Sprintf(`{"error": %q}`, err.Error()), http.StatusInternalServerError)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(map[string]bool{"success": true})
}

func (s *Server) handleUpdateCheck(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")

	repo := s.cfg.GitHubRepo
	if repo == "" {
		repo = "gops/DocSearch"
	}

	rel, hasUpdate, err := updater.CheckLatestRelease(repo, s.version)
	if err != nil {
		http.Error(w, fmt.Sprintf(`{"error": %q}`, err.Error()), http.StatusInternalServerError)
		return
	}

	resp := map[string]interface{}{
		"current_version": s.version,
		"has_update":      hasUpdate,
		"release":         rel,
	}
	_ = json.NewEncoder(w).Encode(resp)
}

func (s *Server) handleUpdateApply(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, `{"error": "method not allowed"}`, http.StatusMethodNotAllowed)
		return
	}

	w.Header().Set("Content-Type", "application/json")

	repo := s.cfg.GitHubRepo
	if repo == "" {
		repo = "gops/DocSearch"
	}

	rel, hasUpdate, err := updater.CheckLatestRelease(repo, s.version)
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

func (s *Server) handleDuplicates(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	report, err := s.idx.FindDuplicates()
	if err != nil {
		http.Error(w, fmt.Sprintf(`{"error": %q}`, err.Error()), http.StatusInternalServerError)
		return
	}
	_ = json.NewEncoder(w).Encode(report)
}

func (s *Server) handleDeleteFile(w http.ResponseWriter, r *http.Request) {
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

	_ = s.idx.DeleteDocument(req.Path)

	_ = json.NewEncoder(w).Encode(map[string]bool{"success": true})
}
