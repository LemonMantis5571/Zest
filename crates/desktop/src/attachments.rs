//! File attachment prep for the composer.
//!
//! PDFs go through the local PDF inspector for classification + Markdown
//! extraction (no OCR). Images become Messages API image blocks. Other files
//! are read as UTF-8 text when possible.

use std::path::{Path, PathBuf};

use base64::Engine;
use serde::Serialize;
use serde_json::{json, Value};
use zest_core::truncate_chars;

/// Soft cap so a single text attach cannot blow the context window.
const MAX_ATTACHMENT_CHARS: usize = 100_000;
const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedAttachment {
    pub id: String,
    pub name: String,
    pub path: String,
    /// `pdf` | `text` | `image` | `error`
    pub kind: String,
    /// `done` | `error`
    pub status: String,
    /// Short label for chips / display message.
    pub detail: String,
    /// Text body for pdf/text; unused for images.
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    /// Raw base64 (no data-URL prefix) for image blocks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_base64: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentInput {
    pub name: String,
    pub detail: String,
    pub content: Option<String>,
    pub status: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub media_type: Option<String>,
    #[serde(default)]
    pub data_base64: Option<String>,
}

pub fn prepare_paths(paths: &[PathBuf]) -> Vec<PreparedAttachment> {
    paths.iter().map(|p| prepare_one(p)).collect()
}

pub fn prepare_image_bytes(bytes: &[u8], media_type: &str, name: &str) -> PreparedAttachment {
    let id = format!("att-{}", zest_core::new_id("file"));
    if bytes.is_empty() {
        return PreparedAttachment {
            id,
            name: name.to_string(),
            path: name.to_string(),
            kind: "error".into(),
            status: "error".into(),
            detail: "empty image".into(),
            content: None,
            media_type: None,
            data_base64: None,
        };
    }
    if bytes.len() > MAX_IMAGE_BYTES {
        return PreparedAttachment {
            id,
            name: name.to_string(),
            path: name.to_string(),
            kind: "error".into(),
            status: "error".into(),
            detail: format!(
                "image too large (max {} MB)",
                MAX_IMAGE_BYTES / (1024 * 1024)
            ),
            content: None,
            media_type: None,
            data_base64: None,
        };
    }
    let mt = normalize_media_type(media_type);
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    let kb = bytes.len().div_ceil(1024);
    PreparedAttachment {
        id,
        name: name.to_string(),
        path: name.to_string(),
        kind: "image".into(),
        status: "done".into(),
        detail: format!("{kb} KB · {mt}"),
        content: None,
        media_type: Some(mt),
        data_base64: Some(b64),
    }
}

fn prepare_one(path: &Path) -> PreparedAttachment {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file")
        .to_string();
    let display = path.display().to_string();
    let id = format!("att-{}", zest_core::new_id("file"));

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if ext == "pdf" {
        return prepare_pdf(path, id, name, display);
    }
    if matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp") {
        return prepare_image_path(path, id, name, display, &ext);
    }

    match read_text_file(path) {
        Ok(text) => {
            let chars = text.chars().count();
            let body = truncate_chars(&text, MAX_ATTACHMENT_CHARS);
            PreparedAttachment {
                id,
                name,
                path: display,
                kind: "text".into(),
                status: "done".into(),
                detail: format!("{chars} chars"),
                content: Some(body),
                media_type: None,
                data_base64: None,
            }
        }
        Err(err) => PreparedAttachment {
            id,
            name,
            path: display,
            kind: "error".into(),
            status: "error".into(),
            detail: err,
            content: None,
            media_type: None,
            data_base64: None,
        },
    }
}

fn prepare_image_path(
    path: &Path,
    id: String,
    name: String,
    display: String,
    ext: &str,
) -> PreparedAttachment {
    match std::fs::read(path) {
        Ok(bytes) => {
            let mut att = prepare_image_bytes(&bytes, media_type_for_ext(ext), &name);
            att.id = id;
            att.path = display;
            att
        }
        Err(err) => PreparedAttachment {
            id,
            name,
            path: display,
            kind: "error".into(),
            status: "error".into(),
            detail: err.to_string(),
            content: None,
            media_type: None,
            data_base64: None,
        },
    }
}

fn prepare_pdf(path: &Path, id: String, name: String, display: String) -> PreparedAttachment {
    match pdf_inspector::process_pdf(path) {
        Ok(result) => {
            let kind_label = format!("{:?}", result.pdf_type);
            let pages = result.page_count;
            match result.markdown {
                Some(md) if !md.trim().is_empty() => {
                    let body = truncate_chars(&md, MAX_ATTACHMENT_CHARS);
                    PreparedAttachment {
                        id,
                        name,
                        path: display,
                        kind: "pdf".into(),
                        status: "done".into(),
                        detail: format!("{kind_label}, {pages} pages"),
                        content: Some(body),
                        media_type: None,
                        data_base64: None,
                    }
                }
                _ => PreparedAttachment {
                    id,
                    name,
                    path: display,
                    kind: "pdf".into(),
                    status: "error".into(),
                    detail: format!(
                        "{kind_label}, {pages} pages — no extractable text (OCR not available)"
                    ),
                    content: None,
                    media_type: None,
                    data_base64: None,
                },
            }
        }
        Err(err) => PreparedAttachment {
            id,
            name,
            path: display,
            kind: "error".into(),
            status: "error".into(),
            detail: format!("PDF read failed: {err}"),
            content: None,
            media_type: None,
            data_base64: None,
        },
    }
}

