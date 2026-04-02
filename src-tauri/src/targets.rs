use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tauri::AppHandle;

use crate::settings::{AppSettings, TargetProviderSettings};
use crate::target_providers::subprocess::create_subprocess_providers;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    pub id: String,
    pub provider: String,
    pub formats: Vec<String>,
    pub title: String,
    pub description: String,
    pub image: String, // base64 encoded PNG
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendPayload {
    pub content: String,
    pub format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginStatus {
    pub healthy: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginStatusEntry {
    pub id: String,
    pub name: String,
    pub healthy: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginError {
    pub plugin_name: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetTargetsResult {
    pub targets: Vec<Target>,
    pub errors: Vec<PluginError>,
}

#[async_trait]
pub trait TargetProvider: Send + Sync {
    fn name(&self) -> &str;
    async fn get_targets(&self) -> Result<Vec<Target>, Box<dyn std::error::Error + Send + Sync>>;
    async fn send_to_target(
        &self,
        target_id: &str,
        payload: &SendPayload,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    fn is_enabled(&self, settings: &TargetProviderSettings) -> bool;

    /// Returns the plugin's link URL if provided via get_info.
    fn get_link(&self) -> Option<String> {
        None
    }

    /// Returns the plugin's current health status.
    fn get_status(&self) -> PluginStatus {
        PluginStatus {
            healthy: true,
            error: None,
        }
    }

    /// Returns `Some({schema, values})` if this provider supports configuration, `None` otherwise.
    async fn get_config_schema(
        &self,
    ) -> Result<Option<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(None)
    }

    /// Applies configuration values. Only called if `get_config_schema` returned `Some`.
    async fn set_config(
        &self,
        _values: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Err("This provider does not support configuration".into())
    }
}

pub struct TargetProviderCoordinator {
    providers: HashMap<String, Arc<dyn TargetProvider>>,
    settings: AppSettings,
    app_handle: AppHandle,
}

impl TargetProviderCoordinator {
    pub fn new(settings: AppSettings, app_handle: AppHandle) -> Self {
        let mut coordinator = Self {
            providers: HashMap::new(),
            settings: settings.clone(),
            app_handle,
        };

        for provider in
            create_subprocess_providers(&settings.target_providers, &coordinator.app_handle)
        {
            coordinator.register_provider(provider);
        }

        coordinator
    }

    fn register_provider(&mut self, provider: Arc<dyn TargetProvider>) {
        self.providers.insert(provider.name().to_string(), provider);
    }

    pub fn reload_providers(&mut self, settings: &AppSettings) {
        self.settings = settings.clone();

        // Remove providers no longer in settings
        self.providers.retain(|name, _| {
            settings
                .target_providers
                .plugins
                .iter()
                .any(|p| p.name == *name)
        });

        // Re-register subprocess providers
        for provider in create_subprocess_providers(&settings.target_providers, &self.app_handle) {
            self.register_provider(provider);
        }

        for (name, provider) in &self.providers {
            let enabled = provider.is_enabled(&settings.target_providers);
            println!(
                "Provider '{}' is {}",
                name,
                if enabled { "enabled" } else { "disabled" }
            );
        }
    }

    /// Returns cloned providers and settings — caller can release the lock before awaiting.
    pub fn snapshot(&self) -> (Vec<Arc<dyn TargetProvider>>, AppSettings) {
        let providers = self.providers.values().cloned().collect();
        (providers, self.settings.clone())
    }

    /// Returns the health status of all registered plugins.
    pub fn get_plugin_statuses(&self) -> Vec<PluginStatusEntry> {
        self.settings
            .target_providers
            .plugins
            .iter()
            .filter_map(|p| {
                let provider = self.providers.get(&p.name)?;
                let status = provider.get_status();
                Some(PluginStatusEntry {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    healthy: status.healthy,
                    error: status.error,
                })
            })
            .collect()
    }

    /// Looks up a provider by plugin id (from settings), returning a cloned Arc.
    pub fn get_provider_by_id(&self, plugin_id: &str) -> Option<Arc<dyn TargetProvider>> {
        let name = self
            .settings
            .target_providers
            .plugins
            .iter()
            .find(|p| p.id == plugin_id)?
            .name
            .clone();
        self.providers.get(&name).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_target() -> Target {
        Target {
            id: "target-1".to_string(),
            provider: "my-plugin".to_string(),
            formats: vec!["text".to_string(), "html".to_string()],
            title: "My Channel".to_string(),
            description: "A description".to_string(),
            image: "base64data==".to_string(),
        }
    }

    #[test]
    fn target_roundtrip() {
        let t = sample_target();
        let json = serde_json::to_string(&t).unwrap();
        let r: Target = serde_json::from_str(&json).unwrap();
        assert_eq!(r.id, t.id);
        assert_eq!(r.provider, t.provider);
        assert_eq!(r.formats, t.formats);
        assert_eq!(r.title, t.title);
        assert_eq!(r.description, t.description);
        assert_eq!(r.image, t.image);
    }

    #[test]
    fn target_field_names_match_protocol() {
        let json = serde_json::to_string(&sample_target()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(v.get("id").is_some());
        assert!(v.get("provider").is_some());
        assert!(v.get("formats").is_some());
        assert!(v.get("title").is_some());
        assert!(v.get("description").is_some());
        assert!(v.get("image").is_some());
    }

    #[test]
    fn send_payload_roundtrip() {
        let p = SendPayload {
            content: "clipboard text".to_string(),
            format: "text".to_string(),
        };
        let json = serde_json::to_string(&p).unwrap();
        let r: SendPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(r.content, p.content);
        assert_eq!(r.format, p.format);
    }

    #[test]
    fn send_payload_field_names_match_protocol() {
        let p = SendPayload {
            content: "x".to_string(),
            format: "text".to_string(),
        };
        let v: serde_json::Value = serde_json::to_value(&p).unwrap();
        assert!(v.get("content").is_some());
        assert!(v.get("format").is_some());
    }
}

#[tauri::command]
pub async fn get_targets(
    coordinator: tauri::State<'_, Arc<Mutex<TargetProviderCoordinator>>>,
) -> Result<GetTargetsResult, String> {
    let (providers, settings) = coordinator
        .lock()
        .map_err(|e| format!("Failed to lock coordinator: {e}"))?
        .snapshot();
    // MutexGuard is dropped here — safe to await below

    let mut all_targets = Vec::new();
    let mut errors = Vec::new();
    for provider in &providers {
        if provider.is_enabled(&settings.target_providers) {
            match provider.get_targets().await {
                Ok(mut targets) => all_targets.append(&mut targets),
                Err(e) => {
                    let msg = format!("{e}");
                    println!("Failed to get targets from '{}': {}", provider.name(), msg);
                    errors.push(PluginError {
                        plugin_name: provider.name().to_string(),
                        message: msg,
                    });
                }
            }
        }
    }

    println!("Total targets retrieved: {}", all_targets.len());
    Ok(GetTargetsResult {
        targets: all_targets,
        errors,
    })
}

#[tauri::command]
pub async fn get_plugin_config_schema(
    coordinator: tauri::State<'_, Arc<Mutex<TargetProviderCoordinator>>>,
    plugin_id: String,
) -> Result<serde_json::Value, String> {
    let provider = coordinator
        .lock()
        .map_err(|e| format!("Failed to lock coordinator: {e}"))?
        .get_provider_by_id(&plugin_id)
        .ok_or_else(|| format!("Plugin '{plugin_id}' not found"))?;

    provider
        .get_config_schema()
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Plugin does not support configuration".to_string())
}

#[tauri::command]
pub async fn set_plugin_config(
    coordinator: tauri::State<'_, Arc<Mutex<TargetProviderCoordinator>>>,
    plugin_id: String,
    values: serde_json::Value,
) -> Result<(), String> {
    let provider = coordinator
        .lock()
        .map_err(|e| format!("Failed to lock coordinator: {e}"))?
        .get_provider_by_id(&plugin_id)
        .ok_or_else(|| format!("Plugin '{plugin_id}' not found"))?;

    provider.set_config(values).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_plugin_link(
    coordinator: tauri::State<'_, Arc<Mutex<TargetProviderCoordinator>>>,
    plugin_id: String,
) -> Result<Option<String>, String> {
    let provider = coordinator
        .lock()
        .map_err(|e| format!("Failed to lock coordinator: {e}"))?
        .get_provider_by_id(&plugin_id)
        .ok_or_else(|| format!("Plugin '{plugin_id}' not found"))?;

    Ok(provider.get_link())
}

#[tauri::command]
pub fn get_plugin_statuses(
    coordinator: tauri::State<'_, Arc<Mutex<TargetProviderCoordinator>>>,
) -> Result<Vec<PluginStatusEntry>, String> {
    Ok(coordinator
        .lock()
        .map_err(|e| format!("Failed to lock coordinator: {e}"))?
        .get_plugin_statuses())
}

#[tauri::command]
pub async fn send_to_target(
    coordinator: tauri::State<'_, Arc<Mutex<TargetProviderCoordinator>>>,
    target_id: String,
    payload: SendPayload,
) -> Result<(), String> {
    let (providers, settings) = coordinator
        .lock()
        .map_err(|e| format!("Failed to lock coordinator: {e}"))?
        .snapshot();
    // MutexGuard is dropped here — safe to await below

    for provider in &providers {
        if !provider.is_enabled(&settings.target_providers) {
            continue;
        }
        if let Ok(targets) = provider.get_targets().await {
            if targets.iter().any(|t| t.id == target_id) {
                return provider
                    .send_to_target(&target_id, &payload)
                    .await
                    .map_err(|e| format!("Failed to send to target: {e}"));
            }
        }
    }

    Err(format!("Target '{target_id}' not found"))
}
