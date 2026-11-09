use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use zip::ZipArchive;

pub fn extract_docx<P: AsRef<Path>>(path: P) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file)?;

    let mut doc_xml = archive.by_name("word/document.xml")?;
    let mut reader = Reader::from_reader(BufReader::new(&mut doc_xml));
    reader.config_mut().trim_text(true);

    let mut out_text = String::new();
    let mut in_text_node = false;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                if e.name().as_ref() == "w:t" {
                    in_text_node = true;
                }
            }
            Ok(Event::End(ref e)) => {
                if e.name().as_ref() == "w:t" {
                    in_text_node = false;
                } else if e.name().as_ref() == "w:p" {
                    out_text.push('\n');
                }
            }
            Ok(Event::Text(ref e)) => {
                if in_text_node {
                    let text = e.xml10_content();
                    out_text.push_str(&text);
                    out_text.push(' ');
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(Box::new(e)),
            _ => (),
        }
        buf.clear();
    }

    Ok(out_text.trim().to_string())
}
