use std::sync::OnceLock;

use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinTransform {
    StripTracking,
    Trim,
    CollapseWhitespace,
    Uppercase,
    Lowercase,
    TitleCase,
    JsonPretty,
    JsonMinify,
    XmlPretty,
    Base64Encode,
    Base64Decode,
    UrlEncode,
    UrlDecode,
    SnakeCase,
    CamelCase,
    KebabCase,
    StripHtml,
    HtmlEncode,
    SortLines,
    DedupeLines,
    RemoveEmptyLines,
    ReverseLines,
    TrimLines,
    NormalizeNewlines,
    RemoveDiacritics,
    Slugify,
    StraightenQuotes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MorphAction {
    Replace { find: String, replace: String },
    Builtin { transform: BuiltinTransform },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MorphRule {
    pub id: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub pattern: String,
    pub action: MorphAction,
}

pub enum CompiledAction {
    Replace { find: Regex, replace: String },
    Builtin(BuiltinTransform),
}

pub struct CompiledMorphRule {
    pub name: String,
    pub pattern: Regex,
    pub action: CompiledAction,
}

pub struct MorphResult {
    pub rule_name: String,
    pub output: String,
}

pub fn compile_rules(rules: &[MorphRule]) -> Vec<CompiledMorphRule> {
    rules
        .iter()
        .filter(|r| r.enabled)
        .filter_map(|r| {
            let pattern = Regex::new(&r.pattern)
                .map_err(|e| println!("Morph rule '{}': invalid pattern: {e}", r.name))
                .ok()?;
            let action = match &r.action {
                MorphAction::Replace { find, replace } => {
                    let find = Regex::new(find)
                        .map_err(|e| println!("Morph rule '{}': invalid find: {e}", r.name))
                        .ok()?;
                    CompiledAction::Replace {
                        find,
                        replace: replace.clone(),
                    }
                }
                MorphAction::Builtin { transform } => CompiledAction::Builtin(*transform),
            };
            Some(CompiledMorphRule {
                name: r.name.clone(),
                pattern,
                action,
            })
        })
        .collect()
}

pub fn apply_first(rules: &[CompiledMorphRule], input: &str) -> Option<MorphResult> {
    for rule in rules {
        if rule.pattern.is_match(input) {
            let output = apply_action(&rule.action, input);
            if output != input {
                return Some(MorphResult {
                    rule_name: rule.name.clone(),
                    output,
                });
            }
        }
    }
    None
}

/// Applies a single built-in transform to `text`. Used by the manual Morph
/// picker window for live preview and on-demand application.
#[tauri::command]
pub fn morph_preview(text: String, transform: BuiltinTransform) -> String {
    apply_builtin(transform, &text)
}

#[derive(Debug, Serialize)]
pub struct MorphTestResult {
    pub matched: bool,
    pub changed: bool,
    pub output: String,
    pub error: Option<String>,
}

/// Compiles a single rule and runs it against `input` so the settings UI can
/// preview what a rule does while it is being authored. Unlike the live monitor
/// this ignores the rule's `enabled` flag and reports regex errors verbatim.
#[tauri::command]
pub fn morph_test_rule(rule: MorphRule, input: String) -> MorphTestResult {
    let pattern = match Regex::new(&rule.pattern) {
        Ok(re) => re,
        Err(e) => {
            return MorphTestResult {
                matched: false,
                changed: false,
                output: input,
                error: Some(format!("Invalid match pattern: {e}")),
            }
        }
    };
    let action = match &rule.action {
        MorphAction::Replace { find, replace } => match Regex::new(find) {
            Ok(re) => CompiledAction::Replace {
                find: re,
                replace: replace.clone(),
            },
            Err(e) => {
                return MorphTestResult {
                    matched: false,
                    changed: false,
                    output: input,
                    error: Some(format!("Invalid find pattern: {e}")),
                }
            }
        },
        MorphAction::Builtin { transform } => CompiledAction::Builtin(*transform),
    };
    let matched = pattern.is_match(&input);
    let output = if matched {
        apply_action(&action, &input)
    } else {
        input.clone()
    };
    let changed = output != input;
    MorphTestResult {
        matched,
        changed,
        output,
        error: None,
    }
}

fn apply_action(action: &CompiledAction, input: &str) -> String {
    match action {
        CompiledAction::Replace { find, replace } => {
            find.replace_all(input, replace.as_str()).into_owned()
        }
        CompiledAction::Builtin(transform) => apply_builtin(*transform, input),
    }
}

fn apply_builtin(transform: BuiltinTransform, input: &str) -> String {
    match transform {
        BuiltinTransform::StripTracking => strip_tracking(input),
        BuiltinTransform::Trim => input.trim().to_string(),
        BuiltinTransform::CollapseWhitespace => collapse_whitespace(input),
        BuiltinTransform::Uppercase => input.to_uppercase(),
        BuiltinTransform::Lowercase => input.to_lowercase(),
        BuiltinTransform::TitleCase => title_case(input),
        BuiltinTransform::JsonPretty => reformat_json(input, true),
        BuiltinTransform::JsonMinify => reformat_json(input, false),
        BuiltinTransform::XmlPretty => pretty_xml(input),
        BuiltinTransform::Base64Encode => base64_encode(input),
        BuiltinTransform::Base64Decode => base64_decode(input),
        BuiltinTransform::UrlEncode => percent_encode(input),
        BuiltinTransform::UrlDecode => percent_decode(input),
        BuiltinTransform::SnakeCase => join_words(input, "_", true),
        BuiltinTransform::KebabCase => join_words(input, "-", true),
        BuiltinTransform::CamelCase => camel_case(input),
        BuiltinTransform::StripHtml => strip_html(input),
        BuiltinTransform::HtmlEncode => html_encode(input),
        BuiltinTransform::SortLines => transform_lines(input, |lines| lines.sort()),
        BuiltinTransform::DedupeLines => dedupe_lines(input),
        BuiltinTransform::RemoveEmptyLines => {
            transform_lines(input, |lines| lines.retain(|l| !l.trim().is_empty()))
        }
        BuiltinTransform::ReverseLines => transform_lines(input, |lines| lines.reverse()),
        BuiltinTransform::TrimLines => trim_lines(input),
        BuiltinTransform::NormalizeNewlines => normalize_newlines(input),
        BuiltinTransform::RemoveDiacritics => remove_diacritics(input),
        BuiltinTransform::Slugify => slugify(input),
        BuiltinTransform::StraightenQuotes => straighten_quotes(input),
    }
}

fn query_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\?([^#\s]*)").expect("query regex"))
}

fn is_tracking_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    k.starts_with("utm_")
        || matches!(
            k.as_str(),
            "gclid"
                | "fbclid"
                | "mc_eid"
                | "mc_cid"
                | "igshid"
                | "_hsenc"
                | "_hsmi"
                | "yclid"
                | "msclkid"
                | "ref_src"
                | "spm"
                | "vero_id"
                | "oly_anon_id"
                | "oly_enc_id"
                | "wickedid"
        )
}

fn strip_tracking(input: &str) -> String {
    query_re()
        .replace_all(input, |caps: &Captures| {
            let kept: Vec<&str> = caps[1]
                .split('&')
                .filter(|pair| !pair.is_empty())
                .filter(|pair| !is_tracking_key(pair.split('=').next().unwrap_or("")))
                .collect();
            if kept.is_empty() {
                String::new()
            } else {
                format!("?{}", kept.join("&"))
            }
        })
        .into_owned()
}

fn collapse_whitespace(input: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"\s+").expect("whitespace regex"));
    re.replace_all(input.trim(), " ").into_owned()
}

