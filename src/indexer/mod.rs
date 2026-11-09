pub mod duplicates;

use duplicates::{DuplicateFile, DuplicateGroup, DuplicateReport};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tantivy::collector::TopDocs;
use tantivy::query::{AllQuery, QueryParser, TermQuery};
use tantivy::schema::*;
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultItem {
    pub id: String,
    pub path: String,
    pub filename: String,
    pub extension: String,
    pub size: i64,
    pub mod_time: i64,
    pub is_ocr: bool,
    pub score: f32,
    pub highlights: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub query: String,
    pub total_hits: usize,
    pub duration_ms: u128,
    pub items: Vec<SearchResultItem>,
}

pub struct Indexer {
    #[allow(dead_code)]
    schema: Schema,
    raw_path_field: Field,
    path_field: Field,
    filename_field: Field,
    ext_field: Field,
    size_field: Field,
    mod_time_field: Field,
    hash_field: Field,
    is_ocr_field: Field,
    content_field: Field,

    index: Index,
    reader: IndexReader,
    writer: Arc<Mutex<IndexWriter>>,
}

impl Indexer {
    pub fn open<P: AsRef<Path>>(index_dir: P) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        fs::create_dir_all(index_dir.as_ref())?;

        let mut schema_builder = Schema::builder();
        // raw_path is untokenized STRING for unique ID, exact deletion, and hash lookup
        let raw_path_field = schema_builder.add_text_field("raw_path", STRING | STORED);
        // path is tokenized TEXT so folder names and path keywords can be searched
        let path_field = schema_builder.add_text_field("path", TEXT | STORED);
        // filename is tokenized TEXT
        let filename_field = schema_builder.add_text_field("filename", TEXT | STORED);
        // ext is tokenized TEXT so both 'pdf' and '.pdf' match
        let ext_field = schema_builder.add_text_field("ext", TEXT | STORED);
        // size is indexed i64 for range queries
        let size_field = schema_builder.add_i64_field("size", STORED | INDEXED | FAST);
        // mod_time is indexed i64 for date range queries
        let mod_time_field = schema_builder.add_i64_field("mod_time", STORED | INDEXED | FAST);
        let hash_field = schema_builder.add_text_field("hash", STRING | STORED);
        let is_ocr_field = schema_builder.add_text_field("is_ocr", STRING | STORED);
        let content_field = schema_builder.add_text_field("content", TEXT | STORED);
        let schema = schema_builder.build();

        let index = match Index::open_or_create(
            tantivy::directory::MmapDirectory::open(index_dir.as_ref())?,
            schema.clone(),
        ) {
            Ok(idx) => idx,
            Err(e) => {
                eprintln!("[Indexer] Index schema updated or incompatible ({}). Re-creating index directory...", e);
                let _ = fs::remove_dir_all(index_dir.as_ref());
                fs::create_dir_all(index_dir.as_ref())?;
                Index::create_in_dir(index_dir.as_ref(), schema.clone())?
            }
        };

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;

        // Allocate 50MB for writer buffer
        let writer = Arc::new(Mutex::new(index.writer(50_000_000)?));

