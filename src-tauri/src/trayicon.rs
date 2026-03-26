use tauri::Manager;

pub fn setup(app: &mut tauri::App) {
    use tauri::{
        image::Image,
        menu::{MenuBuilder, MenuItem},
        tray::TrayIconBuilder,
        WebviewUrl, WebviewWindowBuilder,
    };

    // Create menu items
    let show_i = MenuItem::with_id(app, "show", "Show", true, None::<&str>).unwrap();
    let settings_i = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>).unwrap();
    let about_i = MenuItem::with_id(app, "about", "About", true, None::<&str>).unwrap();
    let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>).unwrap();

    // Build menu
    let menu = MenuBuilder::new(app)
        .item(&show_i)
        .item(&settings_i)
        .separator()
        .item(&about_i)
        .item(&quit_i)
        .build()
        .unwrap();

    // Create tray icon
    let _tray = TrayIconBuilder::with_id("tray")
        .tooltip("clipygo")
        .icon(Image::from_bytes(include_bytes!("../icons/32x32.png")).expect("Failed to load icon"))
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    window.show().unwrap();
                    window.set_focus().unwrap();
                }
            }
            "settings" => {
                // Try to get existing window first
                if let Some(window) = app.get_webview_window("settings") {
                    window.show().unwrap();
                    window.set_focus().unwrap();
                } else {
                    // Create window if it doesn't exist
                    let settings_window = WebviewWindowBuilder::new(
                        app,
                        "settings",
                        WebviewUrl::App("settings".into()),
                    )
                    .title("Settings - clipygo")
                    .devtools(true)
                    .inner_size(1024.0, 768.0)
                    .decorations(false)
                    .center()
                    .build()
                    .unwrap();

                    // Clone for the closure
                    let settings_window_clone = settings_window.clone();
                    settings_window.on_window_event(move |event| {
                        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                            api.prevent_close();
                            settings_window_clone.hide().unwrap();
                        }
                    });
                }
            }
            "about" => {
                if let Some(window) = app.get_webview_window("about") {
                    window.show().unwrap();
                    window.set_focus().unwrap();
                } else {
                    let about_window = WebviewWindowBuilder::new(
                        app,
                        "about",
                        WebviewUrl::App("about".into()),
                    )
                    .title("About - clipygo")
                    .inner_size(360.0, 360.0)
                    .resizable(false)
                    .decorations(false)
                    .devtools(true)
                    .center()
                    .build()
                    .unwrap();

                    let about_window_clone = about_window.clone();
                    about_window.on_window_event(move |event| {
                        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                            api.prevent_close();
                            about_window_clone.hide().unwrap();
                        }
                    });
                }
            }
            "quit" => app.exit(0),
            _ => println!("Unhandled menu item: {:?}", event.id),
        })
        .build(app)
        .unwrap();
}
