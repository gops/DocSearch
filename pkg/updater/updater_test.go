package updater

import (
	"archive/tar"
	"bytes"
	"compress/gzip"
	"testing"
)

func TestIsNewerVersion(t *testing.T) {
	tests := []struct {
		remote   string
		current  string
		expected bool
	}{
		{"v0.2.0", "v0.1.0", true},
		{"v1.0.0", "v0.9.9", true},
		{"0.1.1", "0.1.0", true},
		{"v0.1.0", "v0.1.0", false},
		{"v0.1.0", "v0.2.0", false},
		{"v0.1.0-alpha", "v0.1.0", false},
	}

	for _, tt := range tests {
		res := isNewerVersion(tt.remote, tt.current)
		if res != tt.expected {
			t.Errorf("isNewerVersion(%q, %q) = %v; want %v", tt.remote, tt.current, res, tt.expected)
		}
	}
}

func TestExtractBinaryFromTarGz(t *testing.T) {
	var buf bytes.Buffer
	gw := gzip.NewWriter(&buf)
	tw := tar.NewWriter(gw)

	content := []byte("#!/bin/sh\necho 'hello'")
	header := &tar.Header{
		Name: "docsearch",
		Size: int64(len(content)),
		Mode: 0755,
	}

	if err := tw.WriteHeader(header); err != nil {
		t.Fatalf("WriteHeader failed: %v", err)
	}
	if _, err := tw.Write(content); err != nil {
		t.Fatalf("Write failed: %v", err)
	}
	tw.Close()
	gw.Close()

	extracted, err := extractBinary(buf.Bytes(), "docsearch_linux_amd64.tar.gz")
	if err != nil {
		t.Fatalf("extractBinary failed: %v", err)
	}

	if !bytes.Equal(extracted, content) {
		t.Fatalf("Extracted content mismatch: got %s, want %s", string(extracted), string(content))
	}
}
