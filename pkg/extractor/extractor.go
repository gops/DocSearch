package extractor

import (
	"errors"
	"path/filepath"
	"strings"
)

// Result holds the extracted text and metadata from a document.
type Result struct {
	Content   string
	IsOCR     bool
	PageCount int
	Error     error
}

// Manager orchestrates format-specific extraction engines.
type Manager struct {
	ocrEnabled bool
	ocrCmd     string
}

// NewManager initializes an extractor manager.
func NewManager(ocrEnabled bool, ocrCmd string) *Manager {
	if ocrCmd == "" {
		ocrCmd = "tesseract"
	}
	return &Manager{
		ocrEnabled: ocrEnabled,
		ocrCmd:     ocrCmd,
	}
}

// Extract detects the file format and extracts text content.
func (m *Manager) Extract(filePath string) (*Result, error) {
	ext := strings.ToLower(filepath.Ext(filePath))

	switch ext {
	// Plain text and code formats
	case ".txt", ".md", ".markdown", ".log", ".csv", ".json",
		".yaml", ".yml", ".html", ".css", ".go", ".py", ".js",
		".ts", ".sh", ".bash", ".zsh", ".conf", ".ini", ".env":
		return ExtractPlainText(filePath)

	// Microsoft Word
	case ".docx":
		return ExtractDocx(filePath)

	// PDF Documents
	case ".pdf":
		return ExtractPDF(filePath, m.ocrEnabled, m.ocrCmd)

	// Image files requiring OCR
	case ".png", ".jpg", ".jpeg", ".tiff", ".bmp", ".webp":
		if !m.ocrEnabled {
			return nil, errors.New("OCR extraction is disabled")
		}
		return ExtractImageOCR(filePath, m.ocrCmd)

	default:
		// Attempt plain text read as fallback
		return ExtractPlainText(filePath)
	}
}
