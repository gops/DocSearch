package extractor

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"os/exec"
	"strings"
	"time"
)

// ExtractImageOCR runs OCR on an image file using the configured OCR binary (e.g. tesseract).
func ExtractImageOCR(imagePath string, ocrCmd string) (*Result, error) {
	if ocrCmd == "" {
		ocrCmd = "tesseract"
	}

	binPath, err := exec.LookPath(ocrCmd)
	if err != nil {
		return nil, fmt.Errorf("OCR tool %q not found in system PATH: %w", ocrCmd, err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 60*time.Second)
	defer cancel()

	var stdout, stderr bytes.Buffer
	// tesseract <input> stdout
	cmd := exec.CommandContext(ctx, binPath, imagePath, "stdout", "--oem", "1")
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr

	if err := cmd.Run(); err != nil {
		// Try without extra arguments
		var stdoutRetry, stderrRetry bytes.Buffer
		retryCmd := exec.CommandContext(ctx, binPath, imagePath, "stdout")
		retryCmd.Stdout = &stdoutRetry
		retryCmd.Stderr = &stderrRetry
		if retryErr := retryCmd.Run(); retryErr != nil {
			return nil, fmt.Errorf("OCR execution failed: %v (details: %s)", retryErr, stderrRetry.String())
		}
		stdout = stdoutRetry
	}

	text := strings.TrimSpace(stdout.String())
	if text == "" {
		return nil, errors.New("OCR completed but produced empty text")
	}

	return &Result{
		Content: sanitizeText(text),
		IsOCR:   true,
	}, nil
}

// ExtractPDFViaOCR extracts text from scanned PDFs by rendering or direct tesseract pdf processing.
func ExtractPDFViaOCR(pdfPath string, ocrCmd string) (*Result, error) {
	if ocrCmd == "" {
		ocrCmd = "tesseract"
	}

	binPath, err := exec.LookPath(ocrCmd)
	if err != nil {
		return nil, fmt.Errorf("OCR engine %q not installed: %w", ocrCmd, err)
	}

	ctx, cancel := context.WithTimeout(context.Background(), 120*time.Second)
	defer cancel()

	var stdout, stderr bytes.Buffer
	// Modern Tesseract supports direct PDF input
	cmd := exec.CommandContext(ctx, binPath, pdfPath, "stdout")
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr

	if err := cmd.Run(); err != nil {
		return nil, fmt.Errorf("PDF OCR failed: %v (%s)", err, stderr.String())
	}

	text := strings.TrimSpace(stdout.String())
	if text == "" {
		return nil, errors.New("PDF OCR produced empty text")
	}

	return &Result{
		Content: sanitizeText(text),
		IsOCR:   true,
	}, nil
}

// IsOCRAvailable checks whether the configured OCR tool is installed and reachable.
func IsOCRAvailable(ocrCmd string) bool {
	if ocrCmd == "" {
		ocrCmd = "tesseract"
	}
	_, err := exec.LookPath(ocrCmd)
	return err == nil
}
