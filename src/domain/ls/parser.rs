//! LoliScript line parser — script text → Vec<Block>.
//!
//! Grammar (from RuriLib BlockParser + LineParser):
//! - Optional `!` prefix = disabled
//! - Optional `#label ` prefix = jump label
//! - First token = block type (REQUEST, KEYCHECK, PARSE, FUNCTION, UTILITY, TCP, ...)
//! - Subsequent tokens = inline params
//! - Indented lines (leading space/tab) = continuation of previous block (CONTENT, HEADER, COOKIE, KEYCHAIN, KEY, ...)

use super::*;
use once_cell::sync::Lazy;
use regex::Regex;

#[allow(dead_code)]
static FIRST_TOKEN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^!?(#[^ ]* )?([^ ]*)").expect("FIRST_TOKEN must be a valid regular expression")
});

/// Parse a full .svb file (SETTINGS + SCRIPT sections).
pub fn parse_svb(text: &str) -> anyhow::Result<SvbConfig> {
    let mut settings_json = String::new();
    let mut script = String::new();
    let mut section = String::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "[SETTINGS]" {
            section = "SETTINGS".into();
            continue;
        }
        if trimmed == "[SCRIPT]" {
            section = "SCRIPT".into();
            continue;
        }
        match section.as_str() {
            "SETTINGS" => {
                settings_json.push_str(line);
                settings_json.push('\n');
            }
            "SCRIPT" => {
                script.push_str(line);
                script.push('\n');
            }
            _ => {}
        }
    }

    let settings: ConfigSettings = if settings_json.trim().is_empty() {
        ConfigSettings::default()
    } else {
        serde_json::from_str(&settings_json)?
    };

    Ok(SvbConfig { settings, script })
}

/// Parse the SCRIPT text into blocks.
pub fn parse_script(script: &str) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    // (block_index, sub_kind) — sub_kind tracks which sub-block builder we're in
    let mut current: Option<RawBlock> = None;

    for raw_line in script.lines() {
        if raw_line.trim().is_empty() {
            continue;
        }
        let is_indent = raw_line.starts_with(' ') || raw_line.starts_with('\t');
        let is_implicit_continuation = current
            .as_ref()
            .is_some_and(|block| is_continuation(&block.kind, raw_line));
        if is_indent || is_implicit_continuation {
            let sub = raw_line.trim();
            if let Some(rb) = current.as_mut() {
                rb.push_sub(sub);
            }
            continue;
        }

        // flush previous
        if let Some(rb) = current.take() {
            blocks.push(rb.finish());
        }

        let line = raw_line.trim_end();
        let disabled = line.starts_with('!');
        let mut rest = if disabled { &line[1..] } else { line };
        let mut label = None;
        if rest.starts_with('#') {
            let mut parts = rest[1..].splitn(2, ' ');
            label = Some(parts.next().unwrap_or("").to_string());
            rest = parts.next().unwrap_or("");
        }
        let rest = rest.trim_start();
        if rest.is_empty() {
            continue;
        }
        let mut tokens = rest.splitn(2, ' ');
        let kind = tokens.next().unwrap_or("").to_ascii_uppercase();
        let inline = tokens.next().unwrap_or("").to_string();

        let rb = RawBlock {
            kind: kind.clone(),
            disabled,
            label,
            inline,
            subs: Vec::new(),
        };
        // Some blocks are single-line (no subs needed for finish to succeed)
        match kind.as_str() {
            "REQUEST" | "KEYCHECK" | "PARSE" | "FUNCTION" | "UTILITY" | "TCP" | "WAIT" => {}
            _ => {
                // unsupported — still record as block so engine can skip
            }
        }
        current = Some(rb);
    }
    if let Some(rb) = current.take() {
        blocks.push(rb.finish());
    }
    blocks
}

struct RawBlock {
    kind: String,
    disabled: bool,
    label: Option<String>,
    inline: String,
    subs: Vec<String>,
}

