//! Data pool — wordlist loading with base64 decode + dedup.

use base64::Engine as _;
use std::collections::HashSet;
use std::path::Path;

pub struct DataPool {
    lines: Vec<String>,
    index: usize,
}

impl DataPool {
    pub fn load(path: &Path, is_base64: bool, dedup: bool) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(path)?;
        let mut lines: Vec<String> = if is_base64 {
            // whole file is base64 of newline-separated data
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(raw.trim())
                .or_else(|_| {
                    base64::engine::general_purpose::STANDARD.decode(raw.trim_end_matches('='))
                })
                .map_err(|e| anyhow::anyhow!("base64 decode failed: {e}"))?;
            String::from_utf8_lossy(&decoded)
                .lines()
                .map(|l| l.to_string())
                .collect()
        } else {
            raw.lines().map(|l| l.to_string()).collect()
        };
        lines.retain(|l| !l.trim().is_empty());
        if dedup {
            let mut seen = HashSet::new();
            lines.retain(|l| seen.insert(l.clone()));
        }
        Ok(Self { lines, index: 0 })
    }

    pub fn from_lines(lines: Vec<String>) -> Self {
        Self { lines, index: 0 }
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Takes the next line (wrap-around disabled; returns `None` when exhausted).
    pub fn take_next(&mut self) -> Option<String> {
        if self.index >= self.lines.len() {
            return None;
        }
        let line = self.lines[self.index].clone();
        self.index += 1;
        Some(line)
    }
}

/// Count lines in a file (fast).
pub fn count_lines(path: &Path) -> anyhow::Result<u64> {
    let text = std::fs::read_to_string(path)?;
    Ok(text.lines().filter(|l| !l.trim().is_empty()).count() as u64)
}

/// Deduplicate a file in place.
pub fn unique_file(path: &Path) -> anyhow::Result<u64> {
    let text = std::fs::read_to_string(path)?;
    let mut seen = HashSet::new();
    let mut out = String::new();
    let mut removed = 0u64;
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if seen.insert(line.to_string()) {
            out.push_str(line);
            out.push('\n');
        } else {
            removed += 1;
        }
    }
    std::fs::write(path, out)?;
    Ok(removed)
}
