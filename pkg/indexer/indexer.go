package indexer

import (
	"context"
	"fmt"
	"os"
	"sort"
	"strconv"
	"sync"
	"time"

	"github.com/blugelabs/bluge"
	"github.com/blugelabs/bluge/search/highlight"
)

// Document represents a searchable file record.
type Document struct {
	ID        string
	Path      string
	Filename  string
	Extension string
	Size      int64
	ModTime   time.Time
	Content   string
	Hash      string
	IsOCR     bool
}

// SearchResultItem represents an individual match returned from a search query.
type SearchResultItem struct {
	ID         string    `json:"id"`
	Path       string    `json:"path"`
	Filename   string    `json:"filename"`
	Extension  string    `json:"extension"`
	Size       int64     `json:"size"`
	ModTime    time.Time `json:"mod_time"`
	Score      float64   `json:"score"`
	IsOCR      bool      `json:"is_ocr"`
	Highlights []string  `json:"highlights"`
}

// SearchResult holds search execution metrics and hit items.
type SearchResult struct {
	Query      string             `json:"query"`
	TotalHits  int                `json:"total_hits"`
	DurationMs int64              `json:"duration_ms"`
	Items      []SearchResultItem `json:"items"`
}

// DuplicateFile represents an individual file in a duplicate group.
type DuplicateFile struct {
	Path      string    `json:"path"`
	Filename  string    `json:"filename"`
	Extension string    `json:"extension"`
	Size      int64     `json:"size"`
	ModTime   time.Time `json:"mod_time"`
}

// DuplicateGroup represents a cluster of identical files sharing the exact same content hash.
type DuplicateGroup struct {
	Hash        string          `json:"hash"`
	Size        int64           `json:"size"`
	WastedBytes int64           `json:"wasted_bytes"`
	Count       int             `json:"count"`
	Files       []DuplicateFile `json:"files"`
}

// DuplicateReport summarizes all duplicate groups found in the index.
type DuplicateReport struct {
	TotalGroups      int              `json:"total_groups"`
	TotalDuplicates  int              `json:"total_duplicates"`
	TotalWastedBytes int64           `json:"total_wasted_bytes"`
	Groups           []DuplicateGroup `json:"groups"`
}

// Indexer manages the Bluge index writer, reader, and searches.
type Indexer struct {
	mu          sync.RWMutex
	indexPath   string
	writer      *bluge.Writer
	highlighter *highlight.SimpleHighlighter
}

// Open creates or loads a Bluge index at the specified directory.
func Open(indexPath string) (*Indexer, error) {
	if err := os.MkdirAll(indexPath, 0755); err != nil {
		return nil, fmt.Errorf("failed to create index directory: %w", err)
	}

	cfg := bluge.DefaultConfig(indexPath)
	writer, err := bluge.OpenWriter(cfg)
	if err != nil {
		return nil, fmt.Errorf("failed to open Bluge index writer: %w", err)
	}

	return &Indexer{
		indexPath:   indexPath,
		writer:      writer,
		highlighter: highlight.NewHTMLHighlighter(),
	}, nil
}

// Close safely shuts down the index writer.
func (idx *Indexer) Close() error {
	idx.mu.Lock()
	defer idx.mu.Unlock()

	if idx.writer != nil {
		err := idx.writer.Close()
		idx.writer = nil
		return err
	}
	return nil
}

// IndexDocument updates or inserts a document record into the search index.
func (idx *Indexer) IndexDocument(doc *Document) error {
	idx.mu.Lock()
	defer idx.mu.Unlock()

	if idx.writer == nil {
		return fmt.Errorf("indexer is closed")
	}

	bDoc := bluge.NewDocument(doc.Path).
		AddField(bluge.NewKeywordField("path", doc.Path).StoreValue()).
		AddField(bluge.NewTextField("filename", doc.Filename).StoreValue().HighlightMatches()).
		AddField(bluge.NewKeywordField("ext", doc.Extension).StoreValue()).
		AddField(bluge.NewNumericField("size", float64(doc.Size)).StoreValue()).
		AddField(bluge.NewKeywordField("size_bytes", strconv.FormatInt(doc.Size, 10)).StoreValue()).
		AddField(bluge.NewDateTimeField("mod_time", doc.ModTime).StoreValue()).
		AddField(bluge.NewKeywordField("hash", doc.Hash).StoreValue()).
		AddField(bluge.NewKeywordField("is_ocr", strconv.FormatBool(doc.IsOCR)).StoreValue()).
		AddField(bluge.NewTextField("content", doc.Content).StoreValue().HighlightMatches())

	return idx.writer.Update(bDoc.ID(), bDoc)
}

