use std::path::Path;
use std::process::Command;

pub fn extract_pdf<P: AsRef<Path>>(
    path: P,
    ocr_enabled: bool,
    ocr_cmd: &str,
) -> Result<(String, bool), Box<dyn std::error::Error + Send + Sync>> {
    // 1. Try pdftotext CLI first
    if let Ok(output) = Command::new("pdftotext")
        .arg("-layout")
        .arg("-enc")
        .arg("UTF-8")
        .arg(path.as_ref())
        .arg("-")
        .output()
    {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if text.len() > 20 {
                return Ok((text, false));
            }
        }
    }

    // 2. If digital text was empty or pdftotext missing, attempt OCR
    if ocr_enabled {
        if let Ok(text) = super::ocr::extract_image_ocr(path.as_ref(), ocr_cmd) {
            return Ok((text, true));
        }
    }

    Err("Could not extract text from PDF (pdftotext empty or missing, OCR unavailable)".into())
}