fn title_case(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut at_word_start = true;
    for c in input.chars() {
        if c.is_whitespace() {
            at_word_start = true;
            out.push(c);
        } else if at_word_start {
            out.extend(c.to_uppercase());
            at_word_start = false;
        } else {
            out.extend(c.to_lowercase());
        }
    }
    out
}

fn reformat_json(input: &str, pretty: bool) -> String {
    match serde_json::from_str::<serde_json::Value>(input.trim()) {
        Ok(value) => {
            let formatted = if pretty {
                serde_json::to_string_pretty(&value)
            } else {
                serde_json::to_string(&value)
            };
            formatted.unwrap_or_else(|_| input.to_string())
        }
        Err(_) => input.to_string(),
    }
}

fn base64_encode(input: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(input.as_bytes())
}

fn base64_decode(input: &str) -> String {
    use base64::Engine;
    match base64::engine::general_purpose::STANDARD.decode(input.trim().as_bytes()) {
        Ok(bytes) => String::from_utf8(bytes).unwrap_or_else(|_| input.to_string()),
        Err(_) => input.to_string(),
    }
}

fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| input.to_string())
}

fn split_words(s: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    let mut words = Vec::new();
    let mut cur = String::new();
    for i in 0..chars.len() {
        let c = chars[i];
        if c.is_alphanumeric() {
            if !cur.is_empty() {
                let prev = chars[i - 1];
                let next_lower = chars.get(i + 1).map(|n| n.is_lowercase()).unwrap_or(false);
                let boundary = (prev.is_lowercase() && c.is_uppercase())
                    || (prev.is_uppercase() && c.is_uppercase() && next_lower);
                if boundary {
                    words.push(std::mem::take(&mut cur));
                }
            }
            cur.push(c);
        } else if !cur.is_empty() {
            words.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
}

fn join_words(input: &str, sep: &str, lower: bool) -> String {
    let words = split_words(input);
    if words.is_empty() {
        return input.to_string();
    }
    words
        .iter()
        .map(|w| if lower { w.to_lowercase() } else { w.clone() })
        .collect::<Vec<_>>()
        .join(sep)
}

fn camel_case(input: &str) -> String {
    let words = split_words(input);
    if words.is_empty() {
        return input.to_string();
    }
    let mut out = String::new();
    for (i, w) in words.iter().enumerate() {
        if i == 0 {
            out.push_str(&w.to_lowercase());
        } else {
            out.push_str(&capitalize(w));
        }
    }
    out
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
        None => String::new(),
    }
}

fn strip_html(input: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"<[^>]*>").expect("html tag regex"));
    let no_tags = re.replace_all(input, " ");
    let decoded = no_tags
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    collapse_whitespace(&decoded)
}

