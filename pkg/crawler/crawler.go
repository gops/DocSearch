package crawler

import (
	"crypto/sha256"
	"encoding/hex"
	"io"
	"log"
	"os"
	"path/filepath"
	"sync"
	"sync/atomic"
	"time"

	"docsearch/pkg/config"
	"docsearch/pkg/extractor"
	"docsearch/pkg/indexer"

	"github.com/fsnotify/fsnotify"
)

// Status holds crawler metrics.
type Status struct {
	IsIndexing    bool   `json:"is_indexing"`
	IndexedCount  int64  `json:"indexed_count"`
	TotalFiles    int64  `json:"total_files"`
	LastIndexTime string `json:"last_index_time"`
	LastError     string `json:"last_error"`
}

// Crawler scans configured directories and watches them for file system events.
type Crawler struct {
	cfg       *config.Config
	idx       *indexer.Indexer
	extractor *extractor.Manager
	watcher   *fsnotify.Watcher

	mu            sync.RWMutex
	stopChan      chan struct{}
	isIndexing    atomic.Bool
	indexedCount  atomic.Int64
	totalFiles    atomic.Int64
	lastIndexTime time.Time
	lastError     string
}

// New creates a new directory crawler and watcher.
func New(cfg *config.Config, idx *indexer.Indexer, ext *extractor.Manager) (*Crawler, error) {
	watcher, err := fsnotify.NewWatcher()
	if err != nil {
		return nil, err
	}

	return &Crawler{
		cfg:       cfg,
		idx:       idx,
		extractor: ext,
		watcher:   watcher,
		stopChan:  make(chan struct{}),
	}, nil
}

// Start begins background directory watching and triggers an initial scan.
func (c *Crawler) Start() {
	go c.watchLoop()
	go c.ScanAll()
}

// Stop terminates background watching.
func (c *Crawler) Stop() {
	close(c.stopChan)
	if c.watcher != nil {
		_ = c.watcher.Close()
	}
}

// GetStatus returns the current crawler state.
func (c *Crawler) GetStatus() Status {
	c.mu.RLock()
	defer c.mu.RUnlock()

	lastTime := ""
	if !c.lastIndexTime.IsZero() {
		lastTime = c.lastIndexTime.Format(time.RFC3339)
	}

	return Status{
		IsIndexing:    c.isIndexing.Load(),
		IndexedCount:  c.indexedCount.Load(),
		TotalFiles:    c.totalFiles.Load(),
		LastIndexTime: lastTime,
		LastError:     c.lastError,
	}
}

// ScanAll scans all directories explicitly added to IndexedFolders.
func (c *Crawler) ScanAll() {
	if c.isIndexing.Swap(true) {
		log.Println("[Crawler] Scan already in progress, skipping...")
		return
	}
	defer c.isIndexing.Store(false)

	folders := c.cfg.GetFolders()
	if len(folders) == 0 {
		log.Println("[Crawler] No folders configured to index yet. Add a folder via 'docsearch add <path>' or through the Web UI.")
		return
	}
	log.Printf("[Crawler] Starting scan on %d configured folders...\n", len(folders))

	var allFiles []string
	for _, folder := range folders {
		if _, err := os.Stat(folder); err != nil {
			log.Printf("[Crawler] Folder does not exist or inaccessible: %s\n", folder)
			continue
		}

		// Register with file watcher
		c.addWatchRecursive(folder)

		// Discover matching files
		_ = filepath.WalkDir(folder, func(path string, d os.DirEntry, err error) error {
			if err != nil {
				return nil
			}

			if d.IsDir() {
				if c.cfg.IsExcludedDir(d.Name()) {
					return filepath.SkipDir
				}
				c.addWatch(path)
				return nil
			}

			if c.cfg.IsAllowedExtension(filepath.Ext(path)) {
				allFiles = append(allFiles, path)
			}
			return nil
		})
	}

	c.totalFiles.Store(int64(len(allFiles)))
	c.indexedCount.Store(0)

	log.Printf("[Crawler] Discovered %d candidate files to index.\n", len(allFiles))

	// Worker pool for indexing
	const numWorkers = 4
	jobs := make(chan string, len(allFiles))
	var wg sync.WaitGroup

	for i := 0; i < numWorkers; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for path := range jobs {
				c.indexFile(path)
				c.indexedCount.Add(1)
			}
		}()
	}

	for _, file := range allFiles {
		jobs <- file
	}
	close(jobs)

	wg.Wait()

	c.mu.Lock()
	c.lastIndexTime = time.Now()
	c.mu.Unlock()

	docCount, _ := c.idx.DocCount()
	log.Printf("[Crawler] Scan completed. Current total docs in index: %d\n", docCount)
}

