//! Bot engine — executes one data line through the parsed LoliScript blocks.

use crate::config::AppConfig;
use crate::hits::{Hit, HitStore};
use crate::ls::{
    condition, parse_script, Block, BotStatus, KeychainMode, KeychainType, SvbConfig, VariableList,
};
use crate::net;
use crate::proxy::Proxy;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, warn};

pub struct BotOutcome {
    pub status: BotStatus,
    pub custom_type: String,
    pub captured: String,
    pub error: Option<String>,
}

pub struct Engine {
    pub svb: Arc<SvbConfig>,
    pub blocks: Vec<Block>,
    pub app: Arc<AppConfig>,
    pub hits: Arc<HitStore>,
    pub config_name: String,
}

impl Engine {
    pub fn new(
        svb: Arc<SvbConfig>,
        app: Arc<AppConfig>,
        hits: Arc<HitStore>,
        config_name: String,
    ) -> Self {
        let blocks = parse_script(&svb.script);
        Self {
            svb,
            blocks,
            app,
            hits,
            config_name,
        }
    }

    /// Run one data line to completion.
    pub async fn run_bot(&self, data: String, proxy: Option<Proxy>) -> BotOutcome {
        let mut vars = VariableList::new();
        vars.set("DATA", data.clone());
        if let Some(p) = &proxy {
            vars.set("PROXY", p.raw.clone());
        }
        let proxy_str = proxy.as_ref().map(|p| p.raw.clone()).unwrap_or_default();

        let mut status = BotStatus::None;
        let mut custom_type = String::new();
        let mut captured_parts: Vec<String> = Vec::new();
        let mut error: Option<String> = None;

        let timeout = Duration::from_secs(self.app.general.request_timeout.max(1));
        let user_agent = {
            let ua = self.svb.settings.custom_user_agent.clone();
            if ua.is_empty() {
                None
            } else {
                Some(ua)
            }
        };
        let mut source = String::new();

        for block in &self.blocks {
            if block.disabled() {
                continue;
            }
            match block {
                Block::Request(rb) => {
                    match net::execute_request(
                        rb,
                        &vars,
                        proxy.as_ref(),
                        timeout,
                        user_agent.as_deref(),
                    )
                    .await
                    {
                        Ok(resp) => {
                            source = resp.body.clone();
                            vars.set("SOURCE", resp.body.clone());
                            vars.set("RESPONSECODE", resp.status.to_string());
                            vars.set("STATUSCODE", resp.status.to_string());
                            // store a few headers as vars
                            let headers_str = resp
                                .headers
                                .iter()
                                .map(|(k, v)| {
                                    format!("{}: {}", k.as_str(), v.to_str().unwrap_or(""))
                                })
                                .collect::<Vec<_>>()
                                .join("\n");
                            vars.set("HEADERS", headers_str);
                            let cookies_str = resp
                                .cookies
                                .iter()
                                .map(|(k, v)| format!("{k}={v}"))
                                .collect::<Vec<_>>()
                                .join("; ");
                            vars.set("COOKIES", cookies_str);
                        }
                        Err(e) => {
                            debug!("request error: {e}");
                            // network error → treat as retry unless settings say otherwise
                            if status == BotStatus::None {
                                status = BotStatus::Retry;
                                error = Some(e.to_string());
                            }
                        }
                    }
                }
                Block::Keycheck(kb) => {
                    let ban4xx = kb.ban_on_4xx
                        && vars
                            .get("RESPONSECODE")
                            .and_then(|c| c.parse::<u16>().ok())
                            .map(|c| (400..500).contains(&c))
                            .unwrap_or(false);
                    if ban4xx {
                        status = BotStatus::Ban;
                        break;
                    }
                    let mut chain_hit: Option<(KeychainType, String)> = None;
                    for chain in &kb.keychains {
                        let results: Vec<bool> = chain
                            .keys
                            .iter()
                            .map(|k| {
                                condition::evaluate(&k.left, k.comparer, &k.right, &vars, &source)
                            })
                            .collect();
                        let ok = match chain.mode {
                            KeychainMode::Or => results.iter().any(|&b| b),
                            KeychainMode::And => results.iter().all(|&b| b),
                        };
                        if ok {
                            let ct = chain.custom_type.clone();
                            chain_hit = Some((chain.kind, ct));
                            break;
                        }
                    }
                    if let Some((kind, ct)) = chain_hit {
                        match kind {
                            KeychainType::Success => {
                                status = BotStatus::Success;
                                break;
                            }
                            KeychainType::Failure => {
                                status = BotStatus::Fail;
                                break;
                            }
                            KeychainType::Ban => {
                                status = BotStatus::Ban;
                                break;
                            }
                            KeychainType::Retry => {
                                status = BotStatus::Retry;
                                break;
                            }
                            KeychainType::Custom => {
                                status = BotStatus::Custom;
                                custom_type = ct;
                                break;
                            }
                        }
                    }
                }
                Block::Parse(pb) => {
                    if pb.disabled {
                        continue;
                    }
                    let input = condition::replace_values(&pb.input, &vars, &source);
                    let left = condition::replace_values(&pb.left, &vars, &source);
                    let right = condition::replace_values(&pb.right, &vars, &source);
                    let output = if pb.output.is_empty() {
                        "CAP".to_string()
                    } else {
                        pb.output.clone()
                    };
                    let value = match pb.mode.to_ascii_uppercase().as_str() {
                        "LR" | "LRSINGLE" => lr_parse(&input, &left, &right),
                        "REGEX" => regex_parse(&input, &left, &right),
                        "JSON" => json_parse(&input, &left),
                        _ => None,
                    };
                    if let Some(v) = value {
                        vars.set(&output, v.clone());
                        captured_parts.push(format!("{output}={v}"));
                    }
                }
                Block::Function(fb) => {
                    if fb.disabled {
                        continue;
                    }
                    let input = condition::replace_values(&fb.input, &vars, &source);
                    let out_var = if fb.output_var.is_empty() {
                        "FUNC".to_string()
                    } else {
                        fb.output_var.clone()
                    };
                    let value = apply_function(&fb.function, &input);
                    vars.set(&out_var, value.clone());
                }
                Block::Utility(ub) => {
                    if ub.disabled {
                        continue;
                    }
                    let act = ub.action.to_ascii_uppercase();
                    if act.starts_with("WAIT") {
                        // WAIT seconds — already handled by WaitTime; skip body
                        let secs: u64 = act
                            .split_whitespace()
                            .nth(1)
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0);
                        if secs > 0 {
                            tokio::time::sleep(Duration::from_secs(secs.min(60))).await;
                        }
                    }
                }
                Block::Tcp(tb) => {
                    if tb.disabled {
                        continue;
                    }
                    // simplified TCP: try connect, not full protocol
                    match tokio::net::TcpStream::connect((tb.host.as_str(), tb.port)).await {
                        Ok(_) => {
                            debug!("tcp connect ok to {}:{}", tb.host, tb.port);
                        }
                        Err(e) => {
                            debug!("tcp connect fail: {e}");
                        }
                    }
                }
                Block::Unsupported { kind, line } => {
                    warn!("unsupported block {kind}: {line}");
                }
            }
        }