impl RawBlock {
    fn push_sub(&mut self, sub: &str) {
        self.subs.push(sub.to_string());
    }

    fn finish(self) -> Block {
        let label = self.label;
        let disabled = self.disabled;
        let inline = self.inline.trim().to_string();
        let kind_upper = self.kind.to_ascii_uppercase();

        match kind_upper.as_str() {
            "REQUEST" => Block::Request(Box::new(parse_request(
                &label, disabled, &inline, &self.subs,
            ))),
            "KEYCHECK" => Block::Keycheck(parse_keycheck(&label, disabled, &inline, &self.subs)),
            "PARSE" => Block::Parse(parse_parse(&label, disabled, &inline)),
            "FUNCTION" => Block::Function(parse_function(&label, disabled, &inline)),
            "UTILITY" => Block::Utility(parse_utility(&label, disabled, &inline)),
            "TCP" => Block::Tcp(parse_tcp(&label, disabled, &inline, &self.subs)),
            _ => Block::Unsupported {
                kind: kind_upper.clone(),
                line: format!("{kind_upper} {inline}"),
            },
        }
    }
}

/// Accept common continuation directives without indentation as a convenience.
/// Indentation remains the canonical LoliScript form; this only makes hand-written
/// configurations less surprising and preserves the current request block.
fn is_continuation(parent_kind: &str, raw_line: &str) -> bool {
    let keyword = raw_line
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();

    match parent_kind {
        "REQUEST" => matches!(
            keyword.as_str(),
            "CONTENT" | "RAW" | "HEADER" | "COOKIE" | "BASICAUTH" | "FILE"
        ),
        "KEYCHECK" => matches!(keyword.as_str(), "KEYCHAIN" | "KEY"),
        "TCP" => matches!(keyword.as_str(), "SEND" | "RECEIVE"),
        _ => false,
    }
}

/// Parse REQUEST inline args: METHOD "URL" [STANDARD/MULTIPART/BASICAUTH] [flags...]
fn parse_request(
    label: &Option<String>,
    disabled: bool,
    inline: &str,
    subs: &[String],
) -> RequestBlock {
    let mut rb = RequestBlock {
        label: label.clone(),
        disabled,
        auto_redirect: true,
        read_response_source: true,
        accept_encoding: true,
        ..Default::default()
    };

    let tokens = tokenize(inline);
    if tokens.is_empty() {
        return rb;
    }
    rb.method = tokens[0].to_ascii_uppercase();
    if tokens.len() > 1 {
        rb.url = tokens[1].clone();
    }
    for t in &tokens[2..] {
        match t.to_ascii_uppercase().as_str() {
            "MULTIPART" => rb.content_type = "multipart/form-data".into(),
            "BASICAUTH" => rb.content_type = "basic".into(),
            "NOREDIRECT" => rb.auto_redirect = false,
            "NORESPONSE" => rb.read_response_source = false,
            _ => {}
        }
    }

    for sub in subs {
        let sub = sub.trim();
        let mut parts = sub.splitn(2, ' ');
        let kw = parts.next().unwrap_or("").to_ascii_uppercase();
        let val = parts.next().unwrap_or("").trim().to_string();
        let val = unquote(&val);
        match kw.as_str() {
            "HEADER" => {
                if let Some((k, v)) = val.split_once(':') {
                    rb.headers
                        .insert(k.trim().to_string(), v.trim().to_string());
                }
            }
            "COOKIE" => {
                if let Some((k, v)) = val.split_once('=') {
                    rb.cookies
                        .insert(k.trim().to_string(), v.trim().to_string());
                }
            }
            "CONTENT" => {
                rb.post_data = val;
                if rb.content_type.is_empty() {
                    rb.content_type = "application/x-www-form-urlencoded".into();
                }
            }
            "RAW" => {
                rb.raw_data = val;
                if rb.content_type.is_empty() {
                    rb.content_type = "application/octet-stream".into();
                }
            }
            _ => {}
        }
    }
    rb
}

