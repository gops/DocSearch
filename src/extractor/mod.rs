pub mod docx;
pub mod ocr;
pub mod pdf;
pub mod text;

use std::path::Path;

#[derive(Debug, Clone)]
pub struct ExtractedDoc {
    pub content: String,
    pub is_ocr: bool,
}

pub struct ExtractorManager {
    pub ocr_enabled: bool,
    pub ocr_command: String,
}

impl ExtractorManager {
    pub fn new(ocr_enabled: bool, ocr_command: String) -> Self {
        Self {
            ocr_enabled,
            ocr_command,
        }
    }

    pub fn extract<P: AsRef<Path>>(&self, path: P) -> Result<ExtractedDoc, Box<dyn std::error::Error + Send + Sync>> {
        let p = path.as_ref();
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        match ext.as_str() {
            // Plain text and code
            "txt" | "md" | "markdown" | "log" | "csv" | "json" | "yaml" | "yml" | "html"
            | "css" | "go" | "rs" | "py" | "js" | "ts" | "sh" | "bash" | "zsh" | "conf"
            | "ini" | "env" | "toml" | "sql" => {
                let content = text::extract_plain_text(p)?;
                Ok(ExtractedDoc {
                    content,
                    is_ocr: false,
                })
            }

            // Word documents
            "docx" => {
                let content = docx::extract_docx(p)?;
                Ok(ExtractedDoc {
                    content,
                    is_ocr: false,
                })
            }

            // PDFs
            "pdf" => {
                let (content, is_ocr) = pdf::extract_pdf(p, self.ocr_enabled, &self.ocr_command)?;
                Ok(ExtractedDoc { content, is_ocr })
            }

            // Scanned images
            "png" | "jpg" | "jpeg" | "tiff" | "bmp" | "webp" => {
                if !self.ocr_enabled {
                    return Err("OCR is disabled".into());
                }
                let content = ocr::extract_image_ocr(p, &self.ocr_command)?;
                Ok(ExtractedDoc {
                    content,
                    is_ocr: true,
                })
            }

            // Fallback: try plain text
            _ => {
                let content = text::extract_plain_text(p)?;
                Ok(ExtractedDoc {
                    content,
                    is_ocr: false,
                })
            }
        }
    }
}
