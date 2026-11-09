use std::path::Path;
use std::process::Command;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AvailablePdfTool {
    Pdftotext,
    Ghostscript,
    Mutool,
    None,
}

#[allow(dead_code)]
impl AvailablePdfTool {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Pdftotext => "pdftotext (poppler-utils)",
            Self::Ghostscript => "gs (Ghostscript)",
            Self::Mutool => "mutool (mupdf-tools)",
            Self::None => "None",
        }
    }
}

pub fn is_command_in_path(cmd: &str, test_arg: &str) -> bool {
    Command::new(cmd)
        .arg(test_arg)
        .output()
        .map(|o| o.status.success() || !o.stdout.is_empty() || !o.stderr.is_empty())
        .unwrap_or(false)
}

pub fn get_available_pdf_tools() -> Vec<&'static str> {
    let mut tools = Vec::new();
    if is_command_in_path("pdftotext", "-v") {
        tools.push("pdftotext (poppler-utils)");
    }
    if is_command_in_path("gs", "--version") {
        tools.push("gs (Ghostscript)");
    }
    if is_command_in_path("mutool", "-v") {
        tools.push("mutool (mupdf-tools)");
    }
    tools
}

pub fn is_pdf_extractor_available() -> bool {
    !get_available_pdf_tools().is_empty()
}

pub fn extract_pdf<P: AsRef<Path>>(
    path: P,
    ocr_enabled: bool,
    ocr_cmd: &str,
) -> Result<(String, bool), Box<dyn std::error::Error + Send + Sync>> {
    let p = path.as_ref();

    // 1. Try pdftotext CLI first (poppler-utils)
    if let Some(text) = try_pdftotext(p) {
        if text.trim().len() > 10 {
            return Ok((text, false));
        }
    }

    // 2. Try Ghostscript CLI (gs)
    if let Some(text) = try_ghostscript(p) {
        if text.trim().len() > 10 {
            return Ok((text, false));
        }
    }

    // 3. Try mutool CLI (mupdf-tools)
    if let Some(text) = try_mutool(p) {
        if text.trim().len() > 10 {
            return Ok((text, false));
        }
    }

    // 4. If digital text was empty or tools missing, attempt OCR (for scanned PDFs)
    if ocr_enabled {
        if let Ok(text) = super::ocr::extract_image_ocr(p, ocr_cmd) {
            if text.trim().len() > 10 {
                return Ok((text, true));
            }
        }
    }

    if !is_pdf_extractor_available() {
        Err("No PDF text extractor found (pdftotext or gs). Please install poppler-utils (sudo apt install poppler-utils) or ghostscript (sudo apt install ghostscript) to enable PDF text indexing.".into())
    } else {
        Err("Could not extract text from PDF (file may be empty, encrypted, or a scanned image requiring OCR)".into())
    }
}

pub fn try_pdftotext<P: AsRef<Path>>(path: P) -> Option<String> {
    let output = Command::new("pdftotext")
        .arg("-layout")
        .arg("-enc")
        .arg("UTF-8")
        .arg(path.as_ref())
        .arg("-")
        .output()
        .ok()?;

    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !text.is_empty() {
            return Some(text);
        }
    }
    None
}

pub fn try_ghostscript<P: AsRef<Path>>(path: P) -> Option<String> {
    let output = Command::new("gs")
        .arg("-sDEVICE=txtwrite")
        .arg("-sOutputFile=%stdout")
        .arg("-q")
        .arg("-dNOPAUSE")
        .arg("-dBATCH")
        .arg(path.as_ref())
        .output()
        .ok()?;

    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !text.is_empty() {
        return Some(text);
    }
    None
}

pub fn try_mutool<P: AsRef<Path>>(path: P) -> Option<String> {
    let output = Command::new("mutool")
        .arg("draw")
        .arg("-F")
        .arg("txt")
        .arg("-o")
        .arg("-")
        .arg(path.as_ref())
        .output()
        .ok()?;

    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !text.is_empty() {
            return Some(text);
        }
    }
    None
}