// indexFile performs incremental indexing on a single file.
func (c *Crawler) indexFile(filePath string) {
	info, err := os.Stat(filePath)
	if err != nil {
		return
	}

	// Calculate file hash
	hash, err := calculateFileHash(filePath)
	if err != nil {
		return
	}

	// Compare with stored hash
	storedHash, err := c.idx.GetDocumentHash(filePath)
	if err == nil && storedHash == hash && storedHash != "" {
		// File has not changed, skip re-extracting
		return
	}

	// Extract document text
	res, err := c.extractor.Extract(filePath)
	if err != nil {
		// Even if extraction failed or is partial, we don't crash
		return
	}

	if len(res.Content) == 0 {
		return
	}

	doc := &indexer.Document{
		ID:        filePath,
		Path:      filePath,
		Filename:  filepath.Base(filePath),
		Extension: filepath.Ext(filePath),
		Size:      info.Size(),
		ModTime:   info.ModTime(),
		Content:   res.Content,
		Hash:      hash,
		IsOCR:     res.IsOCR,
	}

	if err := c.idx.IndexDocument(doc); err != nil {
		log.Printf("[Crawler] Failed to index %s: %v\n", filePath, err)
	}
}

// watchLoop monitors file system events in real time.
func (c *Crawler) watchLoop() {
	// Debounce timer for file modification events
	debounce := make(map[string]*time.Timer)
	var debounceMu sync.Mutex

	for {
		select {
		case <-c.stopChan:
			return

		case event, ok := <-c.watcher.Events:
			if !ok {
				return
			}

			path := event.Name

			if event.Has(fsnotify.Create) {
				info, err := os.Stat(path)
				if err == nil && info.IsDir() {
					if !c.cfg.IsExcludedDir(info.Name()) {
						c.addWatchRecursive(path)
					}
				} else if c.cfg.IsAllowedExtension(filepath.Ext(path)) {
					c.indexFile(path)
				}
			}

			if event.Has(fsnotify.Write) {
				if c.cfg.IsAllowedExtension(filepath.Ext(path)) {
					debounceMu.Lock()
					if timer, exists := debounce[path]; exists {
						timer.Stop()
					}
					debounce[path] = time.AfterFunc(1*time.Second, func() {
						c.indexFile(path)
						debounceMu.Lock()
						delete(debounce, path)
						debounceMu.Unlock()
					})
					debounceMu.Unlock()
				}
			}

			if event.Has(fsnotify.Remove) || event.Has(fsnotify.Rename) {
				_ = c.idx.DeleteDocument(path)
			}

		case err, ok := <-c.watcher.Errors:
			if !ok {
				return
			}
			log.Printf("[Crawler] Watcher error: %v\n", err)
		}
	}
}

func (c *Crawler) addWatch(dir string) {
	if c.watcher != nil {
		_ = c.watcher.Add(dir)
	}
}

func (c *Crawler) addWatchRecursive(dir string) {
	_ = filepath.WalkDir(dir, func(path string, d os.DirEntry, err error) error {
		if err == nil && d.IsDir() {
			if c.cfg.IsExcludedDir(d.Name()) {
				return filepath.SkipDir
			}
			c.addWatch(path)
		}
		return nil
	})
}

func calculateFileHash(filePath string) (string, error) {
	f, err := os.Open(filePath)
	if err != nil {
		return "", err
	}
	defer f.Close()

	hasher := sha256.New()
	if _, err := io.Copy(hasher, f); err != nil {
		return "", err
	}

	return hex.EncodeToString(hasher.Sum(nil)), nil
}