/// Parse KEYCHECK sub-lines: KEYCHAIN Success|Failure|Ban|Retry|Custom OR|AND, then KEY lines.
fn parse_keycheck(
    label: &Option<String>,
    disabled: bool,
    inline: &str,
    subs: &[String],
) -> KeycheckBlock {
    let mut kb = KeycheckBlock {
        label: label.clone(),
        disabled,
        ..Default::default()
    };

    // inline flags: BANON4XX, BANONTOCHECK
    for t in tokenize(inline) {
        match t.to_ascii_uppercase().as_str() {
            "BANON4XX" => kb.ban_on_4xx = true,
            "BANONTOCHECK" => kb.ban_on_to_check = true,
            _ => {}
        }
    }

    let mut current_chain: Option<Keychain> = None;
    for sub in subs {
        let sub = sub.trim();
        let tokens = tokenize(sub);
        if tokens.is_empty() {
            continue;
        }
        let kw = tokens[0].to_ascii_uppercase();
        match kw.as_str() {
            "KEYCHAIN" => {
                if let Some(c) = current_chain.take() {
                    kb.keychains.push(c);
                }
                let kind_str = tokens.get(1).map(|s| s.as_str()).unwrap_or("");
                let mode_str = tokens.get(2).map(|s| s.as_str()).unwrap_or("OR");
                let kind = match kind_str.to_ascii_uppercase().as_str() {
                    "SUCCESS" => KeychainType::Success,
                    "FAILURE" => KeychainType::Failure,
                    "BAN" => KeychainType::Ban,
                    "RETRY" => KeychainType::Retry,
                    "CUSTOM" => KeychainType::Custom,
                    _ => KeychainType::Failure,
                };
                let mode = if mode_str.eq_ignore_ascii_case("AND") {
                    KeychainMode::And
                } else {
                    KeychainMode::Or
                };
                let custom_type = if kind == KeychainType::Custom {
                    tokens.get(3).cloned().unwrap_or_default()
                } else {
                    String::new()
                };
                current_chain = Some(Keychain {
                    kind,
                    mode,
                    custom_type,
                    keys: Vec::new(),
                });
            }
            "KEY" => {
                // KEY "left" Comparer "right"
                let rest: Vec<String> = tokens[1..].to_vec();
                let key = parse_key(&rest);
                if let Some(c) = current_chain.as_mut() {
                    c.keys.push(key);
                }
            }
            _ => {}
        }
    }
    if let Some(c) = current_chain.take() {
        kb.keychains.push(c);
    }
    kb
}

fn parse_key(tokens: &[String]) -> KeyEntry {
    // tokens: ["left", "Comparer", "right..."] — right may be multi-token if unquoted
    if tokens.is_empty() {
        return KeyEntry {
            left: "<SOURCE>".into(),
            comparer: Comparer::Contains,
            right: String::new(),
        };
    }
    let left = unquote(&tokens[0]);
    if tokens.len() < 3 {
        // only left + comparer? right empty
        let comparer = Comparer::parse(tokens.get(1).map(|s| s.as_str()).unwrap_or("Contains"))
            .unwrap_or(Comparer::Contains);
        return KeyEntry {
            left,
            comparer,
            right: String::new(),
        };
    }
    let comparer = Comparer::parse(&tokens[1]).unwrap_or(Comparer::Contains);
    let right = if tokens.len() > 2 {
        // join remaining as right (in case of unquoted spaces)
        let r = tokens[2..].join(" ");
        unquote(&r)
    } else {
        String::new()
    };
    KeyEntry {
        left,
        comparer,
        right,
    }
}