// DeleteDocument removes a file path from the index.
func (idx *Indexer) DeleteDocument(filePath string) error {
	idx.mu.Lock()
	defer idx.mu.Unlock()

	if idx.writer == nil {
		return fmt.Errorf("indexer is closed")
	}

	return idx.writer.Delete(bluge.Identifier(filePath))
}

// GetDocumentHash queries the stored hash for a specific file path to check if re-indexing is required.
func (idx *Indexer) GetDocumentHash(filePath string) (string, error) {
	idx.mu.RLock()
	defer idx.mu.RUnlock()

	if idx.writer == nil {
		return "", fmt.Errorf("indexer is closed")
	}

	reader, err := idx.writer.Reader()
	if err != nil {
		return "", err
	}
	defer reader.Close()

	termQuery := bluge.NewTermQuery(filePath).SetField("path")
	req := bluge.NewTopNSearch(1, termQuery)
	dmi, err := reader.Search(context.Background(), req)
	if err != nil {
		return "", err
	}

	hit, err := dmi.Next()
	if err != nil || hit == nil {
		return "", nil // Not found
	}

	var storedHash string
	_ = hit.VisitStoredFields(func(field string, value []byte) bool {
		if field == "hash" {
			storedHash = string(value)
		}
		return true
	})

	return storedHash, nil
}

// Search executes a full-text query across document filename and contents.
func (idx *Indexer) Search(queryStr string, limit int) (*SearchResult, error) {
	idx.mu.RLock()
	defer idx.mu.RUnlock()

	if idx.writer == nil {
		return nil, fmt.Errorf("indexer is closed")
	}

	if limit <= 0 {
		limit = 25
	}

	startTime := time.Now()

	reader, err := idx.writer.Reader()
	if err != nil {
		return nil, fmt.Errorf("failed to acquire reader: %w", err)
	}
	defer reader.Close()

	// Multi-field search across filename and content with prefix support
	fnQuery := bluge.NewMatchQuery(queryStr).SetField("filename").SetBoost(4.0)
	fnPrefixQuery := bluge.NewPrefixQuery(queryStr).SetField("filename").SetBoost(2.0)
	contentQuery := bluge.NewMatchQuery(queryStr).SetField("content")

	disjunctionQuery := bluge.NewBooleanQuery()
	disjunctionQuery.AddShould(fnQuery, fnPrefixQuery, contentQuery)
	req := bluge.NewTopNSearch(limit, disjunctionQuery).IncludeLocations()

	dmi, err := reader.Search(context.Background(), req)
	if err != nil {
		return nil, fmt.Errorf("search failed: %w", err)
	}

	items := make([]SearchResultItem, 0, limit)
	hit, err := dmi.Next()

	for err == nil && hit != nil {
		item := SearchResultItem{
			Score: hit.Score,
		}

		var rawContent string
		var rawModTimeStr string
		var rawSizeStr string
		var isOCRStr string

		_ = hit.VisitStoredFields(func(field string, value []byte) bool {
			val := string(value)
			switch field {
			case "_id":
				item.ID = val
			case "path":
				item.Path = val
			case "filename":
				item.Filename = val
			case "ext":
				item.Extension = val
			case "size":
				rawSizeStr = val
			case "size_bytes":
				rawSizeStr = val
			case "mod_time":
				rawModTimeStr = val
			case "is_ocr":
				isOCRStr = val
			case "content":
				rawContent = val
			}
			return true
		})

		// Parse size
		if rawSizeStr != "" {
			if s, err := strconv.ParseFloat(rawSizeStr, 64); err == nil {
				item.Size = int64(s)
			}
		}

		// Parse mod time
		if rawModTimeStr != "" {
			if t, err := time.Parse(time.RFC3339Nano, rawModTimeStr); err == nil {
				item.ModTime = t
			}
		}

		// Parse isOCR
		item.IsOCR = (isOCRStr == "true")

		// Extract snippets / highlights
		var highlights []string
		if contentLocs, ok := hit.Locations["content"]; ok && len(rawContent) > 0 {
			frags := idx.highlighter.BestFragments(contentLocs, []byte(rawContent), 2)
			for _, frag := range frags {
				highlights = append(highlights, frag)
			}
		}

		// If no content highlights, grab a short excerpt
		if len(highlights) == 0 && len(rawContent) > 0 {
			excerptLen := 150
			if len(rawContent) < excerptLen {
				excerptLen = len(rawContent)
			}
			highlights = append(highlights, rawContent[:excerptLen]+"...")
		}

		item.Highlights = highlights
		items = append(items, item)

		hit, err = dmi.Next()
	}

	duration := time.Since(startTime)

	return &SearchResult{
		Query:      queryStr,
		TotalHits:  len(items),
		DurationMs: duration.Milliseconds(),
		Items:      items,
	}, nil
}

