//! LoliScript parser — config sections + block models.

pub mod condition;
pub mod parser;

pub use parser::{parse_script, parse_svb};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Parsed .svb config: settings + script text.
#[derive(Debug, Clone)]
pub struct SvbConfig {
    pub settings: ConfigSettings,
    pub script: String,
}

/// ConfigSettings mirrors SilverBullet's JSON `[SETTINGS]` section.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct ConfigSettings {
    #[serde(default)]
    pub name: String,
    #[serde(default = "one")]
    pub suggested_bots: u32,
    #[serde(default)]
    pub max_cpm: u32,
    #[serde(default)]
    pub save_empty_captures: bool,
    #[serde(default)]
    pub continue_on_custom: bool,
    #[serde(default)]
    pub save_hits_to_text_file: bool,
    #[serde(default)]
    pub ignore_response_errors: bool,
    #[serde(default = "eight")]
    pub max_redirects: u32,
    #[serde(default)]
    pub needs_proxies: bool,
    #[serde(default)]
    pub only_socks: bool,
    #[serde(default)]
    pub only_ssl: bool,
    #[serde(default)]
    pub max_proxy_uses: u32,
    #[serde(default)]
    pub ban_proxy_after_good_status: bool,
    #[serde(default = "minus_one")]
    pub ban_loop_evasion_override: i32,
    #[serde(default)]
    pub allowed_wordlist1: String,
    #[serde(default)]
    pub allowed_wordlist2: String,
    #[serde(default)]
    pub is_base64: bool,
    #[serde(default)]
    pub filter_list: Vec<String>,
    #[serde(default, deserialize_with = "string_or_number")]
    pub security_protocol: String,
    #[serde(default = "true_bool")]
    pub accept_insecure_certificates: bool,
    #[serde(default)]
    pub custom_user_agent: String,
    #[serde(default)]
    pub required_plugins: Vec<String>,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub last_modified: String,
    #[serde(default)]
    pub hit_info_format: String,
}

fn string_or_number<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let val: serde_json::Value = serde::Deserialize::deserialize(deserializer)?;
    match val {
        serde_json::Value::String(s) => Ok(s),
        serde_json::Value::Number(n) => Ok(n.to_string()),
        serde_json::Value::Null => Ok(String::new()),
        _ => Err(D::Error::custom("expected string or number")),
    }
}

fn one() -> u32 {
    1
}
fn eight() -> u32 {
    8
}
fn minus_one() -> i32 {
    -1
}
fn true_bool() -> bool {
    true
}

/// A single LoliScript block (parsed).
#[derive(Debug, Clone)]
pub enum Block {
    Request(Box<RequestBlock>),
    Keycheck(KeycheckBlock),
    Parse(ParseBlock),
    Function(FunctionBlock),
    Utility(UtilityBlock),
    Tcp(TcpBlock),
    /// Unrecognized / browser / captcha — skipped by HTTP-only engine.
    Unsupported {
        kind: String,
        line: String,
    },
}

#[derive(Debug, Clone, Default)]
pub struct RequestBlock {
    pub label: Option<String>,
    pub disabled: bool,
    pub method: String,
    pub url: String,
    pub content_type: String,
    pub post_data: String,
    pub raw_data: String,
    pub headers: HashMap<String, String>,
    pub cookies: HashMap<String, String>,
    pub auth_user: String,
    pub auth_pass: String,
    pub auto_redirect: bool,
    pub read_response_source: bool,
    pub accept_encoding: bool,
    pub encode_content: bool,
    pub protocol_version: String,
    pub security_protocol: String,
    pub output_variable: String,
    pub save_as_file: bool,
    pub download_path: String,
}

#[derive(Debug, Clone, Default)]
pub struct KeycheckBlock {
    pub label: Option<String>,
    pub disabled: bool,
    pub ban_on_4xx: bool,
    pub ban_on_to_check: bool,
    pub keychains: Vec<Keychain>,
}

