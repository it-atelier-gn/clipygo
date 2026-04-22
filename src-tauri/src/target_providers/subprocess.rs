use std::io::{BufRead, BufReader, Write};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::debug_log;
use crate::settings::{PluginProvider, TargetProviderSettings};
use crate::targets::{PluginStatus, SendPayload, Target, TargetProvider};

const MAX_FAILURES: u32 = 3;

struct ProcessHandle {
    child: Child,
    stdin: ChildStdin,
    response_rx: mpsc::Receiver<Result<String, String>>,
    _reader_thread: JoinHandle<()>,
}

struct ProviderState {
    process: Option<ProcessHandle>,
    failure_count: u32,
    errored: bool,
    last_error: Option<String>,
    info: Option<InfoResponse>,
}

pub struct SubprocessProvider {
    config: PluginProvider,
    state: Mutex<ProviderState>,
    app_handle: AppHandle,
}

// --- Protocol types ---

#[derive(Serialize)]
#[serde(tag = "command", rename_all = "snake_case")]
enum Request<'a> {
    GetInfo,
    GetTargets,
    GetConfigSchema,
    SetConfig {
        values: &'a serde_json::Value,
    },
    Send {
        target_id: &'a str,
        content: &'a str,
        format: &'a str,
    },
}

#[derive(Deserialize, Clone, Default)]
#[allow(dead_code)]
struct InfoResponse {
    #[serde(default)]
    name: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    link: Option<String>,
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

// --- Line classification ---

/// Classifies a stdout line from a plugin as an event, a response, or empty.
#[derive(Debug, PartialEq)]
enum LineKind {
    /// JSON with an `"event"` field — plugin-initiated event.
    Event(serde_json::Value),
    /// Any other non-empty line — a response to a pending request.
    Response(String),
    /// Blank / whitespace-only line — skip.
    Empty,
}

fn classify_line(line: &str) -> LineKind {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return LineKind::Empty;
    }
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if value.get("event").is_some() {
            return LineKind::Event(value);
        }
    }
    LineKind::Response(trimmed.to_string())
}