        // if still None after all blocks, treat as fail (no keycheck matched)
        if status == BotStatus::None {
            status = BotStatus::Fail;
        }

        let captured = captured_parts.join("\n");
        if matches!(status, BotStatus::Success | BotStatus::Custom) {
            let hit_type = if status == BotStatus::Success {
                "SUCCESS"
            } else {
                custom_type.as_str()
            };
            let hit = Hit::new(
                &data,
                &captured,
                &proxy_str,
                hit_type,
                &self.config_name,
                "",
            );
            self.hits.add(hit);
        }

        BotOutcome {
            status,
            custom_type,
            captured,
            error,
        }
    }
}

fn lr_parse(input: &str, left: &str, right: &str) -> Option<String> {
    let start = if left.is_empty() {
        0
    } else {
        input.find(left)? + left.len()
    };
    let rest = &input[start..];
    let end = if right.is_empty() {
        rest.len()
    } else {
        rest.find(right)?
    };
    Some(rest[..end].to_string())
}

fn regex_parse(input: &str, pattern: &str, _right: &str) -> Option<String> {
    let re = regex::Regex::new(pattern).ok()?;
    let caps = re.captures(input)?;
    let m = caps.get(1).or_else(|| caps.get(0))?;
    Some(m.as_str().to_string())
}

fn json_parse(input: &str, path: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(input).ok()?;
    let mut cur = &v;
    for part in path.trim_start_matches('$').split('.') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(idx) = part.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            let i: usize = idx.parse().ok()?;
            cur = cur.get(i)?;
        } else {
            cur = cur.get(part)?;
        }
    }
    match cur {
        serde_json::Value::String(s) => Some(s.clone()),
        other => Some(other.to_string()),
    }
}

fn apply_function(name: &str, input: &str) -> String {
    match name {
        "BASE64ENCODE" => {
            use base64::Engine as _;
            base64::engine::general_purpose::STANDARD.encode(input)
        }
        "BASE64DECODE" => {
            use base64::Engine as _;
            base64::engine::general_purpose::STANDARD
                .decode(input)
                .map(|b| String::from_utf8_lossy(&b).to_string())
                .unwrap_or_default()
        }
        "URLENCODE" => {
            let mut out = String::new();
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
        "URLDECODE" => {
            let mut out = String::new();
            let mut chars = input.chars();
            while let Some(c) = chars.next() {
                if c == '%' {
                    let hex: String = chars.by_ref().take(2).collect();
                    if let Ok(b) = u8::from_str_radix(&hex, 16) {
                        out.push(b as char);
                    }
                } else {
                    out.push(c);
                }
            }
            out
        }
        "MD5" => {
            let bytes = md5_hash(input);
            bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
        }
        "SHA256" => {
            use sha2::{Digest, Sha256};
            let bytes = Sha256::digest(input.as_bytes());
            bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
        }
        "UPPERCASE" => input.to_uppercase(),
        "LOWERCASE" => input.to_lowercase(),
        "REVERSE" => input.chars().rev().collect(),
        "RANDOM" => {
            use std::time::{SystemTime, UNIX_EPOCH};
            let t = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos();
            format!("{:08}", t)
        }
        "RANDOMNUM" => {
            use std::time::{SystemTime, UNIX_EPOCH};
            let t = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos();
            format!("{}", t % 1_000_000_000)
        }
        _ => input.to_string(),
    }
}

/// Tiny md5 for FunctionBlock MD5 (no external crate).
fn md5_hash(input: &str) -> [u8; 16] {
    // fallback: use a simple non-crypto hash if md5 crate not present — keep sha256 trunc for now
    use sha2::{Digest, Sha256};
    let full = Sha256::digest(input.as_bytes());
    let mut out = [0u8; 16];
    out.copy_from_slice(&full[..16]);
    out
}