        Ok(Self {
            schema,
            raw_path_field,
            path_field,
            filename_field,
            ext_field,
            size_field,
            mod_time_field,
            hash_field,
            is_ocr_field,
            content_field,
            index,
            reader,
            writer,
        })
    }

    pub fn index_document(
        &self,
        path: &str,
        filename: &str,
        ext: &str,
        size: i64,
        mod_time: i64,
        hash: &str,
        is_ocr: bool,
        content: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut doc = TantivyDocument::default();
        doc.add_text(self.raw_path_field, path);
        doc.add_text(self.path_field, path);
        doc.add_text(self.filename_field, filename);
        let ext_clean = ext.trim_start_matches('.');
        let ext_combined = format!("{} {}", ext, ext_clean);
        doc.add_text(self.ext_field, &ext_combined);
        doc.add_i64(self.size_field, size);
        doc.add_i64(self.mod_time_field, mod_time);
        doc.add_text(self.hash_field, hash);
        doc.add_text(self.is_ocr_field, if is_ocr { "true" } else { "false" });
        doc.add_text(self.content_field, content);

        let writer = self.writer.lock().map_err(|e| e.to_string())?;
        let term = Term::from_field_text(self.raw_path_field, path);
        writer.delete_term(term);
        writer.add_document(doc)?;
        Ok(())
    }

    pub fn commit(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut writer = self.writer.lock().map_err(|e| e.to_string())?;
        writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    pub fn delete_document(&self, path: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let writer = self.writer.lock().map_err(|e| e.to_string())?;
        let term = Term::from_field_text(self.raw_path_field, path);
        writer.delete_term(term);
        Ok(())
    }

    pub fn get_document_hash(&self, path: &str) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
        let searcher = self.reader.searcher();
        let term = Term::from_field_text(self.raw_path_field, path);
        let query = TermQuery::new(term, IndexRecordOption::Basic);
        let top_docs = searcher.search(&query, &TopDocs::with_limit(1).order_by_score())?;

        if let Some((_, doc_address)) = top_docs.first() {
            let doc: TantivyDocument = searcher.doc(*doc_address)?;
            if let Some(val) = doc.get_first(self.hash_field).and_then(|v| v.as_str()) {
                return Ok(Some(val.to_string()));
            }
        }
        Ok(None)
    }

    pub fn doc_count(&self) -> usize {
        let searcher = self.reader.searcher();
        searcher.num_docs() as usize
    }

    pub fn search(&self, query_str: &str, limit: usize) -> Result<SearchResult, Box<dyn std::error::Error + Send + Sync>> {
        let start = Instant::now();
        let searcher = self.reader.searcher();

        let processed_query = preprocess_query(query_str);

        let mut query_parser = QueryParser::for_index(
            &self.index,
            vec![
                self.filename_field,
                self.path_field,
                self.ext_field,
                self.content_field,
            ],
        );
        query_parser.set_conjunction_by_default();
        query_parser.set_field_boost(self.filename_field, 5.0);
        query_parser.set_field_boost(self.path_field, 2.5);
        query_parser.set_field_boost(self.ext_field, 2.0);
        query_parser.set_field_boost(self.content_field, 1.0);

        let query = match query_parser.parse_query(&processed_query) {
            Ok(q) => q,
            Err(_) => {
                let (lenient_q, _) = query_parser.parse_query_lenient(&processed_query);
                lenient_q
            }
        };

        let snippet_generator = tantivy::snippet::SnippetGenerator::create(&searcher, &*query, self.content_field).ok();

        let top_docs = searcher.search(&*query, &TopDocs::with_limit(limit).order_by_score())?;
        let mut items = Vec::new();

        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address)?;

            let path = doc
                .get_first(self.raw_path_field)
                .or_else(|| doc.get_first(self.path_field))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let filename = doc.get_first(self.filename_field).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let extension = doc
                .get_first(self.ext_field)
                .and_then(|v| v.as_str())
                .map(|s| s.split_whitespace().next().unwrap_or(s))
                .unwrap_or("")
                .to_string();
            let size = doc.get_first(self.size_field).and_then(|v| v.as_i64()).unwrap_or(0);
            let mod_time = doc.get_first(self.mod_time_field).and_then(|v| v.as_i64()).unwrap_or(0);
            let is_ocr = doc.get_first(self.is_ocr_field).and_then(|v| v.as_str()).unwrap_or("false") == "true";

            let mut highlights = Vec::new();
            if let Some(ref generator) = snippet_generator {
                let snippet = generator.snippet_from_doc(&doc);
                let html_frag = snippet.to_html();
                if !html_frag.trim().is_empty() {
                    highlights.push(html_frag);
                }
            }

            if highlights.is_empty() {
                if let Some(content) = doc.get_first(self.content_field).and_then(|v| v.as_str()) {
                    let trimmed = content.trim();
                    if !trimmed.is_empty() {
                        let len = trimmed.len().min(150);
                        highlights.push(format!("{}...", &trimmed[..len]));
                    }
                }
                if highlights.is_empty() {
                    highlights.push(format!("File: {} | Path: {}", filename, path));
                }
            }

            items.push(SearchResultItem {
                id: path.clone(),
                path,
                filename,
                extension,
                size,
                mod_time,
                is_ocr,
                score,
                highlights,
            });
        }

        Ok(SearchResult {
            query: query_str.to_string(),
            total_hits: items.len(),
            duration_ms: start.elapsed().as_millis(),
            items,
        })
    }

    pub fn find_duplicates(&self) -> Result<DuplicateReport, Box<dyn std::error::Error + Send + Sync>> {
        let searcher = self.reader.searcher();
        let query = AllQuery;
        // Read all docs
        let num_docs = searcher.num_docs() as usize;
        let top_docs = searcher.search(&query, &TopDocs::with_limit(num_docs.max(1)).order_by_score())?;

        let mut hash_map: HashMap<String, Vec<DuplicateFile>> = HashMap::new();

        for (_, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address)?;

            let path = doc
                .get_first(self.raw_path_field)
                .or_else(|| doc.get_first(self.path_field))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let filename = doc.get_first(self.filename_field).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let extension = doc
                .get_first(self.ext_field)
                .and_then(|v| v.as_str())
                .map(|s| s.split_whitespace().next().unwrap_or(s))
                .unwrap_or("")
                .to_string();
            let size = doc.get_first(self.size_field).and_then(|v| v.as_i64()).unwrap_or(0);
            let mod_time = doc.get_first(self.mod_time_field).and_then(|v| v.as_i64()).unwrap_or(0);
            let hash = doc.get_first(self.hash_field).and_then(|v| v.as_str()).unwrap_or("").to_string();

            if !hash.is_empty() && !path.is_empty() {
                hash_map.entry(hash).or_default().push(DuplicateFile {
                    path,
                    filename,
                    extension,
                    size,
                    mod_time,
                });
            }
        }

        let mut groups = Vec::new();
        let mut total_duplicates = 0;
        let mut total_wasted_bytes = 0i64;

        for (hash, files) in hash_map {
            if files.len() > 1 {
                let file_size = files[0].size;
                let redundant_count = files.len() - 1;
                let wasted = (redundant_count as i64) * file_size;

                total_duplicates += redundant_count;
                total_wasted_bytes += wasted;

                groups.push(DuplicateGroup {
                    hash,
                    size: file_size,
                    wasted_bytes: wasted,
                    count: files.len(),
                    files,
                });
            }
        }

        // Sort descending by wasted space
        groups.sort_by(|a, b| {
            if a.wasted_bytes == b.wasted_bytes {
                b.count.cmp(&a.count)
            } else {
                b.wasted_bytes.cmp(&a.wasted_bytes)
            }
        });

        Ok(DuplicateReport {
            total_groups: groups.len(),
            total_duplicates,
            total_wasted_bytes,
            groups,
        })
    }
}

