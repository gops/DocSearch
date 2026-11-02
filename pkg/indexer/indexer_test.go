package indexer

import (
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestIndexerLifecycle(t *testing.T) {
	tempDir := filepath.Join(os.TempDir(), "test_docsearch_index")
	_ = os.RemoveAll(tempDir)
	defer os.RemoveAll(tempDir)

	idx, err := Open(tempDir)
	if err != nil {
		t.Fatalf("Open failed: %v", err)
	}
	defer idx.Close()

	doc := &Document{
		ID:        "/path/to/test.md",
		Path:      "/path/to/test.md",
		Filename:  "test.md",
		Extension: ".md",
		Size:      1234,
		ModTime:   time.Now(),
		Content:   "Antigravity search engine with Bluge integration.",
		Hash:      "abc123hash",
		IsOCR:     false,
	}

	if err := idx.IndexDocument(doc); err != nil {
		t.Fatalf("IndexDocument failed: %v", err)
	}

	count, err := idx.DocCount()
	if err != nil {
		t.Fatalf("DocCount failed: %v", err)
	}
	if count != 1 {
		t.Fatalf("Expected 1 document, got %d", count)
	}

	hash, err := idx.GetDocumentHash("/path/to/test.md")
	if err != nil {
		t.Fatalf("GetDocumentHash failed: %v", err)
	}
	if hash != "abc123hash" {
		t.Fatalf("Expected hash abc123hash, got %q", hash)
	}

	res, err := idx.Search("Antigravity", 10)
	if err != nil {
		t.Fatalf("Search failed: %v", err)
	}
	if res.TotalHits != 1 {
		t.Fatalf("Expected 1 hit, got %d", res.TotalHits)
	}

	if len(res.Items[0].Highlights) == 0 {
		t.Fatalf("Expected highlight snippet, got none")
	}

	// Test delete
	if err := idx.DeleteDocument("/path/to/test.md"); err != nil {
		t.Fatalf("DeleteDocument failed: %v", err)
	}

	countAfter, _ := idx.DocCount()
	if countAfter != 0 {
		t.Fatalf("Expected 0 documents after delete, got %d", countAfter)
	}
}