/// Reads lines from `reader`, sending responses through `response_tx` and events through `event_tx`.
/// Runs until EOF or an I/O error. Designed to run in a background thread.
fn reader_loop(
    reader: impl std::io::Read,
    response_tx: mpsc::Sender<Result<String, String>>,
    event_tx: mpsc::Sender<serde_json::Value>,
) {
    let mut reader = BufReader::new(reader);
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => {
                let _ = response_tx.send(Err("Plugin closed stdout unexpectedly".to_string()));
                break;
            }
            Err(e) => {
                let _ = response_tx.send(Err(format!("Read error: {e}")));
                break;
            }
            Ok(_) => match classify_line(&line) {
                LineKind::Empty => continue,
                LineKind::Event(value) => {
                    let _ = event_tx.send(value);
                }
                LineKind::Response(trimmed) => {
                    if response_tx.send(Ok(trimmed)).is_err() {
                        break;
                    }
                }
            },
        }
    }
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
    pub fn new(config: PluginProvider, app_handle: AppHandle) -> Self {
        Self {
            config,
            state: Mutex::new(ProviderState {
                process: None,
                failure_count: 0,
                errored: false,
                last_error: None,
                info: None,
            }),
            app_handle,
        }
    }

    fn spawn(&self) -> Result<ProcessHandle, String> {
        let (program, args) = parse_command(&self.config.command)
            .ok_or_else(|| format!("Empty command for plugin '{}'", self.config.name))?;

        #[allow(unused_mut)]
        let mut cmd = Command::new(&program);
        cmd.args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(target_os = "windows")]
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn plugin '{}': {}", self.config.name, e))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| format!("Plugin '{}': could not get stdin", self.config.name))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| format!("Plugin '{}': could not get stdout", self.config.name))?;
        let stderr = child.stderr.take();

        let (response_tx, response_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel::<serde_json::Value>();
        let app_handle = self.app_handle.clone();
        let plugin_name = self.config.name.clone();

        let reader_thread = std::thread::spawn(move || {
            reader_loop(stdout, response_tx, event_tx);
        });

        // Background thread to forward events to Tauri
        {
            let app_handle = app_handle.clone();
            let plugin_name = plugin_name.clone();
            std::thread::spawn(move || {
                while let Ok(value) = event_rx.recv() {
                    debug_log(
                        &app_handle,
                        &plugin_name,
                        "info",
                        format!("Event: {}", value["event"]),
                    );
                    let _ = app_handle.emit("plugin-event", value);
                }
            });
        }

        // Background thread to capture stderr and emit as debug logs
        if let Some(stderr) = stderr {
            let app_handle = self.app_handle.clone();
            let plugin_name = self.config.name.clone();
            std::thread::spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines() {
                    match line {
                        Ok(line) if !line.trim().is_empty() => {
                            debug_log(&app_handle, &plugin_name, "debug", line);
                        }
                        Err(_) => break,
                        _ => {}
                    }
                }
            });
        }

        Ok(ProcessHandle {
            child,
            stdin,
            response_rx,
            _reader_thread: reader_thread,
        })
    }

    /// Send a request and read one line of response.
    /// If the process is not running, spawns it first (calling get_info as health check).
    /// On failure, kills the process and retries once. After MAX_FAILURES, marks errored.
    fn call(&self, request: &Request) -> Result<String, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "Plugin state lock poisoned".to_string())?;

        if state.errored {
            return Err(state
                .last_error
                .clone()
                .unwrap_or_else(|| format!("Plugin '{}' is in error state", self.config.name)));
        }

        // Up to 2 attempts: once with existing process, once after restart
        for attempt in 0..2u32 {
            if state.process.is_none() {
                match self.spawn() {
                    Ok(handle) => {
                        state.process = Some(handle);
                        // Health check on fresh spawn — also captures plugin info
                        if let Err(e) =
                            Self::send_recv(state.process.as_mut().unwrap(), &Request::GetInfo)
                                .and_then(|resp| {
                                    let info: InfoResponse = serde_json::from_str(&resp)
                                        .map_err(|e| format!("Bad get_info response: {e}"))?;
                                    state.info = Some(info);
                                    Ok(())
                                })
                        {
                            let msg =
                                format!("Plugin '{}' failed health check: {e}", self.config.name);
                            debug_log(&self.app_handle, &self.config.name, "warn", msg.clone());
                            state.process = None;
                            state.failure_count += 1;
                            state.last_error = Some(msg.clone());
                            if state.failure_count >= MAX_FAILURES {
                                state.errored = true;
                                return Err(msg);
                            }
                            continue;
                        }
                        debug_log(
                            &self.app_handle,
                            &self.config.name,
                            "info",
                            format!("Plugin '{}' started successfully", self.config.name),
                        );
                        state.failure_count = 0;
                    }
                    Err(e) => {
                        state.failure_count += 1;
                        state.last_error = Some(e.clone());
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
                    let msg = format!(
                        "Plugin '{}' communication error (attempt {}): {e}",
                        self.config.name,
                        attempt + 1,
                    );
                    debug_log(&self.app_handle, &self.config.name, "error", msg.clone());
                    // Kill the dead process
                    if let Some(mut handle) = state.process.take() {
                        let _ = handle.child.kill();
                    }
                    state.failure_count += 1;
                    state.last_error = Some(msg.clone());
                    if state.failure_count >= MAX_FAILURES {
                        state.errored = true;
                        return Err(msg);
                    }
                    // Loop will retry with a fresh spawn
                }
            }
        }

        Err(format!(
            "Plugin '{}' failed after restart attempt",
            self.config.name
        ))
    }

    fn send_recv(handle: &mut ProcessHandle, request: &Request) -> Result<String, String> {
        let json =
            serde_json::to_string(request).map_err(|e| format!("Serialization error: {e}"))?;

        writeln!(handle.stdin, "{json}").map_err(|e| format!("Write error: {e}"))?;

        handle
            .response_rx
            .recv()
            .map_err(|_| "Plugin reader thread died".to_string())
            .and_then(|r| r)
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

    fn get_link(&self) -> Option<String> {
        self.state
            .lock()
            .ok()
            .and_then(|s| s.info.as_ref().and_then(|i| i.link.clone()))
    }

    fn get_status(&self) -> PluginStatus {
        self.state
            .lock()
            .map(|s| PluginStatus {
                healthy: !s.errored,
                error: s.last_error.clone(),
            })
            .unwrap_or_default()
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
            Err(parsed
                .error
                .unwrap_or_else(|| "Unknown plugin error".to_string())
                .into())
        }
    }

    async fn get_config_schema(
        &self,
    ) -> Result<Option<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>> {
        let response = self.call(&Request::GetConfigSchema)?;
        let value: serde_json::Value = serde_json::from_str(&response).map_err(|e| {
            format!(
                "Plugin '{}' bad get_config_schema response: {e}",
                self.config.name
            )
        })?;
        if value.get("schema").is_some() {
            Ok(Some(value))
        } else {
            Err(format!(
                "Plugin '{}' get_config_schema: missing 'schema' field",
                self.config.name
            )
            .into())
        }
    }

    async fn set_config(
        &self,
        values: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let response = self.call(&Request::SetConfig { values: &values })?;
        let parsed: SendResponse = serde_json::from_str(&response)
            .map_err(|e| format!("Plugin '{}' bad set_config response: {e}", self.config.name))?;
        if parsed.success {
            Ok(())
        } else {
            Err(parsed
                .error
                .unwrap_or_else(|| "Unknown plugin error".to_string())
                .into())
        }
    }

    fn is_enabled(&self, settings: &TargetProviderSettings) -> bool {
        settings
            .plugins
            .iter()
            .any(|p| p.id == self.config.id && p.enabled)
    }
}

