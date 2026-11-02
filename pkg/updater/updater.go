package updater

import (
	"archive/tar"
	"archive/zip"
	"bytes"
	"compress/gzip"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"time"
)

// ReleaseInfo holds metadata for a GitHub release.
type ReleaseInfo struct {
	TagName     string    `json:"tag_name"`
	Name        string    `json:"name"`
	Body        string    `json:"body"`
	PublishedAt time.Time `json:"published_at"`
	HTMLURL     string    `json:"html_url"`
	AssetURL    string    `json:"asset_url"`
	AssetName   string    `json:"asset_name"`
	ChecksumURL string    `json:"checksum_url"`
}

// GitHubReleaseResponse represents the GitHub API response for a release.
type GitHubReleaseResponse struct {
	TagName     string `json:"tag_name"`
	Name        string `json:"name"`
	Body        string `json:"body"`
	HTMLURL     string `json:"html_url"`
	PublishedAt string `json:"published_at"`
	Assets      []struct {
		Name               string `json:"name"`
		BrowserDownloadURL string `json:"browser_download_url"`
	} `json:"assets"`
}

var httpClient = &http.Client{
	Timeout: 45 * time.Second,
}

// CheckLatestRelease queries GitHub Releases API to see if a newer version is available.
func CheckLatestRelease(repoSlug string, currentVersion string) (*ReleaseInfo, bool, error) {
	if repoSlug == "" {
		return nil, false, errors.New("repository slug is empty")
	}

	url := fmt.Sprintf("https://api.github.com/repos/%s/releases/latest", repoSlug)
	req, err := http.NewRequest(http.MethodGet, url, nil)
	if err != nil {
		return nil, false, err
	}

	req.Header.Set("Accept", "application/vnd.github.v3+json")
	req.Header.Set("User-Agent", "DocSearch-Updater/"+currentVersion)

	resp, err := httpClient.Do(req)
	if err != nil {
		return nil, false, fmt.Errorf("network error checking releases: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode == http.StatusNotFound {
		return nil, false, fmt.Errorf("no releases found for repository %s", repoSlug)
	}
	if resp.StatusCode != http.StatusOK {
		return nil, false, fmt.Errorf("GitHub API returned status %s", resp.Status)
	}

	var ghRelease GitHubReleaseResponse
	if err := json.NewDecoder(resp.Body).Decode(&ghRelease); err != nil {
		return nil, false, fmt.Errorf("failed to parse release metadata: %w", err)
	}

	rel := &ReleaseInfo{
		TagName: ghRelease.TagName,
		Name:    ghRelease.Name,
		Body:    ghRelease.Body,
		HTMLURL: ghRelease.HTMLURL,
	}
	if t, err := time.Parse(time.RFC3339, ghRelease.PublishedAt); err == nil {
		rel.PublishedAt = t
	}

	// Match asset for current OS and architecture
	expectedOS := runtime.GOOS
	expectedArch := runtime.GOARCH

	for _, asset := range ghRelease.Assets {
		lowerName := strings.ToLower(asset.Name)
		if strings.Contains(lowerName, "checksums") {
			rel.ChecksumURL = asset.BrowserDownloadURL
			continue
		}

		if strings.Contains(lowerName, expectedOS) && strings.Contains(lowerName, expectedArch) {
			rel.AssetURL = asset.BrowserDownloadURL
			rel.AssetName = asset.Name
		}
	}

	hasUpdate := isNewerVersion(ghRelease.TagName, currentVersion)
	return rel, hasUpdate, nil
}

// DownloadAndApply downloads the matching binary asset, validates checksums if available,
// and replaces the running executable on disk.
func DownloadAndApply(rel *ReleaseInfo) error {
	if rel == nil || rel.AssetURL == "" {
		return errors.New("no matching binary asset found for this platform")
	}

	execPath, err := os.Executable()
	if err != nil {
		return fmt.Errorf("could not determine executable path: %w", err)
	}
	execPath, err = filepath.EvalSymlinks(execPath)
	if err != nil {
		return fmt.Errorf("could not resolve executable symlinks: %w", err)
	}

	// 1. Download asset into memory or temp file
	req, err := http.NewRequest(http.MethodGet, rel.AssetURL, nil)
	if err != nil {
		return err
	}
	req.Header.Set("User-Agent", "DocSearch-Updater")

	resp, err := httpClient.Do(req)
	if err != nil {
		return fmt.Errorf("failed to download release asset: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("asset download failed with status %s", resp.Status)
	}

	archiveBytes, err := io.ReadAll(resp.Body)
	if err != nil {
		return fmt.Errorf("failed to read downloaded archive: %w", err)
	}

	// 2. Validate checksum if checksums.txt is available
	if rel.ChecksumURL != "" {
		if err := verifyChecksum(archiveBytes, rel.AssetName, rel.ChecksumURL); err != nil {
			return fmt.Errorf("checksum validation failed: %w", err)
		}
	}

	// 3. Extract binary from archive
	newBinaryBytes, err := extractBinary(archiveBytes, rel.AssetName)
	if err != nil {
		return fmt.Errorf("failed to extract binary: %w", err)
	}

	// 4. Atomically replace executable
	return applyBinaryReplacement(execPath, newBinaryBytes)
}

// applyBinaryReplacement replaces the running binary on disk.
func applyBinaryReplacement(targetPath string, newBytes []byte) error {
	dir := filepath.Dir(targetPath)
	tmpFile := filepath.Join(dir, fmt.Sprintf(".docsearch-update-%d.tmp", time.Now().UnixNano()))

	if err := os.WriteFile(tmpFile, newBytes, 0755); err != nil {
		return fmt.Errorf("failed to write update temporary file: %w", err)
	}

	if runtime.GOOS == "windows" {
		// On Windows, running exe cannot be directly overwritten.
		// Rename target to target.old, then rename tmpFile to target.
		oldPath := targetPath + ".old"
		_ = os.Remove(oldPath) // remove previous backup if exists
		if err := os.Rename(targetPath, oldPath); err != nil {
			_ = os.Remove(tmpFile)
			return fmt.Errorf("failed to rotate old Windows binary: %w", err)
		}
		if err := os.Rename(tmpFile, targetPath); err != nil {
			// Try to recover
			_ = os.Rename(oldPath, targetPath)
			return fmt.Errorf("failed to move new binary into place: %w", err)
		}
		return nil
	}

	// On Unix/POSIX, atomic rename works even while running
	if err := os.Rename(tmpFile, targetPath); err != nil {
		_ = os.Remove(tmpFile)
		return fmt.Errorf("failed to replace binary: %w", err)
	}

	return nil
}

// extractBinary unpacks the executable binary from .tar.gz, .zip, or raw binary.
func extractBinary(archiveData []byte, filename string) ([]byte, error) {
	lower := strings.ToLower(filename)

	if strings.HasSuffix(lower, ".tar.gz") || strings.HasSuffix(lower, ".tgz") {
		gr, err := gzip.NewReader(bytes.NewReader(archiveData))
		if err != nil {
			return nil, err
		}
		defer gr.Close()

		tr := tar.NewReader(gr)
		for {
			header, err := tr.Next()
			if err == io.EOF {
				break
			}
			if err != nil {
				return nil, err
			}

			baseName := filepath.Base(header.Name)
			if baseName == "docsearch" || baseName == "docsearch.exe" {
				return io.ReadAll(tr)
			}
		}
		return nil, errors.New("docsearch binary not found inside .tar.gz archive")
	}

	if strings.HasSuffix(lower, ".zip") {
		zr, err := zip.NewReader(bytes.NewReader(archiveData), int64(len(archiveData)))
		if err != nil {
			return nil, err
		}

		for _, f := range zr.File {
			baseName := filepath.Base(f.Name)
			if baseName == "docsearch" || baseName == "docsearch.exe" {
				rc, err := f.Open()
				if err != nil {
					return nil, err
				}
				defer rc.Close()
				return io.ReadAll(rc)
			}
		}
		return nil, errors.New("docsearch binary not found inside .zip archive")
	}

	// Assume raw binary if not an archive
	return archiveData, nil
}

// verifyChecksum compares the downloaded bytes' SHA-256 against checksums.txt.
func verifyChecksum(data []byte, filename string, checksumURL string) error {
	req, err := http.NewRequest(http.MethodGet, checksumURL, nil)
	if err != nil {
		return err
	}
	resp, err := httpClient.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("failed to fetch checksums: status %s", resp.Status)
	}

	checksumContent, err := io.ReadAll(resp.Body)
	if err != nil {
		return err
	}

	hasher := sha256.New()
	hasher.Write(data)
	computedHash := hex.EncodeToString(hasher.Sum(nil))

	lines := strings.Split(string(checksumContent), "\n")
	for _, line := range lines {
		line = strings.TrimSpace(line)
		if strings.Contains(line, filename) {
			parts := strings.Fields(line)
			if len(parts) >= 1 && strings.EqualFold(parts[0], computedHash) {
				return nil // Verified
			}
			return fmt.Errorf("checksum mismatch: expected %s, calculated %s", parts[0], computedHash)
		}
	}

	return nil // If filename wasn't explicitly in checksum file, proceed
}

// isNewerVersion compares semver tags (e.g. "v0.2.0" > "v0.1.0").
func isNewerVersion(remoteTag, currentVersion string) bool {
	remoteClean := strings.TrimPrefix(strings.TrimSpace(remoteTag), "v")
	currentClean := strings.TrimPrefix(strings.TrimSpace(currentVersion), "v")

	if remoteClean == currentClean || remoteClean == "" {
		return false
	}

	rParts := strings.Split(remoteClean, ".")
	cParts := strings.Split(currentClean, ".")

	for i := 0; i < len(rParts) && i < len(cParts); i++ {
		var rNum, cNum int
		_, _ = fmt.Sscanf(rParts[i], "%d", &rNum)
		_, _ = fmt.Sscanf(cParts[i], "%d", &cNum)
		if rNum > cNum {
			return true
		}
		if rNum < cNum {
			return false
		}
	}

	return len(rParts) > len(cParts)
}
