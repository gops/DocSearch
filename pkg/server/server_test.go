package server

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"
	"time"

	"docsearch/pkg/config"
	"docsearch/pkg/crawler"
	"docsearch/pkg/extractor"
	"docsearch/pkg/indexer"
)

func TestServerAPI(t *testing.T) {
	tempDir := filepath.Join(os.TempDir(), "test_server_suite")
	_ = os.RemoveAll(tempDir)
	defer os.RemoveAll(tempDir)

	idxDir := filepath.Join(tempDir, "idx")
	cfg := config.DefaultConfig()
	cfg.IndexPath = idxDir
	cfg.IndexedFolders = []string{tempDir}

	idx, err := indexer.Open(idxDir)
	if err != nil {
		t.Fatalf("Open indexer failed: %v", err)
	}
	defer idx.Close()

	// Add sample document
	_ = idx.IndexDocument(&indexer.Document{
		ID:        "/sample/file.txt",
		Path:      "/sample/file.txt",
		Filename:  "file.txt",
		Extension: ".txt",
		Size:      100,
		ModTime:   time.Now(),
		Content:   "Testing local document search HTTP API.",
		Hash:      "hash123",
	})

	ext := extractor.NewManager(false, "")
	c, err := crawler.New(cfg, idx, ext)
	if err != nil {
		t.Fatalf("Crawler init failed: %v", err)
	}
	defer c.Stop()

	srv := NewServer(cfg, idx, c, "0.1.0")

	// 1. Test /api/status
	reqStatus := httptest.NewRequest(http.MethodGet, "/api/status", nil)
	wStatus := httptest.NewRecorder()
	srv.handleStatus(wStatus, reqStatus)

	if wStatus.Code != http.StatusOK {
		t.Fatalf("Expected 200 OK from /api/status, got %d", wStatus.Code)
	}

	var statusResp map[string]interface{}
	if err := json.Unmarshal(wStatus.Body.Bytes(), &statusResp); err != nil {
		t.Fatalf("Failed to parse status JSON: %v", err)
	}
	if statusResp["doc_count"].(float64) != 1 {
		t.Fatalf("Expected doc_count 1, got %v", statusResp["doc_count"])
	}

	// 2. Test /api/search
	reqSearch := httptest.NewRequest(http.MethodGet, "/api/search?q=Testing", nil)
	wSearch := httptest.NewRecorder()
	srv.handleSearch(wSearch, reqSearch)

	if wSearch.Code != http.StatusOK {
		t.Fatalf("Expected 200 OK from /api/search, got %d", wSearch.Code)
	}

	var searchResp indexer.SearchResult
	if err := json.Unmarshal(wSearch.Body.Bytes(), &searchResp); err != nil {
		t.Fatalf("Failed to parse search JSON: %v", err)
	}
	if searchResp.TotalHits != 1 {
		t.Fatalf("Expected 1 match, got %d", searchResp.TotalHits)
	}

	// 3. Test /api/folders
	reqFolders := httptest.NewRequest(http.MethodGet, "/api/folders", nil)
	wFolders := httptest.NewRecorder()
	srv.handleFolders(wFolders, reqFolders)

	if wFolders.Code != http.StatusOK {
		t.Fatalf("Expected 200 OK from /api/folders, got %d", wFolders.Code)
	}

	// 4. Test /api/duplicates
	reqDupes := httptest.NewRequest(http.MethodGet, "/api/duplicates", nil)
	wDupes := httptest.NewRecorder()
	srv.handleDuplicates(wDupes, reqDupes)

	if wDupes.Code != http.StatusOK {
		t.Fatalf("Expected 200 OK from /api/duplicates, got %d", wDupes.Code)
	}

	var dupeReport indexer.DuplicateReport
	if err := json.Unmarshal(wDupes.Body.Bytes(), &dupeReport); err != nil {
		t.Fatalf("Failed to parse duplicates JSON: %v", err)
	}
}