fn html_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

fn trim_lines(input: &str) -> String {
    input
        .lines()
        .map(|l| l.trim())
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalize_newlines(input: &str) -> String {
    input.replace("\r\n", "\n").replace('\r', "\n")
}

fn straighten_quotes(input: &str) -> String {
    input
        .chars()
        .map(|c| match c {
            '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{00AB}' | '\u{00BB}' => '"',
            '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{2039}' | '\u{203A}' => '\'',
            '\u{2013}' | '\u{2014}' | '\u{2212}' => '-',
            '\u{00A0}' => ' ',
            other => other,
        })
        .collect::<String>()
        .replace('\u{2026}', "...")
}

fn deaccent(c: char) -> Option<&'static str> {
    Some(match c {
        'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'ā' | 'ă' | 'ą' => "a",
        'Á' | 'À' | 'Â' | 'Ä' | 'Ã' | 'Å' | 'Ā' | 'Ă' | 'Ą' => "A",
        'é' | 'è' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => "e",
        'É' | 'È' | 'Ê' | 'Ë' | 'Ē' | 'Ė' | 'Ę' | 'Ě' => "E",
        'í' | 'ì' | 'î' | 'ï' | 'ī' | 'į' => "i",
        'Í' | 'Ì' | 'Î' | 'Ï' | 'Ī' | 'Į' => "I",
        'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ø' | 'ō' => "o",
        'Ó' | 'Ò' | 'Ô' | 'Ö' | 'Õ' | 'Ø' | 'Ō' => "O",
        'ú' | 'ù' | 'û' | 'ü' | 'ū' | 'ů' => "u",
        'Ú' | 'Ù' | 'Û' | 'Ü' | 'Ū' | 'Ů' => "U",
        'ç' | 'ć' | 'č' => "c",
        'Ç' | 'Ć' | 'Č' => "C",
        'ñ' | 'ń' => "n",
        'Ñ' | 'Ń' => "N",
        'ý' | 'ÿ' => "y",
        'Ý' | 'Ÿ' => "Y",
        'š' => "s",
        'Š' => "S",
        'ž' => "z",
        'Ž' => "Z",
        'ð' => "d",
        'Ð' => "D",
        'ß' => "ss",
        'æ' => "ae",
        'Æ' => "AE",
        'œ' => "oe",
        'Œ' => "OE",
        'þ' => "th",
        'Þ' => "TH",
        _ => return None,
    })
}

