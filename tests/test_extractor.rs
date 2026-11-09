use docsearch::extractor::docx::extract_docx;
use docsearch::extractor::pdf::{
    get_available_pdf_tools, is_pdf_extractor_available, try_ghostscript, try_pdftotext,
};
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
fn test_pdf_extraction() {
    assert!(is_pdf_extractor_available());
    let tools = get_available_pdf_tools();
    assert!(!tools.is_empty());

    let path = Path::new("test_docs/sample4.pdf");

    // Test pdftotext extraction
    if let Some(text) = try_pdftotext(path) {
        assert!(text.contains("DocSearch PDF Extraction"));
    }

    // Test ghostscript extraction (gs)
    if let Some(text) = try_ghostscript(path) {
        assert!(text.contains("DocSearch PDF Extraction"));
    }

    let mgr = ExtractorManager::new(false, "tesseract".into());
    let res = mgr.extract(path).expect("Failed to extract pdf");
    assert!(res.content.contains("DocSearch PDF Extraction"));
    assert!(!res.is_ocr);
}

#[test]
fn test_manager_dispatch() {
    let mgr = ExtractorManager::new(false, "tesseract".into());
    let res = mgr.extract("test_docs/sample2.md").expect("Failed to extract md");
    assert!(!res.content.is_empty());
    assert!(!res.is_ocr);
}
