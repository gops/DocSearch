package extractor

import (
	"bytes"
	"context"
	"errors"
	"os/exec"
	"strings"
	"time"
)

// ExtractPDF extracts text content from PDF documents.
// It uses pdftotext if available on the system, and can fall back to OCR if digital text is empty.
func ExtractPDF(filePath string, ocrEnabled bool, ocrCmd string) (*Result, error) {
	// Check if pdftotext is available
	pdftotextPath, err := exec.LookPath("pdftotext")
	if err == nil {
		ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
		defer cancel()

		var outBuf, errBuf bytes.Buffer
		cmd := exec.CommandContext(ctx, pdftotextPath, "-layout", "-enc", "UTF-8", filePath, "-")
		cmd.Stdout = &outBuf
		cmd.Stderr = &errBuf

		if err := cmd.Run(); err == nil {
			text := strings.TrimSpace(outBuf.String())
			if len(text) > 20 {
				return &Result{
					Content: sanitizeText(text),
					IsOCR:   false,
				}, nil
			}
		}
	}

	// If pdftotext extracted little to no text, the PDF might be a scanned document
	if ocrEnabled {
		// Attempt OCR extraction if supported
		return ExtractPDFViaOCR(filePath, ocrCmd)
	}

	return nil, errors.New("no digital text extracted and OCR is disabled or unavailable")
}
