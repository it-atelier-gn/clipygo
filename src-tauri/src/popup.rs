use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::{debug_log, show_window_fresh};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Popup {
    pub label: &'static str,
    pub title: &'static str,
    pub width: f64,
    pub height: f64,
    pub resizable: bool,
}

pub const HISTORY: Popup = Popup {
    label: "history",
    title: "History - clipygo",
    width: 900.0,
    height: 640.0,
    resizable: true,
};

pub const EXEC_PICKER: Popup = Popup {
    label: "exec-picker",
    title: "Execute - clipygo",
    width: 560.0,
    height: 480.0,
    resizable: true,
};

pub const MORPH_PICKER: Popup = Popup {
    label: "morph-picker",
    title: "Morph - clipygo",
    width: 560.0,
    height: 640.0,
    resizable: true,
};

pub const SETTINGS: Popup = Popup {
    label: "settings",
    title: "Settings - clipygo",
    width: 1024.0,
    height: 768.0,
    resizable: true,
};

pub const DEBUG: Popup = Popup {
    label: "debug",
    title: "Debug Log - clipygo",
    width: 900.0,
    height: 600.0,
    resizable: true,
};

pub const ABOUT: Popup = Popup {
    label: "about",
    title: "About - clipygo",
    width: 360.0,
    height: 360.0,
    resizable: false,
};

pub fn open(app: &AppHandle, popup: Popup) {
    if let Some(window) = app.get_webview_window(popup.label) {
        show_window_fresh(&window);
        return;
    }
    match WebviewWindowBuilder::new(app, popup.label, WebviewUrl::App(popup.label.into()))
        .title(popup.title)
        .devtools(true)
        .inner_size(popup.width, popup.height)
        .resizable(popup.resizable)
        .decorations(false)
        .center()
        .visible(false)
        .build()
    {
        Ok(window) => {
            let clone = window.clone();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = clone.hide();
                }
            });
            show_window_fresh(&window);
        }
        Err(e) => debug_log(
            app,
            "app",
            "error",
            format!("Failed to create {} window: {e}", popup.label),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Popup; 6] = [HISTORY, EXEC_PICKER, MORPH_PICKER, SETTINGS, DEBUG, ABOUT];

    #[test]
    fn labels_are_unique() {
        for (i, a) in ALL.iter().enumerate() {
            for b in &ALL[i + 1..] {
                assert_ne!(a.label, b.label);
            }
        }
    }

    #[test]
    fn labels_match_existing_routes() {
        let routes = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/routes");
        for popup in ALL {
            assert!(
                routes.join(popup.label).join("+page.svelte").is_file(),
                "missing route for {}",
                popup.label
            );
        }
    }

    #[test]
    fn titles_name_the_app() {
        for popup in ALL {
            assert!(popup.title.ends_with(" - clipygo"), "{}", popup.title);
        }
    }
}
