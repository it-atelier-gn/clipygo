use std::sync::{Arc, Mutex};

use base64::Engine;
use serde::Serialize;
use tauri::{AppHandle, State};
use uuid::Uuid;

use crate::history::{Filter, HistoryCoordinator, HistoryEntryView, Stats};

#[derive(Serialize)]
pub struct ResendPayload {
    pub kind: String,
    pub text: Option<String>,
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
pub fn history_stats(
    coord: State<'_, Arc<Mutex<HistoryCoordinator>>>,
) -> Result<Stats, String> {
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
    if let Some(text) = guard.get_text(id)? {
        return Ok(ResendPayload {
            kind: "text".into(),
            text: Some(text),
            image_base64: None,
            mime: None,
        });
    }
    let list = guard.list(
        &Filter {
            query: String::new(),
            kind: crate::history::FilterKind::Image,
            pinned_only: false,
        },
        0,
        u32::MAX,
    )?;
    let entry = list
        .iter()
        .find(|e| e.id == id)
        .ok_or_else(|| "entry not found".to_string())?;
    let bytes = guard.get_image(id)?;
    Ok(ResendPayload {
        kind: "image".into(),
        text: None,
        image_base64: Some(base64::engine::general_purpose::STANDARD.encode(bytes)),
        mime: entry.mime.clone(),
    })
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
