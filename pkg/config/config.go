package config

import (
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"sync"
)

// Config represents the application settings.
type Config struct {
	mu             sync.RWMutex
	configFilePath string

	IndexedFolders []string `json:"indexed_folders"`
	IndexPath      string   `json:"index_path"`
	ServerPort     int      `json:"server_port"`
	FileExtensions []string `json:"file_extensions"`
	ExcludedDirs   []string `json:"excluded_dirs"`
	OCREnabled     bool     `json:"ocr_enabled"`
	OCRCommand     string   `json:"ocr_command"`
	GitHubRepo     string   `json:"github_repo"`
}

// DefaultConfigPath returns the canonical path for configuration file.
func DefaultConfigPath() string {
	if env := os.Getenv("DOCSEARCH_CONFIG"); env != "" {
		return env
	}

	// Portable / local check
	if _, err := os.Stat("data/config.json"); err == nil {
		return "data/config.json"
	}

	homeDir, err := os.UserHomeDir()
	if err != nil {
		return "data/config.json"
	}

	configDir := filepath.Join(homeDir, ".config", "docsearch")
	if err := os.MkdirAll(configDir, 0755); err != nil {
		// Fallback to local data dir if home directory is read-only
		return "data/config.json"
	}

	return filepath.Join(configDir, "config.json")
}

// DefaultConfig returns safe default settings.
func DefaultConfig() *Config {
	homeDir, _ := os.UserHomeDir()
	dataDir := "data"

	if homeDir != "" {
		sysDataDir := filepath.Join(homeDir, ".local", "share", "docsearch")
		if err := os.MkdirAll(sysDataDir, 0755); err == nil {
			dataDir = sysDataDir
		}
	}

	return &Config{
		IndexedFolders: []string{},
		IndexPath:      filepath.Join(dataDir, "bluge_index"),
		ServerPort:     9876,
		FileExtensions: []string{
			".txt", ".md", ".markdown", ".pdf", ".docx",
			".csv", ".log", ".json", ".yaml", ".yml",
			".png", ".jpg", ".jpeg", ".go", ".py",
			".js", ".ts", ".html", ".css", ".sh",
		},
		ExcludedDirs: []string{
			".git", "node_modules", ".cache", "vendor",
			"target", "build", "dist", "tmp", ".svn",
		},
		OCREnabled: true,
		OCRCommand: "tesseract",
		GitHubRepo: "gops/DocSearch",
	}
}

// Load loads configuration from the given path or default path.
func Load(customPath string) (*Config, error) {
	targetPath := customPath
	if targetPath == "" {
		targetPath = DefaultConfigPath()
	}

	cfg := DefaultConfig()
	cfg.configFilePath = targetPath

	data, err := os.ReadFile(targetPath)
	if err != nil {
		if os.IsNotExist(err) {
			// Save default configuration
			if err := cfg.Save(); err != nil {
				return nil, err
			}
			return cfg, nil
		}
		return nil, err
	}

	if err := json.Unmarshal(data, cfg); err != nil {
		return nil, err
	}

	cfg.configFilePath = targetPath
	return cfg, nil
}

// Save writes current configuration to disk atomically.
func (c *Config) Save() error {
	c.mu.RLock()
	defer c.mu.RUnlock()

	if c.configFilePath == "" {
		c.configFilePath = DefaultConfigPath()
	}

	dir := filepath.Dir(c.configFilePath)
	if err := os.MkdirAll(dir, 0755); err != nil {
		return err
	}

	data, err := json.MarshalIndent(c, "", "  ")
	if err != nil {
		return err
	}

	tmpFile := c.configFilePath + ".tmp"
	if err := os.WriteFile(tmpFile, data, 0644); err != nil {
		return err
	}

	return os.Rename(tmpFile, c.configFilePath)
}

// AddFolder adds a directory to the indexed folders list if it exists and isn't already added.
func (c *Config) AddFolder(folderPath string) (bool, error) {
	c.mu.Lock()
	defer c.mu.Unlock()

	absPath, err := filepath.Abs(folderPath)
	if err != nil {
		return false, err
	}

	info, err := os.Stat(absPath)
	if err != nil {
		return false, err
	}
	if !info.IsDir() {
		return false, errors.New("specified path is not a directory")
	}

	for _, existing := range c.IndexedFolders {
		if filepath.Clean(existing) == filepath.Clean(absPath) {
			return false, nil // already present
		}
	}

	c.IndexedFolders = append(c.IndexedFolders, absPath)
	return true, nil
}

// RemoveFolder removes a directory from the indexed folders list.
func (c *Config) RemoveFolder(folderPath string) bool {
	c.mu.Lock()
	defer c.mu.Unlock()

	absPath, err := filepath.Abs(folderPath)
	cleanTarget := filepath.Clean(absPath)
	if err != nil {
		cleanTarget = filepath.Clean(folderPath)
	}

	newList := make([]string, 0, len(c.IndexedFolders))
	found := false
	for _, existing := range c.IndexedFolders {
		if filepath.Clean(existing) == cleanTarget {
			found = true
			continue
		}
		newList = append(newList, existing)
	}

	if found {
		c.IndexedFolders = newList
	}
	return found
}

// GetFolders returns a copy of indexed folders.
func (c *Config) GetFolders() []string {
	c.mu.RLock()
	defer c.mu.RUnlock()

	copied := make([]string, len(c.IndexedFolders))
	copy(copied, c.IndexedFolders)
	return copied
}

// IsAllowedExtension checks if file extension is supported.
func (c *Config) IsAllowedExtension(ext string) bool {
	c.mu.RLock()
	defer c.mu.RUnlock()

	lower := strings.ToLower(ext)
	for _, item := range c.FileExtensions {
		if strings.ToLower(item) == lower {
			return true
		}
	}
	return false
}

// IsExcludedDir checks if a directory name matches any excluded folder names.
func (c *Config) IsExcludedDir(dirName string) bool {
	c.mu.RLock()
	defer c.mu.RUnlock()

	for _, excluded := range c.ExcludedDirs {
		if strings.EqualFold(dirName, excluded) {
			return true
		}
	}
	return false
}
