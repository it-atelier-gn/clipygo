mod settings;
mod target_providers;
mod targets;
mod trayicon;

use std::str::FromStr;
use std::sync::Arc;
use std::sync::Mutex;

use regex::Regex;

use settings::{AppSettings, SettingsCoordinator};
use tauri::{AppHandle, Listener, Manager};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use crate::targets::TargetProviderCoordinator;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::default(),
            None,
        ))
        .plugin(tauri_plugin_clipboard::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            settings::get_settings,
            settings::save_settings,
            settings::reset_settings,
            settings::add_plugin,
            settings::update_plugin,
            settings::remove_plugin,
            settings::toggle_plugin,
            settings::check_plugin_path,
            settings::fetch_registry,
            settings::install_registry_plugin,
            settings::update_registry_plugin,
            targets::get_targets,
            targets::send_to_target,
            targets::get_plugin_config_schema,
            targets::set_plugin_config,
            targets::get_plugin_link,
            targets::get_plugin_statuses
        ])
        .setup(|app| {
            trayicon::setup(app);

            if let Some(window) = app.get_webview_window("main") {
                let window_clone = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        // Prevent the default close behavior
                        api.prevent_close();
                        // Hide the window instead
                        window_clone.hide().unwrap();
                    }
                });
            }

            if let Some(settings_window) = app.get_webview_window("settings") {
                let settings_window_clone = settings_window.clone();
                settings_window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        // Prevent the default close behavior
                        api.prevent_close();
                        // Hide the window instead
                        settings_window_clone.hide().unwrap();
                    }
                });
            }

            start_clipboard_monitor(app.handle());

            let settings_coordinator = SettingsCoordinator::new(app)?;
            let initial_settings = settings_coordinator.get_settings().clone();
            let target_coordinator = Arc::new(Mutex::new(TargetProviderCoordinator::new(
                initial_settings.clone(),
                app.handle().clone(),
            )));
            app.manage(target_coordinator.clone());

            // Shared patterns — updated on settings change, read by the single clipboard listener
            let shared_patterns: Arc<Mutex<Vec<Regex>>> =
                Arc::new(Mutex::new(compile_patterns(&initial_settings.regex_list)));

            setup_shortcut(app.handle(), &initial_settings);

            // Register the clipboard listener exactly once
            start_clipboard_pattern_monitor(app.handle(), shared_patterns.clone());

            // Listen for plugin events and show notification window for incoming messages
            let app_handle_events = app.handle().clone();
            app.listen("plugin-event", move |event| {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(event.payload()) {
                    if value.get("event").and_then(|e| e.as_str()) == Some("incoming_message") {
                        show_notification_window(&app_handle_events);
                    }
                }
            });

            // On settings change: update shortcut, patterns, and provider coordinator
            let app_handle_listener = app.handle().clone();
            let target_coordinator_listener = target_coordinator.clone();
            let shared_patterns_listener = shared_patterns.clone();
            app.listen("settings-changed", move |_| {
                if let Ok(sc) = SettingsCoordinator::from_handle(&app_handle_listener) {
                    let settings = sc.get_settings().clone();
                    println!("Settings changed — reloading providers");
                    setup_shortcut(&app_handle_listener, &settings);
                    if let Ok(mut p) = shared_patterns_listener.lock() {
                        *p = compile_patterns(&settings.regex_list);
                        println!("Clipboard patterns updated: {} patterns", p.len());
                    }
                    if let Ok(mut coord) = target_coordinator_listener.lock() {
                        coord.reload_providers(&settings);
                    }
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

pub fn setup_shortcut(app: &AppHandle, settings: &AppSettings) {
    let shortcut = match tauri_plugin_global_shortcut::Shortcut::from_str(&settings.global_shortcut)
    {
        Ok(shortcut) => shortcut,
        Err(e) => {
            println!("Unsupported key combination: {e}");
            return;
        }
    };

    if !app.global_shortcut().is_registered(shortcut) {
        app.global_shortcut().unregister_all().unwrap();
        println!("Registering shortcut: {shortcut:?}");
        app.global_shortcut()
            .on_shortcut(shortcut, on_shortcut)
            .unwrap();
    } else {
        println!("Shortcut already registered: {shortcut:?}");
    }
}

pub fn on_shortcut(
    app: &AppHandle,
    shortcut: &tauri_plugin_global_shortcut::Shortcut,
    _event: tauri_plugin_global_shortcut::ShortcutEvent,
) {
    println!("Shortcut pressed: {shortcut:?}");
    if let Some(window) = app.get_webview_window("main") {
        window.show().unwrap();
        window.set_focus().unwrap();
    }
}

fn start_clipboard_monitor(app: &AppHandle) {
    let clipboard = app.state::<tauri_plugin_clipboard::Clipboard>();
    if let Err(e) = clipboard.start_monitor(app.clone()) {
        println!("Failed to start clipboard monitor: {e}");
    } else {
        println!("Clipboard monitor started successfully");
    }
}

fn compile_patterns(regex_list: &[String]) -> Vec<Regex> {
    let patterns: Vec<Regex> = regex_list
        .iter()
        .filter_map(|pattern| {
            Regex::new(pattern)
                .map_err(|e| {
                    println!("Invalid regex pattern '{pattern}': {e}");
                })
                .ok()
        })
        .collect();
    println!("Compiled {} clipboard patterns", patterns.len());
    patterns
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compile_patterns_valid() {
        let patterns = compile_patterns(&[r"https://meet\.google\.com/\w+".to_string()]);
        assert_eq!(patterns.len(), 1);
    }

    #[test]
    fn compile_patterns_invalid_skipped() {
        let patterns = compile_patterns(&["[invalid".to_string()]);
        assert_eq!(patterns.len(), 0);
    }

    #[test]
    fn compile_patterns_mixed() {
        let patterns = compile_patterns(&[
            r"valid\d+".to_string(),
            "[invalid".to_string(),
            r"also_valid".to_string(),
        ]);
        assert_eq!(patterns.len(), 2);
    }

    #[test]
    fn compile_patterns_empty_list() {
        assert_eq!(compile_patterns(&[]).len(), 0);
    }

    #[test]
    fn compile_patterns_default_regex_list_all_valid() {
        let defaults = settings::AppSettings::default();
        let patterns = compile_patterns(&defaults.regex_list);
        assert_eq!(patterns.len(), defaults.regex_list.len());
    }

    #[test]
    fn compile_patterns_matches_expected() {
        let patterns =
            compile_patterns(
                &[r"https://meet\.google\.com/[a-z]{3}-[a-z]{4}-[a-z]{3}".to_string()],
            );
        assert!(patterns[0].is_match("https://meet.google.com/abc-defg-hij"));
        assert!(!patterns[0].is_match("https://zoom.us/j/123456"));
    }

    // --- Default regex patterns ---

    fn default_patterns() -> Vec<regex::Regex> {
        compile_patterns(&settings::AppSettings::default().regex_list)
    }

    fn matches_any(text: &str) -> bool {
        default_patterns().iter().any(|p| p.is_match(text))
    }

    #[test]
    fn default_pattern_jetbrains_code_with_me_matches() {
        assert!(matches_any("https://code-with-me.jetbrains.com/abc123-XYZ"));
    }

    #[test]
    fn default_pattern_zoom_matches() {
        assert!(matches_any("https://mycompany.zoom.us/j/98765432100"));
    }

    #[test]
    fn default_pattern_google_meet_matches() {
        assert!(matches_any("https://meet.google.com/abc-defg-hij"));
    }

    #[test]
    fn default_pattern_teams_matches() {
        assert!(matches_any(
            "https://teams.microsoft.com/l/meetup-join/19%3Ameeting_abc%40thread.v2/0?context=%7B%7D"
        ));
    }

    #[test]
    fn default_patterns_reject_plain_url() {
        assert!(!matches_any("https://example.com"));
    }

    #[test]
    fn default_patterns_reject_empty_string() {
        assert!(!matches_any(""));
    }

    #[test]
    fn default_pattern_google_meet_rejects_wrong_format() {
        // slug must be xxx-xxxx-xxx (3-4-3 lowercase)
        assert!(!matches_any("https://meet.google.com/toolong-slug"));
    }
}

/// Shows the notification window, creating it if it doesn't exist.
fn show_notification_window(app: &AppHandle) {
    use tauri::{WebviewUrl, WebviewWindowBuilder};

    if let Some(window) = app.get_webview_window("notification") {
        let _ = window.show();
        return;
    }

    // Create notification window in bottom-right corner
    match WebviewWindowBuilder::new(app, "notification", WebviewUrl::App("notification".into()))
        .title("clipygo — notification")
        .inner_size(360.0, 300.0)
        .decorations(false)
        .always_on_top(true)
        .focused(false)
        .visible(true)
        .devtools(true)
        .build()
    {
        Ok(window) => {
            // Position in bottom-right
            if let Ok(Some(monitor)) = window.current_monitor() {
                let screen = monitor.size();
                let scale = monitor.scale_factor();
                let x = (screen.width as f64 / scale) - 370.0;
                let y = (screen.height as f64 / scale) - 310.0;
                let _ = window
                    .set_position(tauri::Position::Logical(tauri::LogicalPosition::new(x, y)));
            }

            let window_clone = window.clone();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window_clone.hide();
                }
            });
        }
        Err(e) => println!("Failed to create notification window: {e}"),
    }
}

/// Registers the clipboard update listener exactly once.
/// Patterns are read from `shared_patterns` on every event, so updates take effect immediately.
fn start_clipboard_pattern_monitor(app: &AppHandle, shared_patterns: Arc<Mutex<Vec<Regex>>>) {
    let app_handle = app.clone();
    app.listen(
        "plugin:clipboard://clipboard-monitor/update",
        move |_event| {
            let clipboard = app_handle.state::<tauri_plugin_clipboard::Clipboard>();
            if let Ok(text) = clipboard.read_text() {
                let patterns = match shared_patterns.lock() {
                    Ok(p) => p,
                    Err(_) => return,
                };
                for pattern in patterns.iter() {
                    if pattern.is_match(&text) {
                        println!("Clipboard pattern matched — showing window");
                        if let Some(window) = app_handle.get_webview_window("main") {
                            window.show().unwrap();
                            window.set_focus().unwrap();
                        }
                        break;
                    }
                }
            }
        },
    );
}