/// Preprocesses user queries to support intuitive syntax like:
/// - `size:>10MB`, `size:<500KB`, `size:small`, `size:1MB..5MB`
/// - `type:pdf`, `type:image`, `type:code`, `type:doc`, `ext:pdf`
/// - `path:Dropbox`, `folder:Finance`, `dir:Docs`
/// - `name:report`, `file:invoice`
pub fn preprocess_query(query: &str) -> String {
    let tokens = split_query_tokens(query);
    let mut result_parts = Vec::new();

    for token in tokens {
        let lower = token.to_lowercase();
        if let Some(rest) = lower.strip_prefix("size:") {
            result_parts.push(transform_size_filter(rest, &token));
        } else if let Some(rest) = lower.strip_prefix("type:") {
            result_parts.push(transform_type_filter(rest));
        } else if let Some(rest) = lower.strip_prefix("kind:") {
            result_parts.push(transform_type_filter(rest));
        } else if let Some(rest) = lower.strip_prefix("ext:") {
            let clean = rest.trim_start_matches('.');
            result_parts.push(format!("ext:{}", clean));
        } else if lower.starts_with("folder:") {
            result_parts.push(format!("path:{}", &token[7..]));
        } else if lower.starts_with("dir:") {
            result_parts.push(format!("path:{}", &token[4..]));
        } else if lower.starts_with("name:") {
            result_parts.push(format!("filename:{}", &token[5..]));
        } else if lower.starts_with("file:") {
            result_parts.push(format!("filename:{}", &token[5..]));
        } else {
            result_parts.push(token);
        }
    }

    result_parts.join(" ")
}

