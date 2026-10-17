//! Application + proxy/runner settings (mirrors SilverBullet RLSettings.json).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct AppConfig {
    pub general: GeneralSettings,
    pub proxies: ProxySettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GeneralSettings {
    #[serde(default = "default_wait_time")]
    pub wait_time: u64,
    #[serde(default = "default_request_timeout")]
    pub request_timeout: u64,
    #[serde(default)]
    pub max_hits: u64,
    #[serde(default)]
    pub enable_bot_log: bool,
    #[serde(default)]
    pub webhook_enabled: bool,
    #[serde(default)]
    pub webhook_url: String,
    #[serde(default)]
    pub webhook_user: String,
    #[serde(default = "default_true")]
    pub remove_duplicate_wordlist: bool,
    #[serde(default = "default_hit_output_path")]
    pub hit_output_path: String,
}

fn default_hit_output_path() -> String {
    "hits.txt".to_string()
}

fn default_wait_time() -> u64 {
    0
}
fn default_request_timeout() -> u64 {
    10
}
fn default_true() -> bool {
    true
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            wait_time: default_wait_time(),
            request_timeout: default_request_timeout(),
            max_hits: 0,
            enable_bot_log: false,
            webhook_enabled: false,
            webhook_url: String::new(),
            webhook_user: String::new(),
            remove_duplicate_wordlist: true,
            hit_output_path: default_hit_output_path(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ProxySettings {
    #[serde(default)]
    pub concurrent_use: bool,
    #[serde(default)]
    pub never_ban: bool,
    #[serde(default = "default_ban_loop_evasion")]
    pub ban_loop_evasion: u32,
    #[serde(default)]
    pub shuffle_on_start: bool,
    #[serde(default)]
    pub reload: bool,
    #[serde(default)]
    pub reload_interval: u64, // minutes
    #[serde(default)]
    pub always_get_clearance: bool,
    #[serde(default)]
    pub global_ban_keys: Vec<String>,
    #[serde(default)]
    pub global_retry_keys: Vec<String>,
    #[serde(default)]
    pub remote_proxy_sources: Vec<RemoteProxySource>,
}

fn default_ban_loop_evasion() -> u32 {
    100
}

impl Default for ProxySettings {
    fn default() -> Self {
        Self {
            concurrent_use: false,
            never_ban: false,
            ban_loop_evasion: default_ban_loop_evasion(),
            shuffle_on_start: false,
            reload: false,
            reload_interval: 0,
            always_get_clearance: false,
            global_ban_keys: Vec::new(),
            global_retry_keys: Vec::new(),
            remote_proxy_sources: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct RemoteProxySource {
    pub id: String,
    pub active: bool,
    pub url: String,
    pub proxy_type: String, // http | socks4 | socks5
    pub pattern: String,
    pub output: String, // e.g. "[1]" capture group
}

/// Proxy types matching Extreme.Net ProxyType enum values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ProxyKind {
    Http,
    Socks4,
    Socks4a,
    Socks5,
}

impl ProxyKind {
    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "socks5" => Self::Socks5,
            "socks4a" => Self::Socks4a,
            "socks4" => Self::Socks4,
            _ => Self::Http,
        }
    }
}
