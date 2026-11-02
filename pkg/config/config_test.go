package config

import (
	"testing"
)

func TestMaxFileSize(t *testing.T) {
	cfg := DefaultConfig()
	if cfg.MaxFileSizeMB != 25 {
		t.Fatalf("Expected default MaxFileSizeMB to be 25, got %d", cfg.MaxFileSizeMB)
	}

	expectedBytes := int64(25 * 1024 * 1024)
	if cfg.GetMaxFileSizeBytes() != expectedBytes {
		t.Fatalf("Expected %d bytes, got %d", expectedBytes, cfg.GetMaxFileSizeBytes())
	}

	// 10MB file should not be too large
	if cfg.IsFileTooLarge(10 * 1024 * 1024) {
		t.Fatalf("10MB file should not be considered too large")
	}

	// 30MB file should be too large
	if !cfg.IsFileTooLarge(30 * 1024 * 1024) {
		t.Fatalf("30MB file should be considered too large")
	}

	// Setting to 0 (unlimited)
	cfg.SetMaxFileSizeMB(0)
	if cfg.IsFileTooLarge(100 * 1024 * 1024) {
		t.Fatalf("When limit is 0 (unlimited), 100MB should not be too large")
	}
}