fn remove_diacritics(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match deaccent(c) {
            Some(s) => out.push_str(s),
            None => out.push(c),
        }
    }
    out
}

fn slugify(input: &str) -> String {
    let normalized = remove_diacritics(input).to_lowercase();
    let mut out = String::with_capacity(normalized.len());
    let mut prev_dash = false;
    for c in normalized.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            prev_dash = false;
        } else if !out.is_empty() && !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn transform_lines<F: FnOnce(&mut Vec<&str>)>(input: &str, f: F) -> String {
    let mut lines: Vec<&str> = input.lines().collect();
    f(&mut lines);
    lines.join("\n")
}

fn dedupe_lines(input: &str) -> String {
    let mut seen = std::collections::HashSet::new();
    input
        .lines()
        .filter(|l| seen.insert(*l))
        .collect::<Vec<_>>()
        .join("\n")
}

enum XmlToken {
    Open(String),
    Close(String),
    SelfClose(String),
    Other(String),
    Text(String),
}

fn tokenize_xml(s: &str) -> Option<Vec<XmlToken>> {
    let len = s.len();
    let mut i = 0;
    let mut tokens = Vec::new();
    while i < len {
        if s.as_bytes()[i] == b'<' {
            let rest = &s[i..];
            let end = if rest.starts_with("<!--") {
                i + rest.find("-->")? + 3
            } else if rest.starts_with("<![CDATA[") {
                i + rest.find("]]>")? + 3
            } else {
                i + rest.find('>')? + 1
            };
            let raw = s[i..end].to_string();
            if raw.starts_with("<!--") || raw.starts_with("<![CDATA[") || raw.starts_with("<?") {
                tokens.push(XmlToken::Other(raw));
            } else if raw.starts_with("</") {
                tokens.push(XmlToken::Close(raw));
            } else if raw.ends_with("/>") {
                tokens.push(XmlToken::SelfClose(raw));
            } else if raw.starts_with("<!") {
                tokens.push(XmlToken::Other(raw));
            } else {
                tokens.push(XmlToken::Open(raw));
            }
            i = end;
        } else {
            let p = s[i..].find('<').unwrap_or(len - i);
            let text = s[i..i + p].trim();
            if !text.is_empty() {
                tokens.push(XmlToken::Text(text.to_string()));
            }
            i += p;
        }
    }
    Some(tokens)
}

