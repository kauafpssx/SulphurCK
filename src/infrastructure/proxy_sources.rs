//! Persisted remote proxy sources and their local cache.

use crate::config::ProxyKind;
use crate::proxy::{Proxy, ProxyPool};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxySource {
    pub id: String,
    pub kind: ProxyKind,
    pub url: String,
    pub active: bool,
}

fn root() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("sulphurck")
        .join("proxies")
}

fn registry_path() -> PathBuf {
    root().join("sources.json")
}

fn cache_path() -> PathBuf {
    root().join("cached.txt")
}

pub fn list() -> anyhow::Result<Vec<ProxySource>> {
    let path = registry_path();
    if !path.exists() {
        return Ok(Vec::new());
    }
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}

fn save(sources: &[ProxySource]) -> anyhow::Result<()> {
    std::fs::create_dir_all(root())?;
    std::fs::write(registry_path(), serde_json::to_string_pretty(sources)?)?;
    Ok(())
}

pub fn add(kind: ProxyKind, url: String) -> anyhow::Result<ProxySource> {
    url::Url::parse(&url).map_err(|_| anyhow::anyhow!("proxy source URL must be absolute"))?;
    let mut sources = list()?;
    if sources
        .iter()
        .any(|source| source.url == url && source.kind == kind)
    {
        anyhow::bail!("this proxy source is already registered");
    }
    let source = ProxySource {
        id: uuid::Uuid::new_v4().simple().to_string()[..8].to_string(),
        kind,
        url,
        active: true,
    };
    sources.push(source.clone());
    save(&sources)?;
    Ok(source)
}

pub fn remove(id: &str) -> anyhow::Result<()> {
    let mut sources = list()?;
    let old_len = sources.len();
    sources.retain(|source| source.id != id);
    if sources.len() == old_len {
        anyhow::bail!("proxy source '{id}' was not found");
    }
    save(&sources)
}

/// Fetches active sources, normalizes their proxy kind and refreshes the cache.
pub async fn refresh() -> anyhow::Result<usize> {
    let sources = list()?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()?;
    let mut unique = HashSet::new();
    let mut proxies = Vec::new();

    for source in sources.iter().filter(|source| source.active) {
        let body = client
            .get(&source.url)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        for raw in body.lines().map(str::trim).filter(|line| !line.is_empty()) {
            let normalized = normalize(raw, source.kind);
            if Proxy::parse_line(&normalized).is_some() && unique.insert(normalized.clone()) {
                proxies.push(normalized);
            }
        }
    }

    std::fs::create_dir_all(root())?;
    std::fs::write(cache_path(), proxies.join("\n"))?;
    Ok(proxies.len())
}

/// Refreshes source URLs for a run, then returns their cached proxies.
pub async fn refresh_and_load() -> anyhow::Result<Option<Arc<ProxyPool>>> {
    if list()?.is_empty() {
        return Ok(None);
    }
    refresh().await?;
    let cached = std::fs::read_to_string(cache_path())?;
    let proxies = cached.lines().filter_map(Proxy::parse_line).collect();
    Ok(Some(ProxyPool::from_vec(proxies)))
}

fn normalize(raw: &str, kind: ProxyKind) -> String {
    if raw.contains("://") {
        return raw.to_string();
    }
    match kind {
        ProxyKind::Http => raw.to_string(),
        ProxyKind::Socks4 => format!("socks4://{raw}"),
        ProxyKind::Socks4a => format!("socks4a://{raw}"),
        ProxyKind::Socks5 => format!("socks5://{raw}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_a_scheme_less_socks_source() {
        assert_eq!(
            normalize("127.0.0.1:1080", ProxyKind::Socks5),
            "socks5://127.0.0.1:1080"
        );
    }
}