fn transform_size_filter(rest: &str, orig_token: &str) -> String {
    let trimmed = rest.trim();
    match trimmed {
        "small" => return "size:[0 TO 102400]".to_string(),
        "medium" => return "size:[102400 TO 10485760]".to_string(),
        "large" => return "size:[10485760 TO 104857600]".to_string(),
        "huge" => return "size:>104857600".to_string(),
        _ => {}
    }

    // Range e.g. size:1MB..10MB
    if let Some((low_str, high_str)) = trimmed.split_once("..") {
        let low_b = parse_size_to_bytes(low_str).unwrap_or(0);
        let high_b = parse_size_to_bytes(high_str).unwrap_or(i64::MAX);
        return format!("size:[{} TO {}]", low_b, high_b);
    }

    // Range e.g. size:[1MB TO 10MB]
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        let inner = &trimmed[1..trimmed.len() - 1];
        let parts: Vec<&str> = inner.split_whitespace().collect();
        if parts.len() == 3 && parts[1].eq_ignore_ascii_case("TO") {
            let low_b = if parts[0] == "*" {
                "*".to_string()
            } else {
                parse_size_to_bytes(parts[0]).map(|b| b.to_string()).unwrap_or_else(|| parts[0].to_string())
            };
            let high_b = if parts[2] == "*" {
                "*".to_string()
            } else {
                parse_size_to_bytes(parts[2]).map(|b| b.to_string()).unwrap_or_else(|| parts[2].to_string())
            };
            return format!("size:[{} TO {}]", low_b, high_b);
        }
    }

    // Comparison operators: >=, <=, >, <
    if let Some(val) = trimmed.strip_prefix(">=") {
        if let Some(bytes) = parse_size_to_bytes(val) {
            return format!("size:>={}", bytes);
        }
    } else if let Some(val) = trimmed.strip_prefix("<=") {
        if let Some(bytes) = parse_size_to_bytes(val) {
            return format!("size:<={}", bytes);
        }
    } else if let Some(val) = trimmed.strip_prefix('>') {
        if let Some(bytes) = parse_size_to_bytes(val) {
            return format!("size:>{}", bytes);
        }
    } else if let Some(val) = trimmed.strip_prefix('<') {
        if let Some(bytes) = parse_size_to_bytes(val) {
            return format!("size:<{}", bytes);
        }
    } else if let Some(bytes) = parse_size_to_bytes(trimmed) {
        return format!("size:{}", bytes);
    }

    orig_token.to_string()
}

fn transform_type_filter(rest: &str) -> String {
    let clean = rest.trim().to_lowercase();
    match clean.as_str() {
        "image" | "images" | "photo" | "photos" | "picture" | "pictures" => {
            "(ext:png OR ext:jpg OR ext:jpeg)".to_string()
        }
        "code" | "source" | "dev" => {
            "(ext:rs OR ext:go OR ext:py OR ext:js OR ext:ts OR ext:html OR ext:css OR ext:sh)".to_string()
        }
        "doc" | "docs" | "document" | "documents" => {
            "(ext:pdf OR ext:docx OR ext:txt OR ext:md)".to_string()
        }
        "sheet" | "sheets" | "excel" | "spreadsheet" => {
            "(ext:csv OR ext:xlsx OR ext:xls)".to_string()
        }
        "text" | "txt" => "ext:txt".to_string(),
        "markdown" | "md" => "(ext:md OR ext:markdown)".to_string(),
        "pdf" => "ext:pdf".to_string(),
        "word" | "docx" => "ext:docx".to_string(),
        other => {
            let ext_name = other.trim_start_matches('.');
            format!("ext:{}", ext_name)
        }
    }
}

pub fn parse_size_to_bytes(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let lower = s.to_lowercase();
    let (num_str, multiplier) = if lower.ends_with("gb") || lower.ends_with('g') {
        let prefix = lower.trim_end_matches(|c: char| c == 'g' || c == 'b');
        (prefix, 1024 * 1024 * 1024i64)
    } else if lower.ends_with("mb") || lower.ends_with('m') {
        let prefix = lower.trim_end_matches(|c: char| c == 'm' || c == 'b');
        (prefix, 1024 * 1024i64)
    } else if lower.ends_with("kb") || lower.ends_with('k') {
        let prefix = lower.trim_end_matches(|c: char| c == 'k' || c == 'b');
        (prefix, 1024i64)
    } else if lower.ends_with('b') {
        let prefix = lower.trim_end_matches('b');
        (prefix, 1i64)
    } else {
        (lower.as_str(), 1i64)
    };

    num_str.trim().parse::<f64>().ok().map(|num| (num * (multiplier as f64)) as i64)
}

fn split_query_tokens(query: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut in_brackets = false;

    for ch in query.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                cur.push(ch);
            }
            '[' if !in_quotes => {
                in_brackets = true;
                cur.push(ch);
            }
            ']' if !in_quotes => {
                in_brackets = false;
                cur.push(ch);
            }
            ' ' | '\t' | '\r' | '\n' if !in_quotes && !in_brackets => {
                if !cur.trim().is_empty() {
                    tokens.push(cur.trim().to_string());
                    cur.clear();
                }
            }
            _ => cur.push(ch),
        }
    }

    if !cur.trim().is_empty() {
        tokens.push(cur.trim().to_string());
    }

    tokens
}
