package extractor

import (
	"os"
	"path/filepath"
	"testing"
)

func TestExtractPlainText(t *testing.T) {
	tmpFile := filepath.Join(os.TempDir(), "test_extract.txt")
	defer os.Remove(tmpFile)

	content := "Hello DocSearch text extractor!"
	if err := os.WriteFile(tmpFile, []byte(content), 0644); err != nil {
		t.Fatalf("Failed to create file: %v", err)
	}

	res, err := ExtractPlainText(tmpFile)
	if err != nil {
		t.Fatalf("ExtractPlainText failed: %v", err)
	}

	if res.Content != content {
		t.Fatalf("Expected %q, got %q", content, res.Content)
	}
	if res.IsOCR {
		t.Fatalf("Expected IsOCR to be false")
	}
}

func TestBinarySkip(t *testing.T) {
	tmpFile := filepath.Join(os.TempDir(), "test_binary.bin")
	defer os.Remove(tmpFile)

	// Binary with null bytes
	binaryData := []byte{0x7f, 0x45, 0x4c, 0x46, 0x00, 0x00, 0x01}
	if err := os.WriteFile(tmpFile, binaryData, 0644); err != nil {
		t.Fatalf("Failed to write binary: %v", err)
	}

	_, err := ExtractPlainText(tmpFile)
	if err == nil {
		t.Fatalf("Expected binary file to be rejected, but it succeeded")
	}
}