fn pretty_xml(input: &str) -> String {
    let trimmed = input.trim();
    if !trimmed.starts_with('<') {
        return input.to_string();
    }
    let tokens = match tokenize_xml(trimmed) {
        Some(t) => t,
        None => return input.to_string(),
    };
    let mut out = String::new();
    let mut depth: usize = 0;
    let mut i = 0;
    while i < tokens.len() {
        match &tokens[i] {
            XmlToken::Close(s) => {
                depth = depth.saturating_sub(1);
                push_indented(&mut out, depth, s);
                i += 1;
            }
            XmlToken::Open(s) => {
                if i + 2 < tokens.len() {
                    if let (XmlToken::Text(t), XmlToken::Close(c)) =
                        (&tokens[i + 1], &tokens[i + 2])
                    {
                        push_indented(&mut out, depth, &format!("{s}{t}{c}"));
                        i += 3;
                        continue;
                    }
                }
                push_indented(&mut out, depth, s);
                depth += 1;
                i += 1;
            }
            XmlToken::SelfClose(s) | XmlToken::Other(s) | XmlToken::Text(s) => {
                push_indented(&mut out, depth, s);
                i += 1;
            }
        }
    }
    out.trim_end().to_string()
}

fn push_indented(out: &mut String, depth: usize, content: &str) {
    for _ in 0..depth {
        out.push_str("  ");
    }
    out.push_str(content);
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn replace_rule(name: &str, pattern: &str, find: &str, replace: &str) -> MorphRule {
        MorphRule {
            id: name.to_string(),
            name: name.to_string(),
            enabled: true,
            pattern: pattern.to_string(),
            action: MorphAction::Replace {
                find: find.to_string(),
                replace: replace.to_string(),
            },
        }
    }

    fn builtin_rule(name: &str, pattern: &str, transform: BuiltinTransform) -> MorphRule {
        MorphRule {
            id: name.to_string(),
            name: name.to_string(),
            enabled: true,
            pattern: pattern.to_string(),
            action: MorphAction::Builtin { transform },
        }
    }

    #[test]
    fn strip_tracking_removes_only_tracking_params() {
        assert_eq!(
            strip_tracking("https://x.com/p?utm_source=a&id=7"),
            "https://x.com/p?id=7"
        );
    }

    #[test]
    fn strip_tracking_drops_query_when_all_removed() {
        assert_eq!(
            strip_tracking("https://x.com/p?utm_source=a&fbclid=z"),
            "https://x.com/p"
        );
    }

    #[test]
    fn strip_tracking_preserves_fragment_and_leading_param() {
        assert_eq!(
            strip_tracking("https://x.com/p?id=7&gclid=abc#section"),
            "https://x.com/p?id=7#section"
        );
    }

    #[test]
    fn strip_tracking_noop_without_tracking() {
        assert_eq!(
            strip_tracking("https://x.com/p?id=7"),
            "https://x.com/p?id=7"
        );
    }

    #[test]
    fn trim_builtin() {
        assert_eq!(apply_builtin(BuiltinTransform::Trim, "  hi  "), "hi");
    }

    #[test]
    fn collapse_whitespace_builtin() {
        assert_eq!(
            apply_builtin(BuiltinTransform::CollapseWhitespace, "a   b\t\nc "),
            "a b c"
        );
    }

    #[test]
    fn case_builtins() {
        assert_eq!(apply_builtin(BuiltinTransform::Uppercase, "aB"), "AB");
        assert_eq!(apply_builtin(BuiltinTransform::Lowercase, "aB"), "ab");
        assert_eq!(
            apply_builtin(BuiltinTransform::TitleCase, "john DOE"),
            "John Doe"
        );
    }

    #[test]
    fn json_pretty_and_minify() {
        let minified = apply_builtin(BuiltinTransform::JsonMinify, "{ \"a\" : 1 }");
        assert_eq!(minified, "{\"a\":1}");
        let pretty = apply_builtin(BuiltinTransform::JsonPretty, "{\"a\":1}");
        assert!(pretty.contains("\n"));
    }

    #[test]
    fn json_invalid_is_unchanged() {
        assert_eq!(
            apply_builtin(BuiltinTransform::JsonPretty, "not json"),
            "not json"
        );
    }

    #[test]
    fn xml_pretty_indents_and_inlines_text() {
        let out = apply_builtin(BuiltinTransform::XmlPretty, "<root><a>hi</a><b/></root>");
        assert_eq!(out, "<root>\n  <a>hi</a>\n  <b/>\n</root>");
    }

    #[test]
    fn xml_pretty_non_xml_unchanged() {
        assert_eq!(
            apply_builtin(BuiltinTransform::XmlPretty, "plain text"),
            "plain text"
        );
    }

    #[test]
    fn base64_roundtrip() {
        let enc = apply_builtin(BuiltinTransform::Base64Encode, "hello");
        assert_eq!(enc, "aGVsbG8=");
        assert_eq!(apply_builtin(BuiltinTransform::Base64Decode, &enc), "hello");
    }

    #[test]
    fn base64_decode_invalid_unchanged() {
        assert_eq!(apply_builtin(BuiltinTransform::Base64Decode, "!!!"), "!!!");
    }

    #[test]
    fn percent_encode_decode_roundtrip() {
        let enc = apply_builtin(BuiltinTransform::UrlEncode, "a b&c=d");
        assert_eq!(enc, "a%20b%26c%3Dd");
        assert_eq!(apply_builtin(BuiltinTransform::UrlDecode, &enc), "a b&c=d");
    }

    #[test]
    fn case_conversions() {
        assert_eq!(
            apply_builtin(BuiltinTransform::SnakeCase, "myXMLValue"),
            "my_xml_value"
        );
        assert_eq!(
            apply_builtin(BuiltinTransform::KebabCase, "Hello World"),
            "hello-world"
        );
        assert_eq!(
            apply_builtin(BuiltinTransform::CamelCase, "hello world-foo"),
            "helloWorldFoo"
        );
    }

    #[test]
    fn strip_html_removes_tags_and_decodes_entities() {
        assert_eq!(
            apply_builtin(BuiltinTransform::StripHtml, "<p>a &amp; <b>b</b></p>"),
            "a & b"
        );
    }

    #[test]
    fn line_ops() {
        assert_eq!(
            apply_builtin(BuiltinTransform::SortLines, "b\na\nc"),
            "a\nb\nc"
        );
        assert_eq!(
            apply_builtin(BuiltinTransform::DedupeLines, "a\nb\na\nc"),
            "a\nb\nc"
        );
        assert_eq!(
            apply_builtin(BuiltinTransform::RemoveEmptyLines, "a\n\n  \nb"),
            "a\nb"
        );
    }

    #[test]
    fn html_encode_escapes_special_chars() {
        assert_eq!(
            apply_builtin(BuiltinTransform::HtmlEncode, "<a href=\"x\">a & b</a>"),
            "&lt;a href=&quot;x&quot;&gt;a &amp; b&lt;/a&gt;"
        );
    }

    #[test]
    fn reverse_lines_builtin() {
        assert_eq!(
            apply_builtin(BuiltinTransform::ReverseLines, "a\nb\nc"),
            "c\nb\na"
        );
    }

    #[test]
    fn trim_lines_builtin() {
        assert_eq!(
            apply_builtin(BuiltinTransform::TrimLines, "  a  \n\tb\t\nc"),
            "a\nb\nc"
        );
    }

    #[test]
    fn normalize_newlines_builtin() {
        assert_eq!(
            apply_builtin(BuiltinTransform::NormalizeNewlines, "a\r\nb\rc\nd"),
            "a\nb\nc\nd"
        );
    }

    #[test]
    fn remove_diacritics_builtin() {
        assert_eq!(
            apply_builtin(BuiltinTransform::RemoveDiacritics, "Crème brûlée Straße"),
            "Creme brulee Strasse"
        );
    }

    #[test]
    fn slugify_builtin() {
        assert_eq!(
            apply_builtin(BuiltinTransform::Slugify, "  Héllo, World! 2 "),
            "hello-world-2"
        );
    }

    #[test]
    fn straighten_quotes_builtin() {
        assert_eq!(
            apply_builtin(
                BuiltinTransform::StraightenQuotes,
                "\u{201C}hi\u{201D} \u{2018}x\u{2019} \u{2014} y\u{2026}"
            ),
            "\"hi\" 'x' - y..."
        );
    }

    #[test]
    fn morph_test_rule_reports_match_and_change() {
        let rule = builtin_rule("upper", r"^\w+$", BuiltinTransform::Uppercase);
        let res = morph_test_rule(rule, "hello".to_string());
        assert!(res.matched);
        assert!(res.changed);
        assert_eq!(res.output, "HELLO");
        assert!(res.error.is_none());
    }

    #[test]
    fn morph_test_rule_reports_no_match() {
        let rule = builtin_rule("upper", r"^\d+$", BuiltinTransform::Uppercase);
        let res = morph_test_rule(rule, "hello".to_string());
        assert!(!res.matched);
        assert!(!res.changed);
        assert_eq!(res.output, "hello");
    }

    #[test]
    fn morph_test_rule_reports_invalid_pattern() {
        let rule = builtin_rule("bad", "[invalid", BuiltinTransform::Trim);
        let res = morph_test_rule(rule, "x".to_string());
        assert!(res.error.is_some());
        assert!(!res.matched);
    }

    #[test]
    fn morph_test_rule_replace_with_groups() {
        let rule = replace_rule("reorder", r"^\S+,\s+\S+$", r"^(\S+),\s+(\S+)$", "$2 $1");
        let res = morph_test_rule(rule, "Doe, John".to_string());
        assert!(res.matched);
        assert_eq!(res.output, "John Doe");
    }

    #[test]
    fn apply_first_picks_first_changing_rule() {
        let rules = compile_rules(&[
            // matches but produces no change -> skipped
            builtin_rule("trim", r"^\s", BuiltinTransform::Trim),
            replace_rule("reorder", r"^\S+,\s+\S+$", r"^(\S+),\s+(\S+)$", "$2 $1"),
        ]);
        let r = apply_first(&rules, "Doe, John").expect("should morph");
        assert_eq!(r.rule_name, "reorder");
        assert_eq!(r.output, "John Doe");
    }

    #[test]
    fn apply_first_returns_none_when_nothing_matches() {
        let rules = compile_rules(&[builtin_rule("upper", r"^\d+$", BuiltinTransform::Uppercase)]);
        assert!(apply_first(&rules, "hello").is_none());
    }

    #[test]
    fn apply_first_skips_matching_but_unchanged_rule() {
        let rules = compile_rules(&[builtin_rule("upper", r".*", BuiltinTransform::Uppercase)]);
        // Already uppercase -> output equals input -> treated as no morph.
        assert!(apply_first(&rules, "ABC").is_none());
    }

    #[test]
    fn compile_rules_skips_disabled_and_invalid() {
        let rules = compile_rules(&[
            MorphRule {
                id: "1".into(),
                name: "disabled".into(),
                enabled: false,
                pattern: ".*".into(),
                action: MorphAction::Builtin {
                    transform: BuiltinTransform::Trim,
                },
            },
            MorphRule {
                id: "2".into(),
                name: "bad-pattern".into(),
                enabled: true,
                pattern: "[invalid".into(),
                action: MorphAction::Builtin {
                    transform: BuiltinTransform::Trim,
                },
            },
            builtin_rule("ok", ".*", BuiltinTransform::Trim),
        ]);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].name, "ok");
    }

    #[test]
    fn rule_serde_roundtrip() {
        let rule = replace_rule("r", "^a", "a", "b");
        let json = serde_json::to_string(&rule).expect("serialize");
        let back: MorphRule = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.name, rule.name);
        assert_eq!(back.action, rule.action);
    }

    #[test]
    fn rule_enabled_defaults_true_when_absent() {
        let json = r#"{"id":"x","name":"x","pattern":".*","action":{"kind":"builtin","transform":"trim"}}"#;
        let rule: MorphRule = serde_json::from_str(json).expect("deserialize");
        assert!(rule.enabled);
    }
}