// DocCount returns the total number of indexed documents.
func (idx *Indexer) DocCount() (uint64, error) {
	idx.mu.RLock()
	defer idx.mu.RUnlock()

	if idx.writer == nil {
		return 0, fmt.Errorf("indexer is closed")
	}

	reader, err := idx.writer.Reader()
	if err != nil {
		return 0, err
	}
	defer reader.Close()

	return reader.Count()
}

// FindDuplicates scans all indexed documents and groups identical files by content hash.
func (idx *Indexer) FindDuplicates() (*DuplicateReport, error) {
	idx.mu.RLock()
	defer idx.mu.RUnlock()

	if idx.writer == nil {
		return nil, fmt.Errorf("indexer is closed")
	}

	reader, err := idx.writer.Reader()
	if err != nil {
		return nil, fmt.Errorf("failed to acquire reader: %w", err)
	}
	defer reader.Close()

	// Query all indexed documents
	q := bluge.NewMatchAllQuery()
	req := bluge.NewAllMatches(q)

	dmi, err := reader.Search(context.Background(), req)
	if err != nil {
		return nil, fmt.Errorf("duplicate search failed: %w", err)
	}

	hashMap := make(map[string][]DuplicateFile)

	hit, err := dmi.Next()
	for err == nil && hit != nil {
		var path, filename, ext, hash, sizeStr, modTimeStr string

		_ = hit.VisitStoredFields(func(field string, value []byte) bool {
			val := string(value)
			switch field {
			case "path":
				path = val
			case "filename":
				filename = val
			case "ext":
				ext = val
			case "hash":
				hash = val
			case "size":
				sizeStr = val
			case "size_bytes":
				sizeStr = val
			case "mod_time":
				modTimeStr = val
			}
			return true
		})

		if hash != "" && path != "" {
			var size int64
			if s, err := strconv.ParseFloat(sizeStr, 64); err == nil {
				size = int64(s)
			}
			var modTime time.Time
			if t, err := time.Parse(time.RFC3339Nano, modTimeStr); err == nil {
				modTime = t
			}

			df := DuplicateFile{
				Path:      path,
				Filename:  filename,
				Extension: ext,
				Size:      size,
				ModTime:   modTime,
			}
			hashMap[hash] = append(hashMap[hash], df)
		}

		hit, err = dmi.Next()
	}

	var groups []DuplicateGroup
	var totalDuplicates int
	var totalWastedBytes int64

	for hash, files := range hashMap {
		if len(files) > 1 {
			fileSize := files[0].Size
			redundantCount := len(files) - 1
			wasted := int64(redundantCount) * fileSize

			totalDuplicates += redundantCount
			totalWastedBytes += wasted

			groups = append(groups, DuplicateGroup{
				Hash:        hash,
				Size:        fileSize,
				WastedBytes: wasted,
				Count:       len(files),
				Files:       files,
			})
		}
	}

	// Sort groups by wasted bytes descending (highest wasted disk space first)
	sort.Slice(groups, func(i, j int) bool {
		if groups[i].WastedBytes == groups[j].WastedBytes {
			return groups[i].Count > groups[j].Count
		}
		return groups[i].WastedBytes > groups[j].WastedBytes
	})

	return &DuplicateReport{
		TotalGroups:      len(groups),
		TotalDuplicates:  totalDuplicates,
		TotalWastedBytes: totalWastedBytes,
		Groups:           groups,
	}, nil
}
