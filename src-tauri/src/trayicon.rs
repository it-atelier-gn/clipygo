use tauri::Manager;

use crate::debug_log;
use crate::settings::SettingsCoordinator;

pub fn setup(app: &mut tauri::App) {
    use tauri::{
        image::Image,
        menu::{MenuBuilder, MenuItem},
        tray::TrayIconBuilder,
    };

    let show_debug = SettingsCoordinator::new(app)
        .map(|s| s.get_settings().show_debug_log)
        .unwrap_or(true);

    let show_i = MenuItem::with_id(app, "show", "Show", true, None::<&str>).unwrap();
    let history_i = MenuItem::with_id(app, "history", "History", true, None::<&str>).unwrap();
    let settings_i = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>).unwrap();
    let debug_i = MenuItem::with_id(app, "debug", "Debug Log", true, None::<&str>).unwrap();
    let about_i = MenuItem::with_id(app, "about", "About", true, None::<&str>).unwrap();
    let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>).unwrap();

    let mut menu_builder = MenuBuilder::new(app)
        .item(&show_i)
        .item(&history_i)
        .item(&settings_i);
    if show_debug {
        menu_builder = menu_builder.item(&debug_i);
    }
    let menu = menu_builder
        .separator()
        .item(&about_i)
        .item(&quit_i)
        .build()
        .unwrap();

    let _tray = TrayIconBuilder::with_id("tray")
        .tooltip("clipygo")
        .icon(Image::from_bytes(include_bytes!("../icons/32x32.png")).expect("Failed to load icon"))
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    crate::show_window_fresh(&window);
                }
            }
            "settings" => crate::popup::open(app, crate::popup::SETTINGS),
            "history" => crate::popup::open(app, crate::popup::HISTORY),
            "debug" => crate::popup::open(app, crate::popup::DEBUG),
            "about" => crate::popup::open(app, crate::popup::ABOUT),
            "quit" => app.exit(0),
            _ => debug_log(
                app,
                "app",
                "warn",
                format!("Unhandled menu item: {:?}", event.id),
            ),
        })
        .build(app)
        .unwrap();
}
