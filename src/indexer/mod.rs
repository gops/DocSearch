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
        let path_field = schema_builder.add_text_field("path", STRING | STORED);
        let filename_field = schema_builder.add_text_field("filename", TEXT | STORED);
        let ext_field = schema_builder.add_text_field("ext", STRING | STORED);
        let size_field = schema_builder.add_i64_field("size", STORED | INDEXED);
        let mod_time_field = schema_builder.add_i64_field("mod_time", STORED | INDEXED);
        let hash_field = schema_builder.add_text_field("hash", STRING | STORED);
        let is_ocr_field = schema_builder.add_text_field("is_ocr", STRING | STORED);
        let content_field = schema_builder.add_text_field("content", TEXT | STORED);
        let schema = schema_builder.build();

        let index = Index::open_or_create(
            tantivy::directory::MmapDirectory::open(index_dir.as_ref())?,
            schema.clone(),
        )?;

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;

        // Allocate 50MB for writer buffer
        let writer = Arc::new(Mutex::new(index.writer(50_000_000)?));

        Ok(Self {
            schema,
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
        doc.add_text(self.path_field, path);
        doc.add_text(self.filename_field, filename);
        doc.add_text(self.ext_field, ext);
        doc.add_i64(self.size_field, size);
        doc.add_i64(self.mod_time_field, mod_time);
        doc.add_text(self.hash_field, hash);
        doc.add_text(self.is_ocr_field, if is_ocr { "true" } else { "false" });
        doc.add_text(self.content_field, content);

        let writer = self.writer.lock().map_err(|e| e.to_string())?;
        let term = Term::from_field_text(self.path_field, path);
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
        let term = Term::from_field_text(self.path_field, path);
        writer.delete_term(term);
        Ok(())
    }

    pub fn get_document_hash(&self, path: &str) -> Result<Option<String>, Box<dyn std::error::Error + Send + Sync>> {
        let searcher = self.reader.searcher();
        let term = Term::from_field_text(self.path_field, path);
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

        let mut query_parser = QueryParser::for_index(&self.index, vec![self.filename_field, self.content_field]);
        query_parser.set_field_boost(self.filename_field, 4.0);
        query_parser.set_field_boost(self.content_field, 1.0);

        let query = match query_parser.parse_query(query_str) {
            Ok(q) => q,
            Err(_) => {
                // If query parser failed (e.g. special characters), fallback to fuzzy prefix / term match
                let term = Term::from_field_text(self.filename_field, query_str);
                Box::new(TermQuery::new(term, IndexRecordOption::Basic))
            }
        };

        let snippet_generator = tantivy::snippet::SnippetGenerator::create(&searcher, &*query, self.content_field).ok();

        let top_docs = searcher.search(&*query, &TopDocs::with_limit(limit).order_by_score())?;
        let mut items = Vec::new();

        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address)?;

            let path = doc.get_first(self.path_field).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let filename = doc.get_first(self.filename_field).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let extension = doc.get_first(self.ext_field).and_then(|v| v.as_str()).unwrap_or("").to_string();
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
                    let len = content.len().min(150);
                    highlights.push(format!("{}...", &content[..len]));
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

            let path = doc.get_first(self.path_field).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let filename = doc.get_first(self.filename_field).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let extension = doc.get_first(self.ext_field).and_then(|v| v.as_str()).unwrap_or("").to_string();
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
