//! Reading documents without a model: plain text, Markdown, PDF and Word files.

use std::io::Read;
use std::path::Path;

use nocturne_engine::{MindError, Understanding};

pub const MAX_PAGES: usize = 100;
/// Largest document file read at all.
pub const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;
/// Largest text a document may yield, and the most a Word body may expand to when unzipped.
pub const MAX_TEXT_BYTES: usize = 8 * 1024 * 1024;
const MAX_XML_BYTES: u64 = 24 * 1024 * 1024;
const MAX_PROPS_BYTES: u64 = 64 * 1024;

pub fn read(path: &Path) -> Result<Understanding, MindError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let bytes = read_capped(path)?;
    let (text, pages, markdown) = match ext.as_str() {
        "txt" | "text" => (lossy(&bytes), None, false),
        "md" | "markdown" => (lossy(&bytes), None, true),
        "pdf" => {
            let (text, pages) = pdf(&bytes)?;
            (text, Some(pages), false)
        }
        "docx" => {
            let (text, pages) = docx(&bytes)?;
            (text, pages, false)
        }
        other => return Err(MindError::Unsupported(format!("document type .{other}"))),
    };
    if text.len() > MAX_TEXT_BYTES {
        return Err(MindError::TooLong);
    }
    if text.trim().is_empty() {
        return Err(MindError::Unsupported("empty".into()));
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(Understanding {
        title: title_of(&text, markdown).unwrap_or(stem),
        text,
        summary: None,
        language: None,
        pages,
    })
}

/// The file's bytes, refusing anything over the limit before and while reading.
fn read_capped(path: &Path) -> Result<Vec<u8>, MindError> {
    let fail = |e: std::io::Error| MindError::Failed(e.to_string());
    let file = std::fs::File::open(path).map_err(fail)?;
    if file.metadata().map_err(fail)?.len() > MAX_FILE_BYTES {
        return Err(MindError::TooLong);
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(fail)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(MindError::TooLong);
    }
    Ok(bytes)
}

fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn title_of(text: &str, markdown: bool) -> Option<String> {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty())?;
    let line = if markdown {
        line.trim_start_matches('#').trim()
    } else {
        line
    };
    if line.is_empty() {
        return None;
    }
    Some(line.chars().take(80).collect())
}

fn pdf(bytes: &[u8]) -> Result<(String, u32), MindError> {
    let unreadable =
        |e: &dyn std::fmt::Display| MindError::Failed(format!("could not read the pdf: {e}"));
    // Count first, so a huge file is refused before any page is decoded.
    let expected = lopdf::Document::load_mem(bytes)
        .map_err(|e| unreadable(&e))?
        .get_pages()
        .len();
    if expected > MAX_PAGES {
        return Err(MindError::TooLong);
    }
    let pages = pdf_extract::extract_text_from_mem_by_pages(bytes).map_err(|e| unreadable(&e))?;
    // The extractor stops quietly at the first page it cannot read; that is a failure here.
    if pages.len() != expected {
        return Err(MindError::Failed(format!(
            "page {} of the pdf could not be read",
            pages.len() + 1
        )));
    }
    if pages.iter().all(|p| p.trim().is_empty()) {
        return Err(MindError::Unsupported("scanned".into()));
    }
    let text = pages
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    Ok((text, expected as u32))
}

fn docx(bytes: &[u8]) -> Result<(String, Option<u32>), MindError> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| MindError::Failed(format!("could not open the document: {e}")))?;
    let body = entry(&mut zip, "word/document.xml", MAX_XML_BYTES)?
        .ok_or_else(|| MindError::Failed("no text in the document".into()))?;
    let text = word_text(&body)?;
    let pages = entry(&mut zip, "docProps/app.xml", MAX_PROPS_BYTES)
        .ok()
        .flatten()
        .and_then(|xml| {
            let start = xml.find("<Pages>")? + "<Pages>".len();
            let end = xml[start..].find("</Pages>")? + start;
            xml[start..end].trim().parse().ok()
        });
    Ok((text, pages))
}

/// One part of the archive as text, refusing a part that is, or turns out to be, over `limit`.
/// The size the archive declares is not trusted: reading stops one byte past the limit.
fn entry(
    zip: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>,
    name: &str,
    limit: u64,
) -> Result<Option<String>, MindError> {
    let Ok(file) = zip.by_name(name) else {
        return Ok(None);
    };
    if file.size() > limit {
        return Err(MindError::TooLong);
    }
    let mut out = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut out)
        .map_err(|e| MindError::Failed(format!("could not read the document: {e}")))?;
    if out.len() as u64 > limit {
        return Err(MindError::TooLong);
    }
    Ok(Some(String::from_utf8_lossy(&out).into_owned()))
}

/// The text of every `w:t` run, with a line break at each paragraph end.
fn word_text(xml: &str) -> Result<String, MindError> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut out = String::new();
    let mut in_text = false;
    loop {
        match reader
            .read_event()
            .map_err(|e| MindError::Failed(format!("could not read the document: {e}")))?
        {
            Event::Start(e) if e.name().as_ref() == "w:t" => in_text = true,
            Event::End(e) if e.name().as_ref() == "w:t" => in_text = false,
            Event::End(e) if e.name().as_ref() == "w:p" => out.push('\n'),
            Event::Empty(e) if e.name().as_ref() == "w:tab" => out.push('\t'),
            Event::Empty(e) if e.name().as_ref() == "w:br" => out.push('\n'),
            Event::Text(t) if in_text => {
                out.push_str(&t.xml10_content());
            }
            Event::GeneralRef(r) if in_text => {
                if let Some(c) = r.resolve_char_ref().ok().flatten() {
                    out.push(c);
                } else {
                    match &*r {
                        "amp" => out.push('&'),
                        "lt" => out.push('<'),
                        "gt" => out.push('>'),
                        "quot" => out.push('"'),
                        "apos" => out.push('\''),
                        _ => {}
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(out.trim().to_string())
}
