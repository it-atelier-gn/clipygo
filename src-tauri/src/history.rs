use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

const KEYRING_SERVICE: &str = "clipygo";
const KEYRING_USER: &str = "history-content-key";
const MAX_TEXT_LEN_FOR_PREVIEW: usize = 200;
const PREVIEW_MAX_LINES: usize = 3;
const NONCE_LEN: usize = 24;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntryView {
    pub id: Uuid,
    pub timestamp: i64,
    pub kind_tag: String,
    pub preview: String,
    pub line_count: u32,
    pub mime: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub size_bytes: u64,
    pub matched_pattern: Option<String>,
    pub pinned: bool,
    pub last_sent_to: Option<String>,
    pub formats: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CapturedImage {
    pub mime: String,
    pub width: u32,
    pub height: u32,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct CapturedContent {
    pub files: Option<Vec<String>>,
    pub image: Option<CapturedImage>,
    pub html: Option<String>,
    pub rtf: Option<String>,
    pub text: Option<String>,
}

impl CapturedContent {
    pub fn primary_kind(&self) -> Option<&'static str> {
        if self.files.is_some() {
            Some("files")
        } else if self.image.is_some() {
            Some("image")
        } else if self.html.is_some() {
            Some("html")
        } else if self.rtf.is_some() {
            Some("rtf")
        } else if self.text.is_some() {
            Some("text")
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FullEntry {
    pub kind: String,
    pub mime: Option<String>,
    pub text: Option<String>,
    pub html: Option<String>,
    pub rtf: Option<String>,
    pub files: Option<Vec<String>>,
    pub image: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stats {
    pub items: u64,
    pub bytes_used: u64,
    pub bytes_cap: u64,
    pub persisted_to_disk: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FilterKind {
    #[default]
    All,
    Text,
    Image,
    Html,
    Rtf,
    Files,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Filter {
    #[serde(default)]
    pub kind: FilterKind,
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub pinned_only: bool,
}

pub struct HistoryCoordinator {
    conn: Connection,
    persisted: bool,
    cap_bytes: u64,
    key: [u8; 32],
}

impl HistoryCoordinator {
    pub fn new_in_memory(cap_bytes: u64) -> Result<Self, String> {
        let conn = Connection::open_in_memory().map_err(|e| e.to_string())?;
        init_schema(&conn)?;
        Ok(Self {
            conn,
            persisted: false,
            cap_bytes,
            key: random_key(),
        })
    }

    pub fn new_persisted(path: PathBuf, key: [u8; 32], cap_bytes: u64) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let conn = Connection::open(&path).map_err(|e| e.to_string())?;
        init_schema(&conn)?;
        Ok(Self {
            conn,
            persisted: true,
            cap_bytes,
            key,
        })
    }

    pub fn set_cap(&mut self, cap_bytes: u64) -> Result<(), String> {
        self.cap_bytes = cap_bytes;
        self.evict_until_under_cap()
    }

    pub fn stats(&self) -> Result<Stats, String> {
        let (items, bytes): (u64, u64) = self
            .conn
            .query_row(
                "SELECT COALESCE(COUNT(*),0), COALESCE(SUM(size_bytes),0) FROM entries",
                [],
                |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u64)),
            )
            .map_err(|e| e.to_string())?;
        Ok(Stats {
            items,
            bytes_used: bytes,
            bytes_cap: self.cap_bytes,
            persisted_to_disk: self.persisted,
        })
    }

    #[cfg(test)]
    pub fn insert_text(
        &mut self,
        content: String,
        matched_pattern: Option<String>,
    ) -> Result<Uuid, String> {
        self.insert_captured(
            CapturedContent {
                text: Some(content),
                ..Default::default()
            },
            matched_pattern,
        )
    }

    #[cfg(test)]
    pub fn insert_html(
        &mut self,
        html: String,
        matched_pattern: Option<String>,
    ) -> Result<Uuid, String> {
        self.insert_captured(
            CapturedContent {
                html: Some(html),
                ..Default::default()
            },
            matched_pattern,
        )
    }

    #[cfg(test)]
    pub fn insert_rtf(
        &mut self,
        rtf: String,
        matched_pattern: Option<String>,
    ) -> Result<Uuid, String> {
        self.insert_captured(
            CapturedContent {
                rtf: Some(rtf),
                ..Default::default()
            },
            matched_pattern,
        )
    }

    #[cfg(test)]
    pub fn insert_files(
        &mut self,
        files: Vec<String>,
        matched_pattern: Option<String>,
    ) -> Result<Uuid, String> {
        self.insert_captured(
            CapturedContent {
                files: Some(files),
                ..Default::default()
            },
            matched_pattern,
        )
    }

    #[cfg(test)]
    pub fn insert_image(
        &mut self,
        mime: String,
        width: u32,
        height: u32,
        bytes: Vec<u8>,
        matched_pattern: Option<String>,
    ) -> Result<Uuid, String> {
        self.insert_captured(
            CapturedContent {
                image: Some(CapturedImage {
                    mime,
                    width,
                    height,
                    bytes,
                }),
                ..Default::default()
            },
            matched_pattern,
        )
    }

    pub fn insert_captured(
        &mut self,
        captured: CapturedContent,
        matched_pattern: Option<String>,
    ) -> Result<Uuid, String> {
        let primary = captured
            .primary_kind()
            .ok_or_else(|| "empty capture".to_string())?;
        let id = Uuid::new_v4();
        let ts = now_ms();
        let key = self.key;

        let files_joined = captured.files.as_ref().map(|f| f.join("\n"));
        let primary_content: Option<&str> = match primary {
            "files" => files_joined.as_deref(),
            "html" => captured.html.as_deref(),
            "rtf" => captured.rtf.as_deref(),
            "text" => captured.text.as_deref(),
            _ => None,
        };
        let mime: Option<&str> = match primary {
            "image" => captured.image.as_ref().map(|i| i.mime.as_str()),
            "html" => Some("text/html"),
            "rtf" => Some("text/rtf"),
            _ => None,
        };
        let (width, height) = captured
            .image
            .as_ref()
            .map(|i| (Some(i.width), Some(i.height)))
            .unwrap_or((None, None));

        let mut secondaries: Vec<(&'static str, &str)> = Vec::new();
        for (fmt, content) in [
            ("text", captured.text.as_deref()),
            ("html", captured.html.as_deref()),
            ("rtf", captured.rtf.as_deref()),
        ] {
            if fmt != primary {
                if let Some(c) = content {
                    secondaries.push((fmt, c));
                }
            }
        }

        let mut size: u64 = primary_content.map(|c| c.len() as u64).unwrap_or(0);
        size += captured
            .image
            .as_ref()
            .map(|i| i.bytes.len() as u64)
            .unwrap_or(0);
        size += secondaries.iter().map(|(_, c)| c.len() as u64).sum::<u64>();

        let content_ct = match primary_content {
            Some(c) => Some(encrypt(&key, c.as_bytes())?),
            None => None,
        };
        let image_ct = match captured.image.as_ref() {
            Some(i) => Some(encrypt(&key, &i.bytes)?),
            None => None,
        };
        let mut secondary_cts: Vec<(&'static str, Vec<u8>)> = Vec::new();
        for (fmt, c) in &secondaries {
            secondary_cts.push((fmt, encrypt(&key, c.as_bytes())?));
        }

        let tx = self.conn.transaction().map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO entries (id, timestamp, kind, content_ct, mime, width, height, size_bytes, matched_pattern, pinned) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0)",
            params![
                id.as_bytes().to_vec(),
                ts,
                primary,
                content_ct,
                mime,
                width,
                height,
                size as i64,
                matched_pattern
            ],
        )
        .map_err(|e| e.to_string())?;
        if let Some(ct) = image_ct {
            tx.execute(
                "INSERT INTO images (id, bytes_ct) VALUES (?, ?)",
                params![id.as_bytes().to_vec(), ct],
            )
            .map_err(|e| e.to_string())?;
        }
        for (fmt, ct) in secondary_cts {
            tx.execute(
                "INSERT INTO formats (entry_id, format, content_ct) VALUES (?, ?, ?)",
                params![id.as_bytes().to_vec(), fmt, ct],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())?;
        self.evict_until_under_cap()?;
        Ok(id)
    }

    pub fn list(
        &self,
        filter: &Filter,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<HistoryEntryView>, String> {
        let mut sql = String::from(
            "SELECT id, timestamp, kind, content_ct, mime, width, height, size_bytes, matched_pattern, pinned, last_sent_to, \
             (SELECT GROUP_CONCAT(format) FROM formats WHERE entry_id = entries.id), \
             EXISTS(SELECT 1 FROM images WHERE id = entries.id) \
             FROM entries WHERE 1=1",
        );
        match filter.kind {
            FilterKind::Text => sql.push_str(" AND kind = 'text'"),
            FilterKind::Image => sql.push_str(" AND kind = 'image'"),
            FilterKind::Html => sql.push_str(" AND kind = 'html'"),
            FilterKind::Rtf => sql.push_str(" AND kind = 'rtf'"),
            FilterKind::Files => sql.push_str(" AND kind = 'files'"),
            FilterKind::All => {}
        }
        if filter.pinned_only {
            sql.push_str(" AND pinned = 1");
        }
        sql.push_str(" ORDER BY pinned DESC, timestamp DESC");

        let mut stmt = self.conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                let id_bytes: Vec<u8> = r.get(0)?;
                let id = Uuid::from_slice(&id_bytes).unwrap_or_else(|_| Uuid::nil());
                let kind: String = r.get(2)?;
                let content_ct: Option<Vec<u8>> = r.get(3)?;
                let mime: Option<String> = r.get(4)?;
                let width: Option<i64> = r.get(5)?;
                let height: Option<i64> = r.get(6)?;
                let size_bytes: i64 = r.get(7)?;
                let matched_pattern: Option<String> = r.get(8)?;
                let pinned: i64 = r.get(9)?;
                let last_sent_to: Option<String> = r.get(10)?;
                let secondary_formats: Option<String> = r.get(11)?;
                let has_image: i64 = r.get(12)?;
                Ok((
                    id,
                    r.get::<_, i64>(1)?,
                    kind,
                    content_ct,
                    mime,
                    width,
                    height,
                    size_bytes,
                    matched_pattern,
                    pinned,
                    last_sent_to,
                    secondary_formats,
                    has_image,
                ))
            })
            .map_err(|e| e.to_string())?;

        let query = filter.query.to_lowercase();
        let mut views: Vec<HistoryEntryView> = Vec::new();
        let mut skipped: u32 = 0;
        for row in rows {
            let (
                id,
                timestamp,
                kind,
                content_ct,
                mime,
                width,
                height,
                size_bytes,
                matched_pattern,
                pinned,
                last_sent_to,
                secondary_formats,
                has_image,
            ) = row.map_err(|e| e.to_string())?;

            let mut preview = String::new();
            let mut line_count: u32 = 0;
            let is_text_like = matches!(kind.as_str(), "text" | "html" | "rtf" | "files");
            if is_text_like {
                if let Some(ct) = &content_ct {
                    if let Ok(plain) = decrypt(&self.key, ct) {
                        if let Ok(s) = String::from_utf8(plain) {
                            let searchable = match kind.as_str() {
                                "html" => strip_html(&s),
                                "rtf" => strip_rtf(&s),
                                _ => s.clone(),
                            };
                            if kind == "files" {
                                let (p, n) = files_preview(&s);
                                preview = p;
                                line_count = n;
                            } else {
                                let (p, n) = build_preview(&searchable);
                                preview = p;
                                line_count = n;
                            }
                            if !query.is_empty() && !searchable.to_lowercase().contains(&query) {
                                continue;
                            }
                        }
                    }
                }
            } else if !query.is_empty() {
                let matched_in_pattern = matched_pattern
                    .as_deref()
                    .map(|p| p.to_lowercase().contains(&query))
                    .unwrap_or(false);
                if !matched_in_pattern {
                    continue;
                }
            }

            if skipped < offset {
                skipped += 1;
                continue;
            }
            let mut formats: Vec<String> = Vec::new();
            for f in ["files", "image", "html", "rtf", "text"] {
                let stored = kind == f
                    || (f == "image" && has_image != 0)
                    || secondary_formats
                        .as_deref()
                        .is_some_and(|s| s.split(',').any(|x| x == f));
                if stored {
                    formats.push(f.to_string());
                }
            }
            views.push(HistoryEntryView {
                id,
                timestamp,
                kind_tag: kind,
                preview,
                line_count,
                mime,
                width: width.map(|v| v as u32),
                height: height.map(|v| v as u32),
                size_bytes: size_bytes as u64,
                matched_pattern,
                pinned: pinned != 0,
                last_sent_to,
                formats,
            });
            if views.len() as u32 >= limit {
                break;
            }
        }
        Ok(views)
    }

    pub fn get_image(&self, id: Uuid) -> Result<Vec<u8>, String> {
        let ct: Vec<u8> = self
            .conn
            .query_row(
                "SELECT bytes_ct FROM images WHERE id = ?",
                params![id.as_bytes().to_vec()],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        decrypt(&self.key, &ct)
    }

    pub fn get_full_entry(&self, id: Uuid) -> Result<Option<FullEntry>, String> {
        let row = self
            .conn
            .query_row(
                "SELECT kind, content_ct, mime FROM entries WHERE id = ?",
                params![id.as_bytes().to_vec()],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, Option<Vec<u8>>>(1)?,
                        r.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let (kind, ct, mime) = match row {
            None => return Ok(None),
            Some(v) => v,
        };
        let mut full = FullEntry {
            kind: kind.clone(),
            mime,
            ..Default::default()
        };
        if let Some(ct) = ct {
            let plain = decrypt(&self.key, &ct)?;
            let content = String::from_utf8(plain).map_err(|e| e.to_string())?;
            match kind.as_str() {
                "html" => full.html = Some(content),
                "rtf" => full.rtf = Some(content),
                "files" => {
                    full.files = Some(
                        content
                            .lines()
                            .filter(|l| !l.is_empty())
                            .map(|l| l.to_string())
                            .collect(),
                    )
                }
                _ => full.text = Some(content),
            }
        }
        let mut stmt = self
            .conn
            .prepare("SELECT format, content_ct FROM formats WHERE entry_id = ?")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![id.as_bytes().to_vec()], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (fmt, ct) = row.map_err(|e| e.to_string())?;
            let plain = decrypt(&self.key, &ct)?;
            let content = String::from_utf8(plain).map_err(|e| e.to_string())?;
            match fmt.as_str() {
                "text" => full.text = Some(content),
                "html" => full.html = Some(content),
                "rtf" => full.rtf = Some(content),
                _ => {}
            }
        }
        let image_ct: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT bytes_ct FROM images WHERE id = ?",
                params![id.as_bytes().to_vec()],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if let Some(ct) = image_ct {
            full.image = Some(decrypt(&self.key, &ct)?);
        }
        Ok(Some(full))
    }

    pub fn set_pinned(&self, id: Uuid, pinned: bool) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE entries SET pinned = ? WHERE id = ?",
                params![pinned as i32, id.as_bytes().to_vec()],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn set_last_sent_to(&self, id: Uuid, target: &str) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE entries SET last_sent_to = ? WHERE id = ?",
                params![target, id.as_bytes().to_vec()],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete(&self, id: Uuid) -> Result<(), String> {
        self.conn
            .execute(
                "DELETE FROM entries WHERE id = ?",
                params![id.as_bytes().to_vec()],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn clear(&self, include_pinned: bool) -> Result<(), String> {
        let sql = if include_pinned {
            "DELETE FROM entries"
        } else {
            "DELETE FROM entries WHERE pinned = 0"
        };
        self.conn.execute(sql, []).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn evict_until_under_cap(&mut self) -> Result<(), String> {
        loop {
            let used: i64 = self
                .conn
                .query_row("SELECT COALESCE(SUM(size_bytes),0) FROM entries", [], |r| {
                    r.get(0)
                })
                .map_err(|e| e.to_string())?;
            if (used as u64) <= self.cap_bytes {
                return Ok(());
            }
            let oldest: Option<Vec<u8>> = self
                .conn
                .query_row(
                    "SELECT id FROM entries WHERE pinned = 0 ORDER BY timestamp ASC LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            match oldest {
                Some(id) => {
                    self.conn
                        .execute("DELETE FROM entries WHERE id = ?", params![id])
                        .map_err(|e| e.to_string())?;
                }
                None => return Ok(()),
            }
        }
    }
}

fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         CREATE TABLE IF NOT EXISTS entries (
             id BLOB PRIMARY KEY,
             timestamp INTEGER NOT NULL,
             kind TEXT NOT NULL,
             content_ct BLOB,
             mime TEXT,
             width INTEGER,
             height INTEGER,
             size_bytes INTEGER NOT NULL,
             matched_pattern TEXT,
             pinned INTEGER NOT NULL DEFAULT 0,
             last_sent_to TEXT
         );
         CREATE TABLE IF NOT EXISTS images (
             id BLOB PRIMARY KEY REFERENCES entries(id) ON DELETE CASCADE,
             bytes_ct BLOB NOT NULL
         );
         CREATE TABLE IF NOT EXISTS formats (
             entry_id BLOB NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
             format TEXT NOT NULL,
             content_ct BLOB NOT NULL,
             PRIMARY KEY (entry_id, format)
         );
         CREATE INDEX IF NOT EXISTS idx_entries_ts ON entries(timestamp);
         CREATE INDEX IF NOT EXISTS idx_entries_pinned ON entries(pinned);",
    )
    .map_err(|e| e.to_string())
}

fn encrypt(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, String> {
    use rand::Rng;
    let cipher = XChaCha20Poly1305::new(key.into());
    let mut nonce = [0u8; NONCE_LEN];
    rand::rng().fill_bytes(&mut nonce);
    let ct = cipher
        .encrypt((&nonce).into(), plaintext)
        .map_err(|e| format!("encrypt: {e}"))?;
    let mut out = Vec::with_capacity(NONCE_LEN + ct.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    Ok(out)
}

fn decrypt(key: &[u8; 32], blob: &[u8]) -> Result<Vec<u8>, String> {
    if blob.len() < NONCE_LEN {
        return Err("ciphertext too short".into());
    }
    let (nonce, ct) = blob.split_at(NONCE_LEN);
    let cipher = XChaCha20Poly1305::new(key.into());
    let nonce: &XNonce = nonce.try_into().expect("nonce length checked above");
    cipher
        .decrypt(nonce, ct)
        .map_err(|e| format!("decrypt: {e}"))
}

fn random_key() -> [u8; 32] {
    use rand::Rng;
    let mut k = [0u8; 32];
    rand::rng().fill_bytes(&mut k);
    k
}

fn build_preview(s: &str) -> (String, u32) {
    let line_count = s.lines().count().max(1) as u32;
    let mut snippet: String = s
        .lines()
        .take(PREVIEW_MAX_LINES)
        .collect::<Vec<_>>()
        .join("\n");
    let mut truncated = false;
    if snippet.chars().count() > MAX_TEXT_LEN_FOR_PREVIEW {
        snippet = snippet.chars().take(MAX_TEXT_LEN_FOR_PREVIEW).collect();
        truncated = true;
    }
    if truncated || (line_count as usize) > PREVIEW_MAX_LINES {
        snippet.push('…');
    }
    (snippet, line_count)
}

fn files_preview(joined: &str) -> (String, u32) {
    let files: Vec<&str> = joined.lines().filter(|l| !l.is_empty()).collect();
    let count = files.len() as u32;
    let mut preview = files
        .iter()
        .map(|f| file_name_of(f))
        .collect::<Vec<_>>()
        .join(", ");
    if preview.chars().count() > MAX_TEXT_LEN_FOR_PREVIEW {
        preview = preview
            .chars()
            .take(MAX_TEXT_LEN_FOR_PREVIEW)
            .collect::<String>()
            + "…";
    }
    (preview, count)
}

fn file_name_of(path: &str) -> String {
    let trimmed = path.trim_end_matches(['/', '\\']);
    trimmed
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(trimmed)
        .to_string()
}

pub fn html_to_text(s: &str) -> String {
    strip_html(s)
}

pub fn rtf_to_text(s: &str) -> String {
    strip_rtf(s)
}

fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => {
                in_tag = true;
                out.push(' ');
            }
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let decoded = out
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn strip_rtf(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len();
    let mut out = String::new();
    let mut i = 0usize;
    let mut depth = 0usize;
    let mut skip_depth: Option<usize> = None;
    while i < n {
        match chars[i] {
            '{' => {
                depth += 1;
                i += 1;
            }
            '}' => {
                if let Some(sd) = skip_depth {
                    if depth == sd {
                        skip_depth = None;
                    }
                }
                depth = depth.saturating_sub(1);
                i += 1;
            }
            '\\' => {
                i += 1;
                if i >= n {
                    break;
                }
                let nc = chars[i];
                if nc == '*' {
                    if skip_depth.is_none() {
                        skip_depth = Some(depth);
                    }
                    i += 1;
                } else if nc.is_ascii_alphabetic() {
                    let start = i;
                    while i < n && chars[i].is_ascii_alphabetic() {
                        i += 1;
                    }
                    let word: String = chars[start..i].iter().collect();
                    if i < n && (chars[i] == '-' || chars[i].is_ascii_digit()) {
                        i += 1;
                        while i < n && chars[i].is_ascii_digit() {
                            i += 1;
                        }
                    }
                    if i < n && chars[i] == ' ' {
                        i += 1;
                    }
                    if skip_depth.is_none() {
                        match word.as_str() {
                            "par" | "line" | "tab" | "cell" | "row" => out.push(' '),
                            "fonttbl" | "colortbl" | "stylesheet" | "info" | "pict" | "object"
                            | "header" | "footer" => skip_depth = Some(depth),
                            _ => {}
                        }
                    }
                } else if nc == '\'' {
                    i += 1;
                    let mut k = 0;
                    while k < 2 && i < n && chars[i].is_ascii_hexdigit() {
                        i += 1;
                        k += 1;
                    }
                } else {
                    i += 1;
                }
            }
            c => {
                if skip_depth.is_none() {
                    out.push(c);
                }
                i += 1;
            }
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn get_or_create_disk_key() -> Result<[u8; 32], String> {
    use tmuntaner_keyring::KeyringClient;
    let client = KeyringClient::new(KEYRING_USER, KEYRING_SERVICE, "clipygo")
        .map_err(|e| format!("keyring init: {e}"))?;
    if let Ok(Some(existing)) = client.get_password() {
        if existing.len() == 64 && existing.chars().all(|c| c.is_ascii_hexdigit()) {
            return hex_to_key(&existing);
        }
    }
    let bytes = random_key();
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    client
        .set_password(hex)
        .map_err(|e| format!("keyring set: {e}"))?;
    Ok(bytes)
}

fn hex_to_key(s: &str) -> Result<[u8; 32], String> {
    if s.len() != 64 {
        return Err("bad key length".into());
    }
    let mut out = [0u8; 32];
    for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
        let byte_str = std::str::from_utf8(chunk).map_err(|e| e.to_string())?;
        out[i] = u8::from_str_radix(byte_str, 16).map_err(|e| e.to_string())?;
    }
    Ok(out)
}

pub fn history_db_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("history")
        .join("index.db"))
}

pub fn build_coordinator(
    app: &AppHandle,
    persist_to_disk: bool,
    cap_bytes: u64,
) -> Result<HistoryCoordinator, String> {
    if persist_to_disk {
        let key = get_or_create_disk_key()?;
        let path = history_db_path(app)?;
        HistoryCoordinator::new_persisted(path, key, cap_bytes)
    } else {
        HistoryCoordinator::new_in_memory(cap_bytes)
    }
}

pub fn replace_coordinator(
    state: &Arc<Mutex<HistoryCoordinator>>,
    new_coord: HistoryCoordinator,
) -> Result<(), String> {
    let mut guard = state.lock().map_err(|_| "history lock poisoned")?;
    *guard = new_coord;
    Ok(())
}

pub fn notify_changed(app: &AppHandle) {
    let _ = app.emit("history-changed", ());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_roundtrip() {
        let k = random_key();
        let pt = b"hello world";
        let ct = encrypt(&k, pt).unwrap();
        assert_ne!(&ct[NONCE_LEN..], pt);
        let recovered = decrypt(&k, &ct).unwrap();
        assert_eq!(recovered, pt);
    }

    #[test]
    fn decrypt_with_wrong_key_fails() {
        let k1 = random_key();
        let k2 = random_key();
        let ct = encrypt(&k1, b"secret").unwrap();
        assert!(decrypt(&k2, &ct).is_err());
    }

    #[test]
    fn insert_text_then_list() {
        let mut h = HistoryCoordinator::new_in_memory(1024 * 1024).unwrap();
        let id = h.insert_text("hello".to_string(), None).unwrap();
        let list = h.list(&Filter::default(), 0, 10).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, id);
        assert_eq!(list[0].preview, "hello");
        assert_eq!(list[0].kind_tag, "text");
    }

    #[test]
    fn get_full_entry_roundtrip() {
        let mut h = HistoryCoordinator::new_in_memory(1024 * 1024).unwrap();
        let id = h.insert_text("payload".to_string(), None).unwrap();
        let entry = h.get_full_entry(id).unwrap().unwrap();
        assert_eq!(entry.kind, "text");
        assert_eq!(entry.text.as_deref(), Some("payload"));
    }

    #[test]
    fn insert_image_and_fetch_bytes() {
        let mut h = HistoryCoordinator::new_in_memory(10 * 1024 * 1024).unwrap();
        let bytes = vec![137, 80, 78, 71, 1, 2, 3, 4];
        let id = h
            .insert_image("image/png".into(), 16, 16, bytes.clone(), None)
            .unwrap();
        let fetched = h.get_image(id).unwrap();
        assert_eq!(fetched, bytes);
        let list = h.list(&Filter::default(), 0, 10).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].kind_tag, "image");
        assert_eq!(list[0].width, Some(16));
    }

    #[test]
    fn eviction_drops_oldest_under_cap() {
        let mut h = HistoryCoordinator::new_in_memory(20).unwrap();
        h.insert_text("aaaaaaaaaa".to_string(), None).unwrap();
        h.insert_text("bbbbbbbbbb".to_string(), None).unwrap();
        h.insert_text("cccccccccc".to_string(), None).unwrap();
        let stats = h.stats().unwrap();
        assert!(stats.bytes_used <= 20);
    }

    #[test]
    fn pinned_entries_survive_eviction() {
        let mut h = HistoryCoordinator::new_in_memory(15).unwrap();
        let pinned_id = h.insert_text("PIN1234567".to_string(), None).unwrap();
        h.set_pinned(pinned_id, true).unwrap();
        h.insert_text("xxxxxxxxxx".to_string(), None).unwrap();
        h.insert_text("yyyyyyyyyy".to_string(), None).unwrap();
        let list = h.list(&Filter::default(), 0, 10).unwrap();
        assert!(list.iter().any(|e| e.id == pinned_id));
    }

    #[test]
    fn filter_by_kind_and_query() {
        let mut h = HistoryCoordinator::new_in_memory(1024 * 1024).unwrap();
        h.insert_text("hello world".to_string(), None).unwrap();
        h.insert_text("goodbye world".to_string(), None).unwrap();
        h.insert_image("image/png".into(), 1, 1, vec![1, 2, 3], None)
            .unwrap();
        let only_text = h
            .list(
                &Filter {
                    kind: FilterKind::Text,
                    ..Default::default()
                },
                0,
                10,
            )
            .unwrap();
        assert_eq!(only_text.len(), 2);
        let only_image = h
            .list(
                &Filter {
                    kind: FilterKind::Image,
                    ..Default::default()
                },
                0,
                10,
            )
            .unwrap();
        assert_eq!(only_image.len(), 1);
        let search = h
            .list(
                &Filter {
                    query: "hello".into(),
                    ..Default::default()
                },
                0,
                10,
            )
            .unwrap();
        assert_eq!(search.len(), 1);
        assert_eq!(search[0].preview, "hello world");
    }

    #[test]
    fn build_preview_multiline_reports_line_count() {
        let (preview, lines) = build_preview("line one\nline two\nline three\nline four");
        assert_eq!(lines, 4);
        assert!(preview.starts_with("line one\nline two\nline three"));
        assert!(preview.ends_with('…'));
        let (single, n) = build_preview("just one");
        assert_eq!(n, 1);
        assert_eq!(single, "just one");
    }

    #[test]
    fn strip_html_yields_plain_text() {
        let html = "<p>Hello <b>world</b> &amp; <i>friends</i></p>";
        assert_eq!(strip_html(html), "Hello world & friends");
    }

    #[test]
    fn strip_rtf_skips_control_tables() {
        let rtf = r"{\rtf1\ansi{\fonttbl{\f0 Arial;}}\f0 Hello \b world\b0 .}";
        assert_eq!(strip_rtf(rtf), "Hello world.");
    }

    #[test]
    fn insert_html_lists_with_stripped_preview() {
        let mut h = HistoryCoordinator::new_in_memory(1024 * 1024).unwrap();
        let id = h
            .insert_html("<h1>Title</h1><p>Body text</p>".into(), None)
            .unwrap();
        let list = h.list(&Filter::default(), 0, 10).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].kind_tag, "html");
        assert_eq!(list[0].preview, "Title Body text");
        let entry = h.get_full_entry(id).unwrap().unwrap();
        assert_eq!(entry.kind, "html");
        assert_eq!(
            entry.html.as_deref(),
            Some("<h1>Title</h1><p>Body text</p>")
        );
    }

    #[test]
    fn insert_files_previews_basenames_and_counts() {
        let mut h = HistoryCoordinator::new_in_memory(1024 * 1024).unwrap();
        h.insert_files(
            vec![
                "C:\\Users\\me\\report.pdf".into(),
                "/home/me/photo.png".into(),
            ],
            None,
        )
        .unwrap();
        let list = h.list(&Filter::default(), 0, 10).unwrap();
        assert_eq!(list[0].kind_tag, "files");
        assert_eq!(list[0].line_count, 2);
        assert_eq!(list[0].preview, "report.pdf, photo.png");
    }

    #[test]
    fn filter_and_search_cover_rich_kinds() {
        let mut h = HistoryCoordinator::new_in_memory(1024 * 1024).unwrap();
        h.insert_html("<p>needle in html</p>".into(), None).unwrap();
        h.insert_rtf(r"{\rtf1 plain rtf text}".into(), None)
            .unwrap();
        h.insert_files(vec!["/tmp/doc.txt".into()], None).unwrap();

        let only_html = h
            .list(
                &Filter {
                    kind: FilterKind::Html,
                    ..Default::default()
                },
                0,
                10,
            )
            .unwrap();
        assert_eq!(only_html.len(), 1);

        let search = h
            .list(
                &Filter {
                    query: "needle".into(),
                    ..Default::default()
                },
                0,
                10,
            )
            .unwrap();
        assert_eq!(search.len(), 1);
        assert_eq!(search[0].kind_tag, "html");

        let file_search = h
            .list(
                &Filter {
                    query: "doc.txt".into(),
                    ..Default::default()
                },
                0,
                10,
            )
            .unwrap();
        assert_eq!(file_search.len(), 1);
        assert_eq!(file_search[0].kind_tag, "files");
    }

    #[test]
    fn legacy_db_without_formats_table_upgrades_and_reads() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE entries (
                 id BLOB PRIMARY KEY,
                 timestamp INTEGER NOT NULL,
                 kind TEXT NOT NULL,
                 content_ct BLOB,
                 mime TEXT,
                 width INTEGER,
                 height INTEGER,
                 size_bytes INTEGER NOT NULL,
                 matched_pattern TEXT,
                 pinned INTEGER NOT NULL DEFAULT 0,
                 last_sent_to TEXT
             );
             CREATE TABLE images (
                 id BLOB PRIMARY KEY REFERENCES entries(id) ON DELETE CASCADE,
                 bytes_ct BLOB NOT NULL
             );",
        )
        .unwrap();
        let key = random_key();
        let id = Uuid::new_v4();
        let html = "<b>legacy</b>";
        let ct = encrypt(&key, html.as_bytes()).unwrap();
        conn.execute(
            "INSERT INTO entries (id, timestamp, kind, content_ct, mime, size_bytes, pinned) \
             VALUES (?, ?, 'html', ?, 'text/html', ?, 0)",
            params![id.as_bytes().to_vec(), 1i64, ct, html.len() as i64],
        )
        .unwrap();

        init_schema(&conn).unwrap();
        let h = HistoryCoordinator {
            conn,
            persisted: false,
            cap_bytes: 1024 * 1024,
            key,
        };
        let list = h.list(&Filter::default(), 0, 10).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].kind_tag, "html");
        assert_eq!(list[0].preview, "legacy");
        let full = h.get_full_entry(id).unwrap().unwrap();
        assert_eq!(full.html.as_deref(), Some(html));
        assert!(full.text.is_none());
    }

    #[test]
    fn insert_captured_stores_all_formats() {
        let mut h = HistoryCoordinator::new_in_memory(1024 * 1024).unwrap();
        let id = h
            .insert_captured(
                CapturedContent {
                    html: Some("<b>hi</b>".into()),
                    rtf: Some(r"{\rtf1 hi}".into()),
                    text: Some("hi".into()),
                    ..Default::default()
                },
                None,
            )
            .unwrap();
        let list = h.list(&Filter::default(), 0, 10).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].kind_tag, "html");
        assert_eq!(list[0].formats, vec!["html", "rtf", "text"]);
        let full = h.get_full_entry(id).unwrap().unwrap();
        assert_eq!(full.kind, "html");
        assert_eq!(full.html.as_deref(), Some("<b>hi</b>"));
        assert_eq!(full.rtf.as_deref(), Some(r"{\rtf1 hi}"));
        assert_eq!(full.text.as_deref(), Some("hi"));
        let expected = ("<b>hi</b>".len() + r"{\rtf1 hi}".len() + "hi".len()) as u64;
        assert_eq!(h.stats().unwrap().bytes_used, expected);
    }

    #[test]
    fn insert_captured_image_with_secondary_formats() {
        let mut h = HistoryCoordinator::new_in_memory(10 * 1024 * 1024).unwrap();
        let bytes = vec![1u8, 2, 3, 4];
        let id = h
            .insert_captured(
                CapturedContent {
                    image: Some(CapturedImage {
                        mime: "image/png".into(),
                        width: 8,
                        height: 4,
                        bytes: bytes.clone(),
                    }),
                    html: Some("<img src=\"x\">".into()),
                    text: Some("x".into()),
                    ..Default::default()
                },
                None,
            )
            .unwrap();
        let list = h.list(&Filter::default(), 0, 10).unwrap();
        assert_eq!(list[0].kind_tag, "image");
        assert_eq!(list[0].width, Some(8));
        assert_eq!(list[0].formats, vec!["image", "html", "text"]);
        let full = h.get_full_entry(id).unwrap().unwrap();
        assert_eq!(full.kind, "image");
        assert_eq!(full.image.as_deref(), Some(bytes.as_slice()));
        assert_eq!(full.html.as_deref(), Some("<img src=\"x\">"));
        assert_eq!(full.text.as_deref(), Some("x"));
        assert_eq!(h.get_image(id).unwrap(), bytes);
    }

    #[test]
    fn get_full_entry_single_format_has_no_extras() {
        let mut h = HistoryCoordinator::new_in_memory(1024 * 1024).unwrap();
        let id = h.insert_text("plain".into(), None).unwrap();
        let list = h.list(&Filter::default(), 0, 10).unwrap();
        assert_eq!(list[0].formats, vec!["text"]);
        let full = h.get_full_entry(id).unwrap().unwrap();
        assert_eq!(full.kind, "text");
        assert_eq!(full.text.as_deref(), Some("plain"));
        assert!(full.html.is_none());
        assert!(full.rtf.is_none());
        assert!(full.image.is_none());
        assert!(full.files.is_none());
    }

    #[test]
    fn get_full_entry_files_split_lines() {
        let mut h = HistoryCoordinator::new_in_memory(1024 * 1024).unwrap();
        let id = h
            .insert_captured(
                CapturedContent {
                    files: Some(vec!["C:\\a.txt".into(), "C:\\b.txt".into()]),
                    text: Some("C:\\a.txt".into()),
                    ..Default::default()
                },
                None,
            )
            .unwrap();
        let full = h.get_full_entry(id).unwrap().unwrap();
        assert_eq!(full.kind, "files");
        assert_eq!(
            full.files,
            Some(vec!["C:\\a.txt".to_string(), "C:\\b.txt".to_string()])
        );
        assert_eq!(full.text.as_deref(), Some("C:\\a.txt"));
    }

    #[test]
    fn delete_removes_secondary_formats() {
        let mut h = HistoryCoordinator::new_in_memory(1024 * 1024).unwrap();
        let id = h
            .insert_captured(
                CapturedContent {
                    html: Some("<b>x</b>".into()),
                    text: Some("x".into()),
                    ..Default::default()
                },
                None,
            )
            .unwrap();
        h.delete(id).unwrap();
        assert!(h.get_full_entry(id).unwrap().is_none());
        let orphans: i64 = h
            .conn
            .query_row("SELECT COUNT(*) FROM formats", [], |r| r.get(0))
            .unwrap();
        assert_eq!(orphans, 0);
    }

    #[test]
    fn clear_respects_pin_flag() {
        let mut h = HistoryCoordinator::new_in_memory(1024 * 1024).unwrap();
        let p = h.insert_text("pinned".into(), None).unwrap();
        h.set_pinned(p, true).unwrap();
        h.insert_text("transient".into(), None).unwrap();
        h.clear(false).unwrap();
        let list = h.list(&Filter::default(), 0, 10).unwrap();
        assert_eq!(list.len(), 1);
        assert!(list[0].pinned);
        h.clear(true).unwrap();
        let list = h.list(&Filter::default(), 0, 10).unwrap();
        assert!(list.is_empty());
    }
}
