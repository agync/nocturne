//! Reading documents without a model: plain text, Markdown, PDF and Word files.

use std::io::Read;
use std::path::Path;

use nocturne_engine::{MindError, Understanding};

pub const MAX_PAGES: usize = 100;

pub fn read(path: &Path) -> Result<Understanding, MindError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let bytes = std::fs::read(path).map_err(|e| MindError::Failed(e.to_string()))?;
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
    let pages = pdf_extract::extract_text_from_mem_by_pages(bytes)
        .map_err(|e| MindError::Failed(format!("could not read the pdf: {e}")))?;
    if pages.len() > MAX_PAGES {
        return Err(MindError::TooLong);
    }
    if pages.iter().all(|p| p.trim().is_empty()) {
        return Err(MindError::Unsupported("scanned".into()));
    }
    let count = pages.len() as u32;
    let text = pages
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    Ok((text, count))
}

fn docx(bytes: &[u8]) -> Result<(String, Option<u32>), MindError> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| MindError::Failed(format!("could not open the document: {e}")))?;
    let body = entry(&mut zip, "word/document.xml")
        .ok_or_else(|| MindError::Failed("no text in the document".into()))?;
    let text = word_text(&body)?;
    let pages = entry(&mut zip, "docProps/app.xml").and_then(|xml| {
        let start = xml.find("<Pages>")? + "<Pages>".len();
        let end = xml[start..].find("</Pages>")? + start;
        xml[start..end].trim().parse().ok()
    });
    Ok((text, pages))
}

fn entry(zip: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>, name: &str) -> Option<String> {
    let mut file = zip.by_name(name).ok()?;
    let mut out = String::new();
    file.read_to_string(&mut out).ok()?;
    Some(out)
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
            Event::Start(e) if e.name().as_ref() == b"w:t" => in_text = true,
            Event::End(e) if e.name().as_ref() == b"w:t" => in_text = false,
            Event::End(e) if e.name().as_ref() == b"w:p" => out.push('\n'),
            Event::Empty(e) if e.name().as_ref() == b"w:tab" => out.push('\t'),
            Event::Empty(e) if e.name().as_ref() == b"w:br" => out.push('\n'),
            Event::Text(t) if in_text => {
                let text = t
                    .xml_content()
                    .map_err(|e| MindError::Failed(format!("could not read the document: {e}")))?;
                out.push_str(&text);
            }
            Event::GeneralRef(r) if in_text => {
                if let Some(c) = r.resolve_char_ref().ok().flatten() {
                    out.push(c);
                } else {
                    match &*r {
                        b"amp" => out.push('&'),
                        b"lt" => out.push('<'),
                        b"gt" => out.push('>'),
                        b"quot" => out.push('"'),
                        b"apos" => out.push('\''),
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
