use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::settings::{PluginProvider, TargetProviderSettings};
use crate::targets::{SendPayload, Target, TargetProvider};

const MAX_FAILURES: u32 = 3;

struct ProcessHandle {
    child: Child,
    stdin: ChildStdin,
    reader: BufReader<ChildStdout>,
}

struct ProviderState {
    process: Option<ProcessHandle>,
    failure_count: u32,
    errored: bool,
}

pub struct SubprocessProvider {
    config: PluginProvider,
    state: Mutex<ProviderState>,
}

// --- Protocol types ---

#[derive(Serialize)]
#[serde(tag = "command", rename_all = "snake_case")]
enum Request<'a> {
    GetInfo,
    GetTargets,
    Send {
        target_id: &'a str,
        content: &'a str,
        format: &'a str,
    },
}


#[derive(Deserialize)]
struct TargetsResponse {
    targets: Vec<Target>,
}

#[derive(Deserialize)]
struct SendResponse {
    success: bool,
    #[serde(default)]
    error: Option<String>,
}

// --- Command parsing ---

/// Splits a command string into program + args, respecting double quotes.
fn parse_command(command: &str) -> Option<(String, Vec<String>)> {
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    for c in command.chars() {
        match c {
            '"' => in_quotes = !in_quotes,
            ' ' if !in_quotes => {
                if !current.is_empty() {
                    parts.push(current.clone());
                    current.clear();
                }
            }
            _ => current.push(c),
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }

    if parts.is_empty() {
        return None;
    }

    let program = parts.remove(0);
    Some((program, parts))
}

// --- Implementation ---

impl SubprocessProvider {
    pub fn new(config: PluginProvider) -> Self {
        Self {
            config,
            state: Mutex::new(ProviderState {
                process: None,
                failure_count: 0,
                errored: false,
            }),
        }
    }

    fn spawn(&self) -> Result<ProcessHandle, String> {
        let (program, args) = parse_command(&self.config.command)
            .ok_or_else(|| format!("Empty command for plugin '{}'", self.config.name))?;

        let mut child = Command::new(&program)
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to spawn plugin '{}': {}", self.config.name, e))?;

        let stdin = child.stdin.take()
            .ok_or_else(|| format!("Plugin '{}': could not get stdin", self.config.name))?;
        let stdout = child.stdout.take()
            .ok_or_else(|| format!("Plugin '{}': could not get stdout", self.config.name))?;

        Ok(ProcessHandle {
            child,
            stdin,
            reader: BufReader::new(stdout),
        })
    }

    /// Send a request and read one line of response.
    /// If the process is not running, spawns it first (calling get_info as health check).
    /// On failure, kills the process and retries once. After MAX_FAILURES, marks errored.
    fn call(&self, request: &Request) -> Result<String, String> {
        let mut state = self.state.lock()
            .map_err(|_| "Plugin state lock poisoned".to_string())?;

        if state.errored {
            return Err(format!(
                "Plugin '{}' is in error state — remove and re-add to reset",
                self.config.name
            ));
        }

        // Up to 2 attempts: once with existing process, once after restart
        for attempt in 0..2u32 {
            if state.process.is_none() {
                match self.spawn() {
                    Ok(handle) => {
                        state.process = Some(handle);
                        // Health check on fresh spawn
                        if let Err(e) = Self::send_recv(state.process.as_mut().unwrap(), &Request::GetInfo)
                            .map(|_| ()) {
                            println!("Plugin '{}' get_info failed: {}", self.config.name, e);
                            state.process = None;
                            state.failure_count += 1;
                            if state.failure_count >= MAX_FAILURES {
                                state.errored = true;
                                return Err(format!("Plugin '{}' failed health check {} times, marking errored", self.config.name, MAX_FAILURES));
                            }
                            continue;
                        }
                        println!("Plugin '{}' started successfully", self.config.name);
                        state.failure_count = 0;
                    }
                    Err(e) => {
                        state.failure_count += 1;
                        if state.failure_count >= MAX_FAILURES {
                            state.errored = true;
                        }
                        return Err(e);
                    }
                }
            }

            match Self::send_recv(state.process.as_mut().unwrap(), request) {
                Ok(response) => {
                    state.failure_count = 0;
                    return Ok(response);
                }
                Err(e) => {
                    println!("Plugin '{}' communication error (attempt {}): {}", self.config.name, attempt + 1, e);
                    // Kill the dead process
                    if let Some(mut handle) = state.process.take() {
                        let _ = handle.child.kill();
                    }
                    state.failure_count += 1;
                    if state.failure_count >= MAX_FAILURES {
                        state.errored = true;
                        return Err(format!("Plugin '{}' failed {} times, marking errored: {}", self.config.name, MAX_FAILURES, e));
                    }
                    // Loop will retry with a fresh spawn
                }
            }
        }

        Err(format!("Plugin '{}' failed after restart attempt", self.config.name))
    }

    fn send_recv(handle: &mut ProcessHandle, request: &Request) -> Result<String, String> {
        let json = serde_json::to_string(request)
            .map_err(|e| format!("Serialization error: {}", e))?;

        writeln!(handle.stdin, "{}", json)
            .map_err(|e| format!("Write error: {}", e))?;

        let mut response = String::new();
        handle.reader.read_line(&mut response)
            .map_err(|e| format!("Read error: {}", e))?;

        if response.is_empty() {
            return Err("Plugin closed stdout unexpectedly".to_string());
        }

        Ok(response.trim().to_string())
    }
}

impl Drop for SubprocessProvider {
    fn drop(&mut self) {
        if let Ok(mut state) = self.state.lock() {
            if let Some(mut handle) = state.process.take() {
                let _ = handle.child.kill();
            }
        }
    }
}

#[async_trait]
impl TargetProvider for SubprocessProvider {
    fn name(&self) -> &str {
        &self.config.name
    }

    async fn get_targets(&self) -> Result<Vec<Target>, Box<dyn std::error::Error + Send + Sync>> {
        let response = self.call(&Request::GetTargets)?;
        let parsed: TargetsResponse = serde_json::from_str(&response)
            .map_err(|e| format!("Plugin '{}' bad response: {}", self.config.name, e))?;
        Ok(parsed.targets)
    }

    async fn send_to_target(
        &self,
        target_id: &str,
        payload: &SendPayload,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let response = self.call(&Request::Send {
            target_id,
            content: &payload.content,
            format: &payload.format,
        })?;
        let parsed: SendResponse = serde_json::from_str(&response)
            .map_err(|e| format!("Plugin '{}' bad response: {}", self.config.name, e))?;

        if parsed.success {
            Ok(())
        } else {
            Err(parsed.error.unwrap_or_else(|| "Unknown plugin error".to_string()).into())
        }
    }

    fn is_enabled(&self, settings: &TargetProviderSettings) -> bool {
        settings.plugins
            .iter()
            .any(|p| p.id == self.config.id && p.enabled)
    }
}

pub fn create_subprocess_providers(settings: &TargetProviderSettings) -> Vec<Arc<dyn TargetProvider>> {
    settings.plugins
        .iter()
        .map(|plugin| Arc::new(SubprocessProvider::new(plugin.clone())) as Arc<dyn TargetProvider>)
        .collect()
}
