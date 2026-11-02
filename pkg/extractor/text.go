package extractor

import (
	"bytes"
	"errors"
	"io"
	"os"
	"unicode/utf8"
)

const maxTextFileSize = 10 * 1024 * 1024 // 10MB limit per document

// ExtractPlainText reads text files safely, verifying UTF-8 validity and skipping binaries.
func ExtractPlainText(filePath string) (*Result, error) {
	stat, err := os.Stat(filePath)
	if err != nil {
		return nil, err
	}

	if stat.Size() > maxTextFileSize {
		// Read first 10MB
		f, err := os.Open(filePath)
		if err != nil {
			return nil, err
		}
		defer f.Close()

		buf := make([]byte, maxTextFileSize)
		n, err := io.ReadFull(f, buf)
		if err != nil && err != io.ErrUnexpectedEOF && err != io.EOF {
			return nil, err
		}
		data := buf[:n]
		if isBinary(data) {
			return nil, errors.New("file appears to be binary, skipped")
		}
		return &Result{
			Content: sanitizeText(string(data)),
			IsOCR:   false,
		}, nil
	}

	data, err := os.ReadFile(filePath)
	if err != nil {
		return nil, err
	}

	if isBinary(data) {
		return nil, errors.New("file appears to be binary, skipped")
	}

	return &Result{
		Content: sanitizeText(string(data)),
		IsOCR:   false,
	}, nil
}

// isBinary inspects the initial byte slice for null bytes and non-printable sequences.
func isBinary(data []byte) bool {
	sampleSize := 1024
	if len(data) < sampleSize {
		sampleSize = len(data)
	}

	if sampleSize == 0 {
		return false
	}

	sample := data[:sampleSize]
	if bytes.IndexByte(sample, 0) != -1 {
		return true // Null byte found
	}

	if !utf8.Valid(sample) {
		return true
	}

	return false
}

// sanitizeText removes null bytes and unprintable runes.
func sanitizeText(s string) string {
	return stringsReplaceNull(s)
}

func stringsReplaceNull(s string) string {
	var b bytes.Buffer
	b.Grow(len(s))
	for _, r := range s {
		if r != 0 && r != 0xFFFD {
			b.WriteRune(r)
		}
	}
	return b.String()
}
