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
