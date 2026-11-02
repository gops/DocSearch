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
func TestFindDuplicates(t *testing.T) {
	tempDir := filepath.Join(os.TempDir(), "test_docsearch_dups")
	_ = os.RemoveAll(tempDir)
	defer os.RemoveAll(tempDir)

	idx, err := Open(tempDir)
	if err != nil {
		t.Fatalf("Open failed: %v", err)
	}
	defer idx.Close()

	// 2 copies of file A (1000 bytes each)
	_ = idx.IndexDocument(&Document{
		ID:        "/docs/fileA.txt",
		Path:      "/docs/fileA.txt",
		Filename:  "fileA.txt",
		Extension: ".txt",
		Size:      1000,
		ModTime:   time.Now(),
		Content:   "Content of file A",
		Hash:      "hashA",
	})
	_ = idx.IndexDocument(&Document{
		ID:        "/backup/fileA_copy.txt",
		Path:      "/backup/fileA_copy.txt",
		Filename:  "fileA_copy.txt",
		Extension: ".txt",
		Size:      1000,
		ModTime:   time.Now(),
		Content:   "Content of file A",
		Hash:      "hashA",
	})

	// 1 unique file B
	_ = idx.IndexDocument(&Document{
		ID:        "/docs/fileB.txt",
		Path:      "/docs/fileB.txt",
		Filename:  "fileB.txt",
		Extension: ".txt",
		Size:      5000,
		ModTime:   time.Now(),
		Content:   "Unique content B",
		Hash:      "hashB",
	})

	report, err := idx.FindDuplicates()
	if err != nil {
		t.Fatalf("FindDuplicates failed: %v", err)
	}

	if report.TotalGroups != 1 {
		t.Fatalf("Expected 1 duplicate group, got %d", report.TotalGroups)
	}
	if report.TotalDuplicates != 1 {
		t.Fatalf("Expected 1 duplicate file, got %d", report.TotalDuplicates)
	}
	if report.TotalWastedBytes != 1000 {
		t.Fatalf("Expected 1000 wasted bytes, got %d", report.TotalWastedBytes)
	}
	if len(report.Groups[0].Files) != 2 {
		t.Fatalf("Expected 2 files in duplicate group, got %d", len(report.Groups[0].Files))
	}
}
