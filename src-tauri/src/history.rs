use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

const KEYRING_SERVICE: &str = "clipygo";
const KEYRING_USER: &str = "history-content-key";
const MAX_TEXT_LEN_FOR_PREVIEW: usize = 200;
const NONCE_LEN: usize = 24;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HistoryKind {
    Text { content: String },
    Image { mime: String, width: u32, height: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntryView {
    pub id: Uuid,
    pub timestamp: i64,
    pub kind_tag: String,
    pub preview: String,
    pub mime: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub size_bytes: u64,
    pub matched_pattern: Option<String>,
    pub pinned: bool,
    pub last_sent_to: Option<String>,
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

    pub fn insert_text(
        &mut self,
        content: String,
        matched_pattern: Option<String>,
    ) -> Result<Uuid, String> {
        let id = Uuid::new_v4();
        let ts = now_ms();
        let plain_bytes = content.as_bytes();
        let size = plain_bytes.len() as u64;
        let ciphertext = encrypt(&self.key, plain_bytes)?;
        self.conn
            .execute(
                "INSERT INTO entries (id, timestamp, kind, content_ct, size_bytes, matched_pattern, pinned) \
                 VALUES (?, ?, 'text', ?, ?, ?, 0)",
                params![
                    id.as_bytes().to_vec(),
                    ts,
                    ciphertext,
                    size as i64,
                    matched_pattern
                ],
            )
            .map_err(|e| e.to_string())?;
        self.evict_until_under_cap()?;
        Ok(id)
    }

    pub fn insert_image(
        &mut self,
        mime: String,
        width: u32,
        height: u32,
        bytes: Vec<u8>,
        matched_pattern: Option<String>,
    ) -> Result<Uuid, String> {
        let id = Uuid::new_v4();
        let ts = now_ms();
        let size = bytes.len() as u64;
        let ciphertext = encrypt(&self.key, &bytes)?;
        let tx = self.conn.transaction().map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO entries (id, timestamp, kind, mime, width, height, size_bytes, matched_pattern, pinned) \
             VALUES (?, ?, 'image', ?, ?, ?, ?, ?, 0)",
            params![id.as_bytes().to_vec(), ts, mime, width, height, size as i64, matched_pattern],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO images (id, bytes_ct) VALUES (?, ?)",
            params![id.as_bytes().to_vec(), ciphertext],
        )
        .map_err(|e| e.to_string())?;
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
            "SELECT id, timestamp, kind, content_ct, mime, width, height, size_bytes, matched_pattern, pinned, last_sent_to \
             FROM entries WHERE 1=1",
        );
        match filter.kind {
            FilterKind::Text => sql.push_str(" AND kind = 'text'"),
            FilterKind::Image => sql.push_str(" AND kind = 'image'"),
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
            ) = row.map_err(|e| e.to_string())?;

            let mut preview = String::new();
            if kind == "text" {
                if let Some(ct) = &content_ct {
                    if let Ok(plain) = decrypt(&self.key, ct) {
                        if let Ok(s) = String::from_utf8(plain) {
                            preview = truncate_preview(&s);
                            if !query.is_empty() && !s.to_lowercase().contains(&query) {
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
            views.push(HistoryEntryView {
                id,
                timestamp,
                kind_tag: kind,
                preview,
                mime,
                width: width.map(|v| v as u32),
                height: height.map(|v| v as u32),
                size_bytes: size_bytes as u64,
                matched_pattern,
                pinned: pinned != 0,
                last_sent_to,
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

    pub fn get_text(&self, id: Uuid) -> Result<Option<String>, String> {
        let ct: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT content_ct FROM entries WHERE id = ? AND kind = 'text'",
                params![id.as_bytes().to_vec()],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .flatten();
        match ct {
            None => Ok(None),
            Some(c) => {
                let plain = decrypt(&self.key, &c)?;
                String::from_utf8(plain).map(Some).map_err(|e| e.to_string())
            }
        }
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
                .query_row(
                    "SELECT COALESCE(SUM(size_bytes),0) FROM entries",
                    [],
                    |r| r.get(0),
                )
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
         CREATE INDEX IF NOT EXISTS idx_entries_ts ON entries(timestamp);
         CREATE INDEX IF NOT EXISTS idx_entries_pinned ON entries(pinned);",
    )
    .map_err(|e| e.to_string())
}

fn encrypt(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, String> {
    use rand::RngCore;
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key));
    let mut nonce = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce);
    let ct = cipher
        .encrypt(XNonce::from_slice(&nonce), plaintext)
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
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key));
    cipher
        .decrypt(XNonce::from_slice(nonce), ct)
        .map_err(|e| format!("decrypt: {e}"))
}

fn random_key() -> [u8; 32] {
    use rand::RngCore;
    let mut k = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut k);
    k
}

fn truncate_preview(s: &str) -> String {
    let one_line: String = s.lines().next().unwrap_or("").to_string();
    if one_line.chars().count() <= MAX_TEXT_LEN_FOR_PREVIEW {
        one_line
    } else {
        one_line
            .chars()
            .take(MAX_TEXT_LEN_FOR_PREVIEW)
            .collect::<String>()
            + "…"
    }
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
    fn get_text_roundtrip() {
        let mut h = HistoryCoordinator::new_in_memory(1024 * 1024).unwrap();
        let id = h.insert_text("payload".to_string(), None).unwrap();
        assert_eq!(h.get_text(id).unwrap().as_deref(), Some("payload"));
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
