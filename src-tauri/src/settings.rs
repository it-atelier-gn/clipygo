use std::sync::Arc;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use tauri::{App, AppHandle, Emitter, Manager};
use tauri_plugin_store::{Store, StoreExt};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MsTeamsSettings {
    pub enabled: bool,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginProvider {
    pub id: String,
    pub name: String,
    pub command: String, // full command line e.g. "node plugin.js" or "C:\plugins\demo.exe"
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetProviderSettings {
    pub msteams: MsTeamsSettings,
    pub plugins: Vec<PluginProvider>,
}

impl Default for TargetProviderSettings {
    fn default() -> Self {
        Self {
            msteams: MsTeamsSettings::default(),
            plugins: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub autostart: bool,
    pub global_shortcut: String,
    pub regex_list: Vec<String>,
    pub target_providers: TargetProviderSettings,
    #[serde(default = "default_registry_url")]
    pub registry_url: String,
}

fn default_registry_url() -> String {
    DEFAULT_REGISTRY_URL.to_string()
}

const DEFAULT_REGISTRY_URL: &str =
    "https://raw.githubusercontent.com/it-atelier-gn/clipygo-plugins/main/registry.json";

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            autostart: true,
            global_shortcut: "Ctrl+F10".to_string(),
            regex_list: vec![
                // JetBrains Code With Me
                r"https://code-with-me\.jetbrains\.com/[a-zA-Z0-9\-_]+".to_string(),
                // Zoom meeting links
                r"https://[a-z0-9\-]+\.zoom\.us/j/[0-9]+".to_string(),
                // Google Meet
                r"https://meet\.google\.com/[a-z]{3}-[a-z]{4}-[a-z]{3}".to_string(),
                // Microsoft Teams meeting links
                r"https://teams\.microsoft\.com/l/meetup-join/[^\s]+".to_string(),

            ],
            target_providers: TargetProviderSettings::default(),
            registry_url: DEFAULT_REGISTRY_URL.to_string(),
        }
    }
}

pub struct SettingsCoordinator {
    store: Arc<Store<tauri::Wry>>,
    settings: AppSettings,
}

impl SettingsCoordinator {
    const SETTINGS_KEY: &'static str = "app_settings";

    /// Create a new SettingsManager instance
    pub fn new(app: &App) -> Result<Self, Box<dyn std::error::Error>> {
        let store = app.store("config.json")?;
        let mut manager = Self {
            store,
            settings: AppSettings::default(),
        };

        // Load existing settings or create default ones
        manager.load_settings()?;
        Ok(manager)
    }

    pub fn from_handle(app_handle: &AppHandle) -> Result<Self, Box<dyn std::error::Error>> {
        let store = app_handle.store("config.json")?;
        let mut manager = Self {
            store,
            settings: AppSettings::default(),
        };

        manager.load_settings()?;
        Ok(manager)
    }

    pub fn get_settings(&self) -> &AppSettings {
        &self.settings
    }

    pub fn update_settings(
        &mut self,
        new_settings: AppSettings,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.settings = new_settings;
        self.save_settings()
    }

    pub fn reset_to_defaults(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.settings = AppSettings::default();
        self.save_settings()
    }

    fn load_settings(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(stored_settings) = self.store.get(Self::SETTINGS_KEY) {
            match serde_json::from_value::<AppSettings>(stored_settings.clone()) {
                Ok(settings) => {
                    self.settings = settings;
                    println!("Settings loaded successfully");
                }
                Err(e) => {
                    println!("Failed to deserialize settings, using defaults: {}", e);
                    self.settings = AppSettings::default();
                    self.save_settings()?; // Save default settings
                }
            }
        } else {
            println!("No settings found, using defaults");
            self.settings = AppSettings::default();
            self.save_settings()?; // Save default settings
        }
        Ok(())
    }

    fn save_settings(&self) -> Result<(), Box<dyn std::error::Error>> {
        let settings_value = serde_json::to_value(&self.settings)?;
        self.store.set(Self::SETTINGS_KEY, settings_value);
        self.store.save()?;
        println!("Settings saved successfully");
        Ok(())
    }
}

#[tauri::command]
pub fn get_settings(app_handle: AppHandle) -> Result<AppSettings, String> {
    let manager = SettingsCoordinator::from_handle(&app_handle)
        .map_err(|e| format!("Failed to create settings manager: {}", e))?;
    Ok(manager.get_settings().clone())
}

#[tauri::command]
pub fn save_settings(app_handle: AppHandle, settings: AppSettings) -> Result<(), String> {
    let mut manager = SettingsCoordinator::from_handle(&app_handle)
        .map_err(|e| format!("Failed to create settings manager: {}", e))?;
    manager
        .update_settings(settings)
        .map_err(|e| format!("Failed to save settings: {}", e))?;
    let _ = app_handle.emit("settings-changed", ());
    Ok(())
}

#[tauri::command]
pub fn reset_settings(app_handle: AppHandle) -> Result<AppSettings, String> {
    let mut manager = SettingsCoordinator::from_handle(&app_handle)
        .map_err(|e| format!("Failed to create settings manager: {}", e))?;
    manager
        .reset_to_defaults()
        .map_err(|e| format!("Failed to reset settings: {}", e))?;
    let _ = app_handle.emit("settings-changed", ());
    Ok(manager.get_settings().clone())
}

// Plugin Management Commands

#[tauri::command]
pub fn add_plugin(
    app_handle: AppHandle,
    command: String,
    name: String,
) -> Result<String, String> {
    use uuid::Uuid;

    let mut manager = SettingsCoordinator::from_handle(&app_handle)
        .map_err(|e| format!("Failed to create settings manager: {}", e))?;

    let mut settings = manager.get_settings().clone();

    if settings.target_providers.plugins.iter().any(|p| p.command == command) {
        return Err("Plugin with this command already exists".to_string());
    }

    let id = Uuid::new_v4().to_string();
    settings.target_providers.plugins.push(PluginProvider {
        id: id.clone(),
        name,
        command,
        enabled: true,
    });

    manager
        .update_settings(settings)
        .map_err(|e| format!("Failed to save settings: {}", e))?;

    let _ = app_handle.emit("settings-changed", ());
    Ok(id)
}

#[tauri::command]
pub fn remove_plugin(app_handle: AppHandle, plugin_id: String) -> Result<(), String> {
    let mut manager = SettingsCoordinator::from_handle(&app_handle)
        .map_err(|e| format!("Failed to create settings manager: {}", e))?;

    let mut settings = manager.get_settings().clone();
    settings.target_providers.plugins.retain(|p| p.id != plugin_id);

    manager
        .update_settings(settings)
        .map_err(|e| format!("Failed to save settings: {}", e))?;
    let _ = app_handle.emit("settings-changed", ());
    Ok(())
}

#[tauri::command]
pub fn update_plugin(app_handle: AppHandle, plugin_id: String, name: String, command: String) -> Result<(), String> {
    let mut manager = SettingsCoordinator::from_handle(&app_handle)
        .map_err(|e| format!("Failed to create settings manager: {}", e))?;

    let mut settings = manager.get_settings().clone();

    if let Some(plugin) = settings.target_providers.plugins.iter_mut().find(|p| p.id == plugin_id) {
        plugin.name = name;
        plugin.command = command;
        manager
            .update_settings(settings)
            .map_err(|e| format!("Failed to save settings: {}", e))?;
        let _ = app_handle.emit("settings-changed", ());
        Ok(())
    } else {
        Err("Plugin not found".to_string())
    }
}

#[tauri::command]
pub fn toggle_plugin(app_handle: AppHandle, plugin_id: String, enabled: bool) -> Result<(), String> {
    let mut manager = SettingsCoordinator::from_handle(&app_handle)
        .map_err(|e| format!("Failed to create settings manager: {}", e))?;

    let mut settings = manager.get_settings().clone();

    if let Some(plugin) = settings.target_providers.plugins.iter_mut().find(|p| p.id == plugin_id) {
        plugin.enabled = enabled;
        manager
            .update_settings(settings)
            .map_err(|e| format!("Failed to save settings: {}", e))?;
        let _ = app_handle.emit("settings-changed", ());
        Ok(())
    } else {
        Err("Plugin not found".to_string())
    }
}

/// Returns true if the executable referenced by the first token of `command` can be found.
#[tauri::command]
pub fn check_plugin_path(command: String) -> bool {
    use std::path::Path;

    let program = extract_program(&command);
    if program.is_empty() {
        return false;
    }

    // Absolute or relative path with separators — check directly
    if program.contains('/') || program.contains('\\') {
        return Path::new(&program).exists();
    }

    // Bare name — search PATH
    if let Ok(path_var) = std::env::var("PATH") {
        let sep = if cfg!(windows) { ';' } else { ':' };
        for dir in path_var.split(sep) {
            let candidate = Path::new(dir).join(&program);
            if candidate.exists() {
                return true;
            }
            #[cfg(windows)]
            {
                let with_exe = Path::new(dir).join(format!("{}.exe", program));
                if with_exe.exists() {
                    return true;
                }
            }
        }
    }
    false
}

// --- Registry ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryPlatform {
    pub url: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryPlugin {
    pub id: String,
    pub name: String,
    pub description: String,
    pub author: String,
    pub version: String,
    pub repo: String,
    pub platforms: std::collections::HashMap<String, RegistryPlatform>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Registry {
    pub version: u32,
    pub plugins: Vec<RegistryPlugin>,
}

/// Fetch and return the registry JSON from the configured URL.
#[tauri::command]
pub async fn fetch_registry(app_handle: AppHandle) -> Result<Registry, String> {
    let registry_url = SettingsCoordinator::from_handle(&app_handle)
        .map_err(|e| e.to_string())?
        .get_settings()
        .registry_url
        .clone();

    let bytes = reqwest::get(&registry_url)
        .await
        .map_err(|e| format!("Failed to fetch registry: {}", e))?
        .bytes()
        .await
        .map_err(|e| format!("Failed to read registry response: {}", e))?;

    serde_json::from_slice(&bytes)
        .map_err(|e| format!("Failed to parse registry: {}", e))
}

/// Download a plugin binary, verify its SHA256, save to the plugins dir, and register it.
#[tauri::command]
pub async fn install_registry_plugin(
    app_handle: AppHandle,
    plugin: RegistryPlugin,
    platform_key: String,
) -> Result<(), String> {
    let platform = plugin.platforms.get(&platform_key)
        .ok_or_else(|| format!("No binary for platform '{}'", platform_key))?;

    // Determine install directory
    let install_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Could not resolve app data dir: {}", e))?
        .join("plugins");

    std::fs::create_dir_all(&install_dir)
        .map_err(|e| format!("Failed to create plugins dir: {}", e))?;

    let ext = if platform_key.starts_with("windows") { ".exe" } else { "" };
    let filename = format!("{}-{}{}", plugin.id, platform_key, ext);
    let dest: PathBuf = install_dir.join(&filename);

    // Download
    println!("[install] plugin='{}' platform='{}' url='{}'", plugin.id, platform_key, platform.url);
    println!("[install] dest='{}'", dest.display());

    let response = reqwest::get(&platform.url)
        .await
        .map_err(|e| format!("Download failed: {}", e))?;

    println!("[install] HTTP {}", response.status());

    if !response.status().is_success() {
        return Err(format!("Download failed: HTTP {} — URL: {}", response.status(), platform.url));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read download: {}", e))?;

    println!("[install] downloaded {} bytes", bytes.len());

    // Verify SHA256 if provided
    if !platform.sha256.is_empty() {
        use std::fmt::Write as FmtWrite;
        let digest = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            let result = hasher.finalize();
            let mut hex = String::with_capacity(64);
            for byte in result {
                let _ = write!(hex, "{:02x}", byte);
            }
            hex
        };
        if digest != platform.sha256 {
            return Err(format!(
                "SHA256 mismatch for '{}': expected {}, got {}",
                plugin.id, platform.sha256, digest
            ));
        }
    }

    // Write binary
    std::fs::write(&dest, &bytes)
        .map_err(|e| format!("Failed to write plugin binary: {}", e))?;

    // Make executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&dest)
            .map_err(|e| e.to_string())?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&dest, perms).map_err(|e| e.to_string())?;
    }

    // Register plugin in settings
    let command = dest.to_string_lossy().to_string();
    let mut manager = SettingsCoordinator::from_handle(&app_handle)
        .map_err(|e| e.to_string())?;
    let mut settings = manager.get_settings().clone();

    // Replace existing entry with same id if present, otherwise add
    let existing = settings.target_providers.plugins.iter().position(|p| p.name == plugin.name);
    let entry = PluginProvider {
        id: if let Some(i) = existing {
            settings.target_providers.plugins[i].id.clone()
        } else {
            uuid::Uuid::new_v4().to_string()
        },
        name: plugin.name.clone(),
        command,
        enabled: true,
    };
    if let Some(i) = existing {
        settings.target_providers.plugins[i] = entry;
    } else {
        settings.target_providers.plugins.push(entry);
    }

    manager.update_settings(settings).map_err(|e| e.to_string())?;
    let _ = app_handle.emit("settings-changed", ());
    Ok(())
}

fn extract_program(command: &str) -> String {
    let trimmed = command.trim();
    if trimmed.starts_with('"') {
        trimmed[1..].split('"').next().unwrap_or("").to_string()
    } else {
        trimmed.split_whitespace().next().unwrap_or("").to_string()
    }
}