#[derive(Debug, Clone)]
pub struct Keychain {
    pub kind: KeychainType,
    pub mode: KeychainMode,
    pub custom_type: String,
    pub keys: Vec<KeyEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeychainType {
    Success,
    Failure,
    Ban,
    Retry,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeychainMode {
    Or,
    And,
}

#[derive(Debug, Clone)]
pub struct KeyEntry {
    pub left: String,
    pub comparer: Comparer,
    pub right: String,
}

/// Condition comparers (from RuriLib.Functions.Conditions.Comparer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comparer {
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterOrEqual,
    EqualTo,
    NotEqualTo,
    Contains,
    DoesNotContain,
    Exists,
    DoesNotExist,
    MatchesRegex,
    DoesNotMatchRegex,
}

impl Comparer {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "LessThan" => Some(Self::LessThan),
            "LessThanOrEqual" => Some(Self::LessThanOrEqual),
            "GreaterThan" => Some(Self::GreaterThan),
            "GreaterOrEqual" => Some(Self::GreaterOrEqual),
            "EqualTo" => Some(Self::EqualTo),
            "NotEqualTo" => Some(Self::NotEqualTo),
            "Contains" => Some(Self::Contains),
            "DoesNotContain" => Some(Self::DoesNotContain),
            "Exists" => Some(Self::Exists),
            "DoesNotExist" => Some(Self::DoesNotExist),
            "MatchesRegex" => Some(Self::MatchesRegex),
            "DoesNotMatchRegex" => Some(Self::DoesNotMatchRegex),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ParseBlock {
    pub label: Option<String>,
    pub disabled: bool,
    pub input: String,
    pub mode: String, // LR | JSON | CSS | Regex
    pub left: String,
    pub right: String,
    pub output: String, // VAR | CAP
    pub name: String,
}

#[derive(Debug, Clone, Default)]
pub struct FunctionBlock {
    pub label: Option<String>,
    pub disabled: bool,
    pub function: String, // e.g. URLENCODE, MD5, BASE64ENCODE
    pub input: String,
    pub output: String,
    pub output_var: String,
}

#[derive(Debug, Clone, Default)]
pub struct UtilityBlock {
    pub label: Option<String>,
    pub disabled: bool,
    pub action: String,
}

#[derive(Debug, Clone, Default)]
pub struct TcpBlock {
    pub label: Option<String>,
    pub disabled: bool,
    pub host: String,
    pub port: u16,
    pub send: String,
}

/// Bot status codes (from RuriLib BotStatus).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BotStatus {
    Success,
    Fail,
    Custom,
    Ban,
    Retry,
    Error,
    None,
}

/// Variable list (captures).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VariableList {
    pub all: Vec<CVar>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CVar {
    pub name: String,
    pub value: String,
}

impl Block {
    pub fn disabled(&self) -> bool {
        match self {
            Block::Request(rb) => rb.disabled,
            Block::Keycheck(kb) => kb.disabled,
            Block::Parse(pb) => pb.disabled,
            Block::Function(fb) => fb.disabled,
            Block::Utility(ub) => ub.disabled,
            Block::Tcp(tb) => tb.disabled,
            Block::Unsupported { .. } => false,
        }
    }
}

impl VariableList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.all
            .iter()
            .find(|v| v.name.eq_ignore_ascii_case(name))
            .map(|v| v.value.as_str())
    }

    pub fn set(&mut self, name: &str, value: String) {
        if let Some(v) = self
            .all
            .iter_mut()
            .find(|v| v.name.eq_ignore_ascii_case(name))
        {
            v.value = value;
        } else {
            self.all.push(CVar {
                name: name.to_string(),
                value,
            });
        }
    }

    pub fn to_capture_string(&self) -> String {
        self.all
            .iter()
            .map(|v| format!("{}={}", v.name, v.value))
            .collect::<Vec<_>>()
            .join(" ")
    }
}
