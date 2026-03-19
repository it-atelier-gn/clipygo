use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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
}

pub struct TargetProviderCoordinator {
    providers: HashMap<String, Arc<dyn TargetProvider>>,
    settings: AppSettings,
}

impl TargetProviderCoordinator {
    pub fn new(settings: AppSettings) -> Self {
        let mut coordinator = Self {
            providers: HashMap::new(),
            settings: settings.clone(),
        };

        for provider in create_subprocess_providers(&settings.target_providers) {
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
            settings.target_providers.plugins.iter().any(|p| p.name == *name)
        });

        // Re-register subprocess providers
        for provider in create_subprocess_providers(&settings.target_providers) {
            self.register_provider(provider);
        }

        for (name, provider) in &self.providers {
            let enabled = provider.is_enabled(&settings.target_providers);
            println!("Provider '{}' is {}", name, if enabled { "enabled" } else { "disabled" });
        }
    }

    /// Returns cloned providers and settings — caller can release the lock before awaiting.
    pub fn snapshot(&self) -> (Vec<Arc<dyn TargetProvider>>, AppSettings) {
        let providers = self.providers.values().cloned().collect();
        (providers, self.settings.clone())
    }
}

#[tauri::command]
pub async fn get_targets(
    coordinator: tauri::State<'_, Arc<Mutex<TargetProviderCoordinator>>>,
) -> Result<Vec<Target>, String> {
    let (providers, settings) = coordinator
        .lock()
        .map_err(|e| format!("Failed to lock coordinator: {}", e))?
        .snapshot();
    // MutexGuard is dropped here — safe to await below

    let mut all_targets = Vec::new();
    for provider in &providers {
        if provider.is_enabled(&settings.target_providers) {
            match provider.get_targets().await {
                Ok(mut targets) => all_targets.append(&mut targets),
                Err(e) => println!("Failed to get targets from '{}': {}", provider.name(), e),
            }
        }
    }

    println!("Total targets retrieved: {}", all_targets.len());
    Ok(all_targets)
}

#[tauri::command]
pub async fn send_to_target(
    coordinator: tauri::State<'_, Arc<Mutex<TargetProviderCoordinator>>>,
    target_id: String,
    payload: SendPayload,
) -> Result<(), String> {
    let (providers, settings) = coordinator
        .lock()
        .map_err(|e| format!("Failed to lock coordinator: {}", e))?
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
                    .map_err(|e| format!("Failed to send to target: {}", e));
            }
        }
    }

    Err(format!("Target '{}' not found", target_id))
}
