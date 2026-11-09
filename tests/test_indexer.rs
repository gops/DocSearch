use docsearch::indexer::Indexer;
use tempfile::TempDir;

#[test]
fn test_indexer_crud_and_search() {
    let temp_dir = TempDir::new().unwrap();
    let idx = Indexer::open(temp_dir.path()).unwrap();

    idx.index_document(
        "/test/doc1.txt",
        "doc1.txt",
        ".txt",
        100,
        1700000000,
        "hash1",
        false,
        "The quick brown fox jumps over the lazy dog",
    )
    .unwrap();

    idx.index_document(
        "/test/doc2.txt",
        "doc2.txt",
        ".txt",
        200,
        1700000000,
        "hash2",
        false,
        "Rust programming language with Slint GUI",
    )
    .unwrap();

    idx.commit().unwrap();

    assert_eq!(idx.doc_count(), 2);

    let search_res = idx.search("fox", 10).unwrap();
    assert_eq!(search_res.total_hits, 1);
    assert_eq!(search_res.items[0].filename, "doc1.txt");

    let search_slint = idx.search("Slint", 10).unwrap();
    assert_eq!(search_slint.total_hits, 1);
    assert_eq!(search_slint.items[0].filename, "doc2.txt");

    // Search by path keyword
    let search_path = idx.search("path:test", 10).unwrap();
    assert_eq!(search_path.total_hits, 2);

    // Search by folder alias
    let search_folder = idx.search("folder:test", 10).unwrap();
    assert_eq!(search_folder.total_hits, 2);

    // Search by name alias
    let search_name = idx.search("name:doc1", 10).unwrap();
    assert_eq!(search_name.total_hits, 1);
    assert_eq!(search_name.items[0].filename, "doc1.txt");

    // Search by type / ext
    let search_type = idx.search("type:txt", 10).unwrap();
    assert_eq!(search_type.total_hits, 2);

    let search_ext = idx.search("ext:txt", 10).unwrap();
    assert_eq!(search_ext.total_hits, 2);

    // Search by size
    let search_size_gt = idx.search("size:>150B", 10).unwrap();
    assert_eq!(search_size_gt.total_hits, 1);
    assert_eq!(search_size_gt.items[0].filename, "doc2.txt");

    let search_size_lt = idx.search("size:<150B", 10).unwrap();
    assert_eq!(search_size_lt.total_hits, 1);
    assert_eq!(search_size_lt.items[0].filename, "doc1.txt");

    let search_size_range = idx.search("size:50B..150B", 10).unwrap();
    assert_eq!(search_size_range.total_hits, 1);
    assert_eq!(search_size_range.items[0].filename, "doc1.txt");

    // Combined search: path + type + size + content
    let search_combined = idx.search("path:test type:txt size:<150B fox", 10).unwrap();
    assert_eq!(search_combined.total_hits, 1);
    assert_eq!(search_combined.items[0].filename, "doc1.txt");

    // Test hash lookup
    let hash = idx.get_document_hash("/test/doc1.txt").unwrap();
    assert_eq!(hash, Some("hash1".into()));

    // Test delete
    idx.delete_document("/test/doc1.txt").unwrap();
    idx.commit().unwrap();
    assert_eq!(idx.doc_count(), 1);
}

#[test]
fn test_duplicate_detection() {
    let temp_dir = TempDir::new().unwrap();
    let idx = Indexer::open(temp_dir.path()).unwrap();

    // Two docs with identical hash
    idx.index_document(
        "/folder1/file.txt",
        "file.txt",
        ".txt",
        1024,
        1700000000,
        "same_hash_12345",
        false,
        "Content identical copy 1",
    )
    .unwrap();

    idx.index_document(
        "/folder2/copy.txt",
        "copy.txt",
        ".txt",
        1024,
        1700000000,
        "same_hash_12345",
        false,
        "Content identical copy 2",
    )
    .unwrap();

    // Single doc
    idx.index_document(
        "/folder3/unique.txt",
        "unique.txt",
        ".txt",
        512,
        1700000000,
        "unique_hash_999",
        false,
        "Unique content",
    )
    .unwrap();

    idx.commit().unwrap();

    let report = idx.find_duplicates().unwrap();
    assert_eq!(report.total_groups, 1);
    assert_eq!(report.total_duplicates, 1);
    assert_eq!(report.total_wasted_bytes, 1024);
    assert_eq!(report.groups[0].files.len(), 2);
}

#[test]
fn test_query_preprocessing() {
    use docsearch::indexer::{parse_size_to_bytes, preprocess_query};

    assert_eq!(parse_size_to_bytes("10MB"), Some(10 * 1024 * 1024));
    assert_eq!(parse_size_to_bytes("500KB"), Some(500 * 1024));
    assert_eq!(parse_size_to_bytes("1GB"), Some(1024 * 1024 * 1024));
    assert_eq!(parse_size_to_bytes("100B"), Some(100));

    assert_eq!(
        preprocess_query("type:pdf size:>1MB path:Dropbox"),
        "ext:pdf size:>1048576 path:Dropbox"
    );

    assert_eq!(
        preprocess_query("folder:Projects file:main.rs"),
        "path:Projects filename:main.rs"
    );

    assert_eq!(
        preprocess_query("type:image screenshot size:small"),
        "(ext:png OR ext:jpg OR ext:jpeg) screenshot size:[0 TO 102400]"
    );

    assert_eq!(
        preprocess_query("size:1MB..5MB report"),
        "size:[1048576 TO 5242880] report"
    );
}
