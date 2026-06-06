use std::io::Write;
use std::process::{Command, Stdio};

use regex::Regex;
use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

pub const CLIPBOARD_PLACEHOLDER: &str = "{clipboard}";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecCommand {
    pub id: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub pattern: String,
    pub path: String,
    #[serde(default)]
    pub args: String,
    #[serde(default)]
    pub working_dir: String,
    #[serde(default)]
    pub pipe_stdin: bool,
}

pub fn command_matches(cmd: &ExecCommand, clipboard: &str) -> bool {
    if cmd.pattern.trim().is_empty() {
        return true;
    }
    Regex::new(&cmd.pattern)
        .map(|re| re.is_match(clipboard))
        .unwrap_or(false)
}

pub fn parse_args(args: &str, clipboard: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_arg = false;
    let mut quote: Option<char> = None;
    for c in args.chars() {
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                } else {
                    cur.push(c);
                }
            }
            None => match c {
                '\'' | '"' => {
                    quote = Some(c);
                    in_arg = true;
                }
                c if c.is_whitespace() => {
                    if in_arg {
                        out.push(std::mem::take(&mut cur));
                        in_arg = false;
                    }
                }
                _ => {
                    cur.push(c);
                    in_arg = true;
                }
            },
        }
    }
    if in_arg {
        out.push(cur);
    }
    out.into_iter()
        .map(|a| a.replace(CLIPBOARD_PLACEHOLDER, clipboard))
        .collect()
}

pub fn run_command(cmd: &ExecCommand, clipboard: &str) -> Result<(), String> {
    if cmd.path.trim().is_empty() {
        return Err("Command path is empty".to_string());
    }
    let path = cmd.path.replace(CLIPBOARD_PLACEHOLDER, clipboard);
    let args = parse_args(&cmd.args, clipboard);

    let mut command = Command::new(&path);
    command.args(&args);
    if !cmd.working_dir.trim().is_empty() {
        command.current_dir(cmd.working_dir.replace(CLIPBOARD_PLACEHOLDER, clipboard));
    }
    command.stdout(Stdio::null()).stderr(Stdio::null());
    command.stdin(if cmd.pipe_stdin {
        Stdio::piped()
    } else {
        Stdio::null()
    });

    let mut child = command
        .spawn()
        .map_err(|e| format!("Failed to launch '{path}': {e}"))?;

    if cmd.pipe_stdin {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(clipboard.as_bytes());
        }
    }
    Ok(())
}

#[tauri::command]
pub fn exec_run(command: ExecCommand, clipboard: String) -> Result<(), String> {
    run_command(&command, &clipboard)
}

#[tauri::command]
pub fn exec_match_commands(commands: Vec<ExecCommand>, clipboard: String) -> Vec<bool> {
    commands
        .iter()
        .map(|c| command_matches(c, &clipboard))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(pattern: &str, path: &str, args: &str) -> ExecCommand {
        ExecCommand {
            id: "1".to_string(),
            name: "test".to_string(),
            enabled: true,
            pattern: pattern.to_string(),
            path: path.to_string(),
            args: args.to_string(),
            working_dir: String::new(),
            pipe_stdin: false,
        }
    }

    #[test]
    fn empty_pattern_always_matches() {
        assert!(command_matches(&cmd("", "x", ""), "anything"));
    }

    #[test]
    fn pattern_matches_clipboard() {
        assert!(command_matches(&cmd(r"^https?://", "x", ""), "https://a.com"));
        assert!(!command_matches(&cmd(r"^https?://", "x", ""), "no url"));
    }

    #[test]
    fn invalid_pattern_never_matches() {
        assert!(!command_matches(&cmd("[invalid", "x", ""), "anything"));
    }

    #[test]
    fn parse_args_splits_on_whitespace() {
        assert_eq!(parse_args("a b  c", ""), vec!["a", "b", "c"]);
    }

    #[test]
    fn parse_args_honors_quotes() {
        assert_eq!(
            parse_args(r#"--msg "hello world" 'a b'"#, ""),
            vec!["--msg", "hello world", "a b"]
        );
    }

    #[test]
    fn parse_args_substitutes_clipboard_placeholder() {
        assert_eq!(
            parse_args("--input {clipboard}", "some text"),
            vec!["--input", "some text"]
        );
    }

    #[test]
    fn parse_args_keeps_substituted_value_as_single_arg() {
        assert_eq!(
            parse_args("{clipboard}", "a b c"),
            vec!["a b c"]
        );
    }

    #[test]
    fn run_command_rejects_empty_path() {
        assert!(run_command(&cmd("", "  ", ""), "x").is_err());
    }
}
