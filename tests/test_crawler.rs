use docsearch::config::Config;
use docsearch::crawler::{calculate_file_hash, Crawler};
use docsearch::extractor::ExtractorManager;
use docsearch::indexer::Indexer;
use std::fs;
use std::sync::{Arc, RwLock};
use tempfile::TempDir;

#[test]
fn test_crawler_indexing_and_throttling() {
    let temp_dir = TempDir::new().unwrap();
    let index_path = temp_dir.path().join("index");
    let docs_dir = temp_dir.path().join("docs");
    fs::create_dir_all(&docs_dir).unwrap();

    // Create a few test documents
    let file1 = docs_dir.join("test1.txt");
    fs::write(&file1, "The quick brown fox jumps over the lazy dog.").unwrap();

    let file2 = docs_dir.join("test2.md");
    fs::write(&file2, "# Rust Search Engine\nSuper low CPU and I/O consumption.").unwrap();

    // Large-ish file to test chunking and hash yielding
    let file3 = docs_dir.join("test3.txt");
    let large_content = "A".repeat(1024 * 1024 + 100); // > 1MB
    fs::write(&file3, large_content).unwrap();

    // Verify calculate_file_hash works correctly even on >1MB files with throttling
    let hash1 = calculate_file_hash(&file1).unwrap();
    assert!(!hash1.is_empty());
    let hash3 = calculate_file_hash(&file3).unwrap();
    assert!(!hash3.is_empty());

    let mut cfg = Config::default();
    cfg.index_path = index_path.to_string_lossy().to_string();
    cfg.indexed_folders = vec![docs_dir.to_string_lossy().to_string()];
    cfg.indexing_delay_ms = 5; // Fast enough for tests, but exercises pacing sleep

    let config_arc = Arc::new(RwLock::new(cfg));
    let indexer = Arc::new(Indexer::open(&index_path).unwrap());
    let extractor = Arc::new(ExtractorManager::new(false, "tesseract".to_string()));

    let crawler = Crawler::new(config_arc, indexer.clone(), extractor);

    // Initial scan
    crawler.scan_all();

    // Documents should be indexed
    assert_eq!(indexer.doc_count(), 3);

    let search_res = indexer.search("jumps", 10).unwrap();
    assert_eq!(search_res.total_hits, 1);
    assert_eq!(search_res.items[0].filename, "test1.txt");

    let search_rust = indexer.search("consumption", 10).unwrap();
    assert_eq!(search_rust.total_hits, 1);
    assert_eq!(search_rust.items[0].filename, "test2.md");

    // Search by path filter
    let search_path = indexer.search("path:docs", 10).unwrap();
    assert_eq!(search_path.total_hits, 3);

    // Search by type filter
    let search_md = indexer.search("type:markdown", 10).unwrap();
    assert_eq!(search_md.total_hits, 1);
    assert_eq!(search_md.items[0].filename, "test2.md");

    // Search by size filter (>1MB)
    let search_large = indexer.search("size:>1MB", 10).unwrap();
    assert_eq!(search_large.total_hits, 1);
    assert_eq!(search_large.items[0].filename, "test3.txt");

    // Search by size filter (<1KB)
    let search_small = indexer.search("size:<1KB", 10).unwrap();
    assert_eq!(search_small.total_hits, 2);

    // Search by filename filter
    let search_name = indexer.search("name:test2", 10).unwrap();
    assert_eq!(search_name.total_hits, 1);
    assert_eq!(search_name.items[0].filename, "test2.md");

    // Second scan (all files unchanged)
    crawler.scan_all();
    assert_eq!(indexer.doc_count(), 3);
}
