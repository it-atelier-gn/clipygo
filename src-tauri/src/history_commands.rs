use std::sync::{Arc, Mutex};

use base64::Engine;
use serde::Serialize;
use tauri::{AppHandle, State};
use uuid::Uuid;

use crate::history::{Filter, HistoryCoordinator, HistoryEntryView, Stats};

#[derive(Serialize, Default)]
pub struct ResendPayload {
    pub kind: String,
    pub text: Option<String>,
    pub html: Option<String>,
    pub rtf: Option<String>,
    pub files: Option<Vec<String>>,
    pub image_base64: Option<String>,
    pub mime: Option<String>,
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
    let entry = guard
        .get_entry(id)?
        .ok_or_else(|| "entry not found".to_string())?;
    match entry.kind.as_str() {
        "image" => {
            let bytes = guard.get_image(id)?;
            Ok(ResendPayload {
                kind: "image".into(),
                image_base64: Some(base64::engine::general_purpose::STANDARD.encode(bytes)),
                mime: entry.mime,
                ..Default::default()
            })
        }
        "html" => {
            let html = entry.text.unwrap_or_default();
            Ok(ResendPayload {
                kind: "html".into(),
                text: Some(crate::history::html_to_text(&html)),
                html: Some(html),
                mime: entry.mime,
                ..Default::default()
            })
        }
        "rtf" => Ok(ResendPayload {
            kind: "rtf".into(),
            rtf: entry.text,
            mime: entry.mime,
            ..Default::default()
        }),
        "files" => {
            let files = entry
                .text
                .unwrap_or_default()
                .lines()
                .filter(|l| !l.is_empty())
                .map(|l| l.to_string())
                .collect();
            Ok(ResendPayload {
                kind: "files".into(),
                files: Some(files),
                ..Default::default()
            })
        }
        _ => Ok(ResendPayload {
            kind: "text".into(),
            text: entry.text,
            ..Default::default()
        }),
    }
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