fn read_text_file(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    if bytes.iter().take(8192).any(|&b| b == 0) {
        return Err("binary file — only text, images, and PDF are supported".into());
    }
    String::from_utf8(bytes).map_err(|_| "not valid UTF-8 text".into())
}

fn media_type_for_ext(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "image/png",
    }
}

fn normalize_media_type(raw: &str) -> String {
    let t = raw.trim().to_ascii_lowercase();
    match t.as_str() {
        "image/jpg" => "image/jpeg".into(),
        "image/png" | "image/jpeg" | "image/gif" | "image/webp" => t,
        other if other.starts_with("image/") => other.to_string(),
        _ => "image/png".into(),
    }
}

/// Compact line shown in the chat bubble.
pub fn format_display_message(text: &str, attachments: &[AttachmentInput]) -> String {
    let mut out = text.trim().to_string();
    if attachments.is_empty() {
        return out;
    }
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    for att in attachments {
        out.push_str(&format!("Attached: {} ({})", att.name, att.detail));
        out.push('\n');
    }
    out.trim_end().to_string()
}

/// Build Messages API user content blocks (text + images).
pub fn build_user_content(text: &str, attachments: &[AttachmentInput]) -> Vec<Value> {
    let mut blocks = Vec::new();
    let mut text_body = text.trim().to_string();

    let text_atts: Vec<_> = attachments
        .iter()
        .filter(|a| {
            let kind = a.kind.as_deref().unwrap_or("");
            kind != "image"
                && a.status == "done"
                && a.content.as_ref().is_some_and(|c| !c.trim().is_empty())
        })
        .collect();
    let images: Vec<_> = attachments
        .iter()
        .filter(|a| {
            a.kind.as_deref() == Some("image")
                && a.status == "done"
                && a.data_base64.as_ref().is_some_and(|d| !d.is_empty())
        })
        .collect();
    let failed: Vec<_> = attachments
        .iter()
        .filter(|a| {
            !(a.status == "done"
                && (a.content.as_ref().is_some_and(|c| !c.trim().is_empty())
                    || (a.kind.as_deref() == Some("image")
                        && a.data_base64.as_ref().is_some_and(|d| !d.is_empty()))))
        })
        .collect();

    if !text_atts.is_empty() {
        if !text_body.is_empty() {
            text_body.push_str("\n\n");
        }
        text_body.push_str("---\nAttached files:\n");
        for att in &text_atts {
            let content = att.content.as_deref().unwrap_or("");
            text_body.push_str(&format!(
                "\n### {}\n({})\n\n{}\n",
                att.name, att.detail, content
            ));
        }
    }
    if !failed.is_empty() {
        if !text_body.is_empty() {
            text_body.push_str("\n\n");
        }
        text_body.push_str("Could not extract:\n");
        for att in failed {
            text_body.push_str(&format!("- {} — {}\n", att.name, att.detail));
        }
    }

    if !text_body.trim().is_empty() {
        blocks.push(json!({ "type": "text", "text": text_body.trim() }));
    } else if images.is_empty() {
        blocks.push(json!({ "type": "text", "text": "(empty)" }));
    }

    for img in images {
        let media = img.media_type.clone().unwrap_or_else(|| "image/png".into());
        let data = img.data_base64.clone().unwrap_or_default();
        blocks.push(json!({
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": media,
                "data": data,
            }
        }));
    }

    blocks
}

pub fn has_images(attachments: &[AttachmentInput]) -> bool {
    attachments.iter().any(|a| {
        a.kind.as_deref() == Some("image")
            && a.status == "done"
            && a.data_base64.as_ref().is_some_and(|d| !d.is_empty())
    })
}

pub fn has_usable_attachment(attachments: &[AttachmentInput]) -> bool {
    attachments.iter().any(|a| {
        a.status == "done"
            && (a.content.as_ref().is_some_and(|c| !c.trim().is_empty())
                || (a.kind.as_deref() == Some("image")
                    && a.data_base64.as_ref().is_some_and(|d| !d.is_empty())))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_lists_attachment_names() {
        let atts = vec![AttachmentInput {
            name: "a.pdf".into(),
            detail: "TextBased, 2 pages".into(),
            content: Some("hello".into()),
            status: "done".into(),
            kind: Some("pdf".into()),
            media_type: None,
            data_base64: None,
        }];
        let display = format_display_message("Please summarize", &atts);
        assert!(display.contains("Please summarize"));
        assert!(display.contains("Attached: a.pdf"));
        assert!(!display.contains("hello"));
    }

    #[test]
    fn image_block_in_user_content() {
        let atts = vec![AttachmentInput {
            name: "shot.png".into(),
            detail: "12 KB".into(),
            content: None,
            status: "done".into(),
            kind: Some("image".into()),
            media_type: Some("image/png".into()),
            data_base64: Some("AAAA".into()),
        }];
        let blocks = build_user_content("what is this?", &atts);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0]["type"], "text");
        assert_eq!(blocks[1]["type"], "image");
        assert_eq!(blocks[1]["source"]["data"], "AAAA");
    }
}
