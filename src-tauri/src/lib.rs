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
            targets::get_targets,
            targets::send_to_target
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
            )));
            app.manage(target_coordinator.clone());

            // Shared patterns — updated on settings change, read by the single clipboard listener
            let shared_patterns: Arc<Mutex<Vec<Regex>>> =
                Arc::new(Mutex::new(compile_patterns(&initial_settings.regex_list)));

            setup_shortcut(app.handle(), &initial_settings);

            // Register the clipboard listener exactly once
            start_clipboard_pattern_monitor(app.handle(), shared_patterns.clone());

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