fn parse_parse(label: &Option<String>, disabled: bool, inline: &str) -> ParseBlock {
    // PARSE input mode left right -> output name
    let tokens = tokenize(inline);
    let mut pb = ParseBlock {
        label: label.clone(),
        disabled,
        ..Default::default()
    };
    if tokens.is_empty() {
        return pb;
    }
    pb.input = tokens[0].clone();
    pb.mode = tokens.get(1).cloned().unwrap_or_default();
    pb.left = tokens.get(2).cloned().unwrap_or_default();
    // right may include "->" and output
    let mut right_parts: Vec<String> = tokens[3.min(tokens.len())..].to_vec();
    // find "->"
    if let Some(idx) = right_parts.iter().position(|t| t == "->") {
        pb.right = right_parts[..idx].join(" ");
        pb.output = right_parts.get(idx + 1).cloned().unwrap_or_default();
        pb.name = right_parts.get(idx + 2).cloned().unwrap_or_default();
        right_parts.clear();
    } else {
        pb.right = right_parts.join(" ");
    }
    pb
}

fn parse_function(label: &Option<String>, disabled: bool, inline: &str) -> FunctionBlock {
    // FUNCTION FuncName input -> output var
    let tokens = tokenize(inline);
    let mut fb = FunctionBlock {
        label: label.clone(),
        disabled,
        ..Default::default()
    };
    if tokens.is_empty() {
        return fb;
    }
    fb.function = tokens[0].clone();
    let rest = &tokens[1..];
    if let Some(idx) = rest.iter().position(|t| t == "->") {
        fb.input = rest[..idx].join(" ");
        fb.output = rest.get(idx + 1).cloned().unwrap_or_default();
        fb.output_var = rest.get(idx + 2).cloned().unwrap_or_default();
    } else {
        fb.input = rest.join(" ");
    }
    fb
}

fn parse_utility(label: &Option<String>, disabled: bool, inline: &str) -> UtilityBlock {
    UtilityBlock {
        label: label.clone(),
        disabled,
        action: inline.trim().to_string(),
    }
}

fn parse_tcp(label: &Option<String>, disabled: bool, inline: &str, subs: &[String]) -> TcpBlock {
    let tokens = tokenize(inline);
    let mut tb = TcpBlock {
        label: label.clone(),
        disabled,
        ..Default::default()
    };
    if tokens.len() >= 2 {
        tb.host = unquote(&tokens[0]);
        tb.port = tokens[1].parse().unwrap_or(0);
    }
    for sub in subs {
        let sub = sub.trim();
        let mut parts = sub.splitn(2, ' ');
        let kw = parts.next().unwrap_or("").to_ascii_uppercase();
        let val = parts.next().unwrap_or("").trim().to_string();
        if kw == "SEND" {
            tb.send = unquote(&val);
        }
    }
    tb
}

/// Tokenize a string into quoted/unquoted tokens.
fn tokenize(s: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let chars = s.chars();
    let mut cur = String::new();
    let mut in_quote = false;
    for c in chars {
        match c {
            '"' => {
                in_quote = !in_quote;
            }
            c if c.is_whitespace() && !in_quote => {
                if !cur.is_empty() {
                    tokens.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    tokens
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_request() {
        let script = r#"REQUEST GET "https://example.com"
HEADER "Accept: text/html""#;
        let blocks = parse_script(script);
        assert_eq!(blocks.len(), 1);
        if let Block::Request(r) = &blocks[0] {
            assert_eq!(r.method, "GET");
            assert_eq!(r.url, "https://example.com");
            assert!(r.headers.contains_key("Accept"));
        } else {
            panic!("expected Request");
        }
    }

    #[test]
    fn test_parse_keycheck() {
        let script = r#"KEYCHECK
  KEYCHAIN Success OR
    KEY "<SOURCE>" Contains "Welcome"
  KEYCHAIN Failure OR
    KEY "<SOURCE>" Contains "Invalid""#;
        let blocks = parse_script(script);
        assert_eq!(blocks.len(), 1);
        if let Block::Keycheck(k) = &blocks[0] {
            assert_eq!(k.keychains.len(), 2);
            assert_eq!(k.keychains[0].kind, KeychainType::Success);
            assert_eq!(k.keychains[0].keys.len(), 1);
            assert_eq!(k.keychains[0].keys[0].right, "Welcome");
        } else {
            panic!("expected Keycheck");
        }
    }
}
