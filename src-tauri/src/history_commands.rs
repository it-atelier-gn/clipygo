use std::sync::{Arc, Mutex};

use base64::Engine;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use uuid::Uuid;

use crate::history::{Filter, FullEntry, HistoryCoordinator, HistoryEntryView, Stats};

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ResendPayload {
    pub kind: String,
    pub text: Option<String>,
    pub html: Option<String>,
    pub rtf: Option<String>,
    pub files: Option<Vec<String>>,
    pub image_base64: Option<String>,
    pub mime: Option<String>,
}

pub fn payload_from_full_entry(full: FullEntry) -> ResendPayload {
    let mut payload = ResendPayload {
        kind: full.kind,
        text: full.text,
        html: full.html,
        rtf: full.rtf,
        files: full.files,
        image_base64: full
            .image
            .map(|b| base64::engine::general_purpose::STANDARD.encode(b)),
        mime: full.mime,
    };
    if payload.text.is_none() {
        if let Some(html) = &payload.html {
            payload.text = Some(crate::history::html_to_text(html));
        } else if let Some(rtf) = &payload.rtf {
            payload.text = Some(crate::history::rtf_to_text(rtf));
        }
    }
    payload
}

#[tauri::command]
pub fn history_list(
    coord: State<'_, Arc<Mutex<HistoryCoordinator>>>,
    filter: Filter,
    offset: u32,
    limit: u32,
) -> Result<Vec<HistoryEntryView>, String> {
    let guard = coord.lock().map_err(|_| "history lock poisoned")?;
    guard.list(&filter, offset, limit)
}

#[tauri::command]
pub fn history_stats(coord: State<'_, Arc<Mutex<HistoryCoordinator>>>) -> Result<Stats, String> {
    let guard = coord.lock().map_err(|_| "history lock poisoned")?;
    guard.stats()
}

#[tauri::command]
pub fn history_get_image(
    coord: State<'_, Arc<Mutex<HistoryCoordinator>>>,
    id: Uuid,
) -> Result<String, String> {
    let guard = coord.lock().map_err(|_| "history lock poisoned")?;
    let bytes = guard.get_image(id)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

#[tauri::command]
pub fn history_resend(
    coord: State<'_, Arc<Mutex<HistoryCoordinator>>>,
    id: Uuid,
) -> Result<ResendPayload, String> {
    let guard = coord.lock().map_err(|_| "history lock poisoned")?;
    let full = guard
        .get_full_entry(id)?
        .ok_or_else(|| "entry not found".to_string())?;
    Ok(payload_from_full_entry(full))
}

#[tauri::command]
pub fn history_pin(
    app: AppHandle,
    coord: State<'_, Arc<Mutex<HistoryCoordinator>>>,
    id: Uuid,
    pinned: bool,
) -> Result<(), String> {
    {
        let guard = coord.lock().map_err(|_| "history lock poisoned")?;
        guard.set_pinned(id, pinned)?;
    }
    crate::history::notify_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn history_set_last_sent_to(
    app: AppHandle,
    coord: State<'_, Arc<Mutex<HistoryCoordinator>>>,
    id: Uuid,
    target: String,
) -> Result<(), String> {
    {
        let guard = coord.lock().map_err(|_| "history lock poisoned")?;
        guard.set_last_sent_to(id, &target)?;
    }
    crate::history::notify_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn history_delete(
    app: AppHandle,
    coord: State<'_, Arc<Mutex<HistoryCoordinator>>>,
    id: Uuid,
) -> Result<(), String> {
    {
        let guard = coord.lock().map_err(|_| "history lock poisoned")?;
        guard.delete(id)?;
    }
    crate::history::notify_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn history_clear(
    app: AppHandle,
    coord: State<'_, Arc<Mutex<HistoryCoordinator>>>,
    include_pinned: bool,
) -> Result<(), String> {
    {
        let guard = coord.lock().map_err(|_| "history lock poisoned")?;
        guard.clear(include_pinned)?;
    }
    crate::history::notify_changed(&app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_keeps_stored_text() {
        let payload = payload_from_full_entry(FullEntry {
            kind: "html".into(),
            html: Some("<b>rich</b>".into()),
            text: Some("stored".into()),
            ..Default::default()
        });
        assert_eq!(payload.text.as_deref(), Some("stored"));
        assert_eq!(payload.html.as_deref(), Some("<b>rich</b>"));
    }

    #[test]
    fn payload_derives_text_from_html() {
        let payload = payload_from_full_entry(FullEntry {
            kind: "html".into(),
            html: Some("<p>Hello <b>world</b></p>".into()),
            ..Default::default()
        });
        assert_eq!(payload.text.as_deref(), Some("Hello world"));
    }

    #[test]
    fn payload_derives_text_from_rtf() {
        let payload = payload_from_full_entry(FullEntry {
            kind: "rtf".into(),
            rtf: Some(r"{\rtf1\ansi Hello \b world\b0 .}".into()),
            ..Default::default()
        });
        assert_eq!(payload.text.as_deref(), Some("Hello world."));
        assert!(payload.rtf.is_some());
    }

    #[test]
    fn payload_encodes_image_base64() {
        let payload = payload_from_full_entry(FullEntry {
            kind: "image".into(),
            mime: Some("image/png".into()),
            image: Some(vec![1, 2, 3]),
            ..Default::default()
        });
        assert_eq!(payload.kind, "image");
        assert_eq!(payload.image_base64.as_deref(), Some("AQID"));
        assert_eq!(payload.mime.as_deref(), Some("image/png"));
    }
}
