use docsearch::extractor::docx::extract_docx;
use docsearch::extractor::text::extract_plain_text;
use docsearch::extractor::ExtractorManager;
use std::path::Path;

#[test]
fn test_plain_text_extraction() {
    let path = Path::new("test_docs/sample1.txt");
    let text = extract_plain_text(path).expect("Failed to extract plain text");
    assert!(!text.is_empty());
}

#[test]
fn test_docx_extraction() {
    let path = Path::new("test_docs/sample3.docx");
    let text = extract_docx(path).expect("Failed to extract docx");
    assert!(!text.is_empty());
}

#[test]
fn test_manager_dispatch() {
    let mgr = ExtractorManager::new(false, "tesseract".into());
    let res = mgr.extract("test_docs/sample2.md").expect("Failed to extract md");
    assert!(!res.content.is_empty());
    assert!(!res.is_ocr);
}
