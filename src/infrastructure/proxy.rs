//! Proxy pool — parse, rotation, ban tracking.

use crate::config::ProxyKind;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Proxy {
    pub kind: ProxyKind,
    pub host: String,
    pub port: u16,
    pub user: Option<String>,
    pub pass: Option<String>,
    pub raw: String,
}

impl Proxy {
    pub fn parse_line(line: &str) -> Option<Self> {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return None;
        }
        // scheme://host:port or scheme://user:pass@host:port
        if let Some(scheme_rest) = line.split_once("://") {
            let kind = ProxyKind::from_str_lossy(scheme_rest.0);
            let rest = scheme_rest.1;
            let (auth, hostport) = match rest.rsplit_once('@') {
                Some((a, hp)) => (Some(a), hp),
                None => (None, rest),
            };
            let (host, port) = hostport.rsplit_once(':')?;
            let port: u16 = port.parse().ok()?;
            let (user, pass) = match auth {
                Some(a) => {
                    if let Some((u, p)) = a.split_once(':') {
                        (Some(u.to_string()), Some(p.to_string()))
                    } else {
                        (Some(a.to_string()), None)
                    }
                }
                None => (None, None),
            };
            return Some(Self {
                kind,
                host: host.to_string(),
                port,
                user,
                pass,
                raw: line.to_string(),
            });
        }
        // host:port or host:port:user:pass
        let parts: Vec<&str> = line.split(':').collect();
        match parts.len() {
            2 => {
                let port: u16 = parts[1].parse().ok()?;
                Some(Self {
                    kind: ProxyKind::Http,
                    host: parts[0].to_string(),
                    port,
                    user: None,
                    pass: None,
                    raw: line.to_string(),
                })
            }
            4 => {
                let port: u16 = parts[1].parse().ok()?;
                Some(Self {
                    kind: ProxyKind::Http,
                    host: parts[0].to_string(),
                    port,
                    user: Some(parts[2].to_string()),
                    pass: Some(parts[3].to_string()),
                    raw: line.to_string(),
                })
            }
            _ => None,
        }
    }

    /// reqwest Proxy for this proxy.
    pub fn to_reqwest(&self) -> anyhow::Result<reqwest::Proxy> {
        let url = match self.kind {
            ProxyKind::Http => format!("http://{}:{}", self.host, self.port),
            ProxyKind::Socks4 | ProxyKind::Socks4a => {
                format!("socks4://{}:{}", self.host, self.port)
            }
            ProxyKind::Socks5 => format!("socks5://{}:{}", self.host, self.port),
        };
        let mut p = reqwest::Proxy::all(&url)?;
        if let Some(u) = &self.user {
            let pass = self.pass.clone().unwrap_or_default();
            p = p.basic_auth(u, &pass);
        }
        Ok(p)
    }
}

pub struct ProxyPool {
    proxies: Vec<Proxy>,
    cursor: AtomicUsize,
    ban_counts: Mutex<HashMap<String, u32>>,
    uses: Mutex<HashMap<String, u32>>,
}

impl ProxyPool {
    pub fn load(path: &Path) -> anyhow::Result<Arc<Self>> {
        let text = std::fs::read_to_string(path)?;
        let proxies: Vec<Proxy> = text.lines().filter_map(Proxy::parse_line).collect();
        Ok(Arc::new(Self {
            proxies,
            cursor: AtomicUsize::new(0),
            ban_counts: Mutex::new(HashMap::new()),
            uses: Mutex::new(HashMap::new()),
        }))
    }

    pub fn from_vec(proxies: Vec<Proxy>) -> Arc<Self> {
        Arc::new(Self {
            proxies,
            cursor: AtomicUsize::new(0),
            ban_counts: Mutex::new(HashMap::new()),
            uses: Mutex::new(HashMap::new()),
        })
    }

    pub fn len(&self) -> usize {
        self.proxies.len()
    }

    pub fn is_empty(&self) -> bool {
        self.proxies.is_empty()
    }

    /// Next proxy in rotation (round-robin).
    pub fn next(&self) -> Option<Proxy> {
        if self.proxies.is_empty() {
            return None;
        }
        let idx = self.cursor.fetch_add(1, Ordering::Relaxed) % self.proxies.len();
        let p = self.proxies[idx].clone();
        if let Some(u) = self.uses.lock().get_mut(&p.raw) {
            *u += 1;
        } else {
            self.uses.lock().insert(p.raw.clone(), 1);
        }
        Some(p)
    }

    pub fn ban(&self, proxy: &Proxy) {
        if let Some(c) = self.ban_counts.lock().get_mut(&proxy.raw) {
            *c += 1;
        } else {
            self.ban_counts.lock().insert(proxy.raw.clone(), 1);
        }
    }

    pub fn uses_of(&self, proxy: &Proxy) -> u32 {
        self.uses.lock().get(&proxy.raw).copied().unwrap_or(0)
    }

    pub fn stats(&self) -> (usize, usize) {
        let banned = self.ban_counts.lock().len();
        (self.proxies.len(), banned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_plain() {
        let p = Proxy::parse_line("1.2.3.4:8080").expect("a valid HTTP proxy should parse");
        assert_eq!(p.host, "1.2.3.4");
        assert_eq!(p.port, 8080);
        assert_eq!(p.kind, ProxyKind::Http);
    }

    #[test]
    fn test_parse_socks5_auth() {
        let p = Proxy::parse_line("socks5://user:pass@1.2.3.4:1080")
            .expect("a valid SOCKS5 proxy with authentication should parse");
        assert_eq!(p.kind, ProxyKind::Socks5);
        assert_eq!(p.user.as_deref(), Some("user"));
        assert_eq!(p.port, 1080);
    }
}