pub fn create_subprocess_providers(
    settings: &TargetProviderSettings,
    app_handle: &AppHandle,
) -> Vec<Arc<dyn TargetProvider>> {
    settings
        .plugins
        .iter()
        .map(|plugin| {
            Arc::new(SubprocessProvider::new(plugin.clone(), app_handle.clone()))
                as Arc<dyn TargetProvider>
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Request serialization ---

    #[test]
    fn request_get_info_serializes() {
        let json = serde_json::to_string(&Request::GetInfo).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["command"], "get_info");
    }

    #[test]
    fn request_get_targets_serializes() {
        let json = serde_json::to_string(&Request::GetTargets).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["command"], "get_targets");
    }

    #[test]
    fn request_get_config_schema_serializes() {
        let json = serde_json::to_string(&Request::GetConfigSchema).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["command"], "get_config_schema");
    }

    #[test]
    fn request_set_config_serializes() {
        let values = serde_json::json!({"key": "value"});
        let json = serde_json::to_string(&Request::SetConfig { values: &values }).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["command"], "set_config");
        assert_eq!(v["values"]["key"], "value");
    }

    #[test]
    fn request_send_serializes() {
        let json = serde_json::to_string(&Request::Send {
            target_id: "t1",
            content: "hello",
            format: "text",
        })
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["command"], "send");
        assert_eq!(v["target_id"], "t1");
        assert_eq!(v["content"], "hello");
        assert_eq!(v["format"], "text");
    }

    // --- Response deserialization ---

    #[test]
    fn targets_response_deserializes() {
        let json = r#"{"targets":[{"id":"t1","provider":"p","formats":["text"],"title":"T","description":"D","image":""}]}"#;
        let r: TargetsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(r.targets.len(), 1);
        assert_eq!(r.targets[0].id, "t1");
    }

    #[test]
    fn targets_response_empty_list() {
        let r: TargetsResponse = serde_json::from_str(r#"{"targets":[]}"#).unwrap();
        assert!(r.targets.is_empty());
    }

    #[test]
    fn send_response_success() {
        let r: SendResponse = serde_json::from_str(r#"{"success":true}"#).unwrap();
        assert!(r.success);
        assert!(r.error.is_none());
    }

    #[test]
    fn send_response_failure_with_error() {
        let r: SendResponse =
            serde_json::from_str(r#"{"success":false,"error":"something went wrong"}"#).unwrap();
        assert!(!r.success);
        assert_eq!(r.error.as_deref(), Some("something went wrong"));
    }

    #[test]
    fn send_response_failure_without_error_field() {
        let r: SendResponse = serde_json::from_str(r#"{"success":false}"#).unwrap();
        assert!(!r.success);
        assert!(r.error.is_none());
    }

    // --- parse_command ---

    #[test]
    fn parse_command_simple() {
        let (prog, args) = parse_command("node plugin.js").unwrap();
        assert_eq!(prog, "node");
        assert_eq!(args, vec!["plugin.js"]);
    }

    #[test]
    fn parse_command_no_args() {
        let (prog, args) = parse_command("myplugin").unwrap();
        assert_eq!(prog, "myplugin");
        assert!(args.is_empty());
    }

    #[test]
    fn parse_command_multiple_args() {
        let (prog, args) = parse_command("node plugin.js --verbose --port 8080").unwrap();
        assert_eq!(prog, "node");
        assert_eq!(args, vec!["plugin.js", "--verbose", "--port", "8080"]);
    }

    #[test]
    fn parse_command_quoted_program() {
        let (prog, args) = parse_command(r#""C:\path with spaces\plugin.exe" --arg"#).unwrap();
        assert_eq!(prog, r"C:\path with spaces\plugin.exe");
        assert_eq!(args, vec!["--arg"]);
    }

    #[test]
    fn parse_command_quoted_arg() {
        let (prog, args) = parse_command(r#"node "my plugin.js""#).unwrap();
        assert_eq!(prog, "node");
        assert_eq!(args, vec!["my plugin.js"]);
    }

    #[test]
    fn parse_command_extra_spaces() {
        let (prog, args) = parse_command("node  plugin.js").unwrap();
        assert_eq!(prog, "node");
        assert_eq!(args, vec!["plugin.js"]);
    }

    #[test]
    fn parse_command_empty_returns_none() {
        assert!(parse_command("").is_none());
    }

    #[test]
    fn parse_command_whitespace_only_returns_none() {
        assert!(parse_command("   ").is_none());
    }

    // --- classify_line ---

    #[test]
    fn classify_line_empty_string() {
        assert_eq!(classify_line(""), LineKind::Empty);
    }

    #[test]
    fn classify_line_whitespace_only() {
        assert_eq!(classify_line("   \n"), LineKind::Empty);
    }

    #[test]
    fn classify_line_event_with_event_field() {
        let line = r#"{"event":"incoming_message","data":{"from_name":"Alice"}}"#;
        match classify_line(line) {
            LineKind::Event(v) => {
                assert_eq!(v["event"], "incoming_message");
                assert_eq!(v["data"]["from_name"], "Alice");
            }
            other => panic!("Expected Event, got {other:?}"),
        }
    }

    #[test]
    fn classify_line_event_with_whitespace() {
        let line = r#"  {"event":"connection_status","data":{"status":"connected"}}  "#;
        match classify_line(line) {
            LineKind::Event(v) => assert_eq!(v["event"], "connection_status"),
            other => panic!("Expected Event, got {other:?}"),
        }
    }

    #[test]
    fn classify_line_response_json_without_event() {
        let line = r#"{"targets":[]}"#;
        match classify_line(line) {
            LineKind::Response(s) => assert_eq!(s, r#"{"targets":[]}"#),
            other => panic!("Expected Response, got {other:?}"),
        }
    }

    #[test]
    fn classify_line_response_non_json() {
        let line = "not json at all";
        match classify_line(line) {
            LineKind::Response(s) => assert_eq!(s, "not json at all"),
            other => panic!("Expected Response, got {other:?}"),
        }
    }

    #[test]
    fn classify_line_response_json_with_event_null() {
        // "event" key exists but is null — still an event (has the key)
        let line = r#"{"event":null}"#;
        match classify_line(line) {
            LineKind::Event(v) => assert!(v.get("event").is_some()),
            other => panic!("Expected Event, got {other:?}"),
        }
    }

    // --- reader_loop ---

    #[test]
    fn reader_loop_separates_events_and_responses() {
        use std::io::Cursor;

        let input = concat!(
            r#"{"name":"test","version":"1.0"}"#,
            "\n",
            r#"{"event":"incoming_message","data":{}}"#,
            "\n",
            r#"{"targets":[]}"#,
            "\n",
            r#"{"event":"connection_status","data":{"status":"ok"}}"#,
            "\n",
        );

        let (resp_tx, resp_rx) = mpsc::channel();
        let (evt_tx, evt_rx) = mpsc::channel();

        reader_loop(Cursor::new(input), resp_tx, evt_tx);

        // Collect responses (excluding the EOF error)
        let responses: Vec<String> = resp_rx.try_iter().filter_map(|r| r.ok()).collect();
        let events: Vec<serde_json::Value> = evt_rx.try_iter().collect();

        assert_eq!(responses.len(), 2);
        assert!(responses[0].contains("\"name\""));
        assert!(responses[1].contains("\"targets\""));

        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["event"], "incoming_message");
        assert_eq!(events[1]["event"], "connection_status");
    }

    #[test]
    fn reader_loop_eof_sends_error() {
        use std::io::Cursor;

        let input = ""; // immediate EOF
        let (resp_tx, resp_rx) = mpsc::channel();
        let (evt_tx, _evt_rx) = mpsc::channel();

        reader_loop(Cursor::new(input), resp_tx, evt_tx);

        let result = resp_rx.try_recv().unwrap();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("closed stdout"));
    }

    #[test]
    fn reader_loop_skips_blank_lines() {
        use std::io::Cursor;

        let input = concat!("\n", "   \n", r#"{"success":true}"#, "\n", "\n",);

        let (resp_tx, resp_rx) = mpsc::channel();
        let (evt_tx, evt_rx) = mpsc::channel();

        reader_loop(Cursor::new(input), resp_tx, evt_tx);

        let responses: Vec<String> = resp_rx.try_iter().filter_map(|r| r.ok()).collect();
        let events: Vec<serde_json::Value> = evt_rx.try_iter().collect();

        assert_eq!(responses.len(), 1);
        assert!(responses[0].contains("success"));
        assert_eq!(events.len(), 0);
    }

    #[test]
    fn reader_loop_handles_interleaved_events_and_responses() {
        use std::io::Cursor;

        let input = concat!(
            r#"{"event":"e1","data":{}}"#,
            "\n",
            r#"{"event":"e2","data":{}}"#,
            "\n",
            r#"{"event":"e3","data":{}}"#,
            "\n",
            r#"{"response":"finally"}"#,
            "\n",
        );

        let (resp_tx, resp_rx) = mpsc::channel();
        let (evt_tx, evt_rx) = mpsc::channel();

        reader_loop(Cursor::new(input), resp_tx, evt_tx);

        let responses: Vec<String> = resp_rx.try_iter().filter_map(|r| r.ok()).collect();
        let events: Vec<serde_json::Value> = evt_rx.try_iter().collect();

        assert_eq!(responses.len(), 1);
        assert_eq!(events.len(), 3);
    }
}
