//! Hit storage — text file + in-memory list.

use chrono::Local;
use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Hit {
    pub data: String,
    pub captured: String,
    pub proxy: String,
    pub hit_type: String, // SUCCESS / CUSTOM
    pub config_name: String,
    pub wordlist_name: String,
    pub date: String,
}

impl Hit {
    pub fn new(
        data: &str,
        captured: &str,
        proxy: &str,
        hit_type: &str,
        config_name: &str,
        wordlist_name: &str,
    ) -> Self {
        Self {
            data: data.to_string(),
            captured: captured.to_string(),
            proxy: proxy.to_string(),
            hit_type: hit_type.to_string(),
            config_name: config_name.to_string(),
            wordlist_name: wordlist_name.to_string(),
            date: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        }
    }

    /// Format per HitInfoFormat placeholders: <DATA> <PROXY> <DATE> <CONFIG> <WORDLIST> <TYPE> <CAPTURE>
    pub fn format(&self, template: &str) -> String {
        template
            .replace("<DATA>", &self.data)
            .replace("<PROXY>", &self.proxy)
            .replace("<DATE>", &self.date)
            .replace("<CONFIG>", &self.config_name)
            .replace("<WORDLIST>", &self.wordlist_name)
            .replace("<TYPE>", &self.hit_type)
            .replace("<CAPTURE>", &self.captured)
    }

    pub fn default_format(&self) -> String {
        self.format(&format!(
            "<DATE> <TYPE> <DATA> <CAPTURE> [{PROXY}]",
            PROXY = self.proxy
        ))
    }

    pub fn hash_key(&self) -> String {
        let mut h = Sha256::new();
        h.update(self.data.as_bytes());
        h.update(self.config_name.as_bytes());
        h.update(self.wordlist_name.as_bytes());
        format!("{:x}", h.finalize())
    }
}

pub struct HitStore {
    hits: Mutex<Vec<Hit>>,
    seen: Mutex<HashSet<String>>,
    out_path: Option<PathBuf>,
    template: String,
}

impl HitStore {
    pub fn new(out_path: Option<PathBuf>, template: String) -> Arc<Self> {
        Arc::new(Self {
            hits: Mutex::new(Vec::new()),
            seen: Mutex::new(HashSet::new()),
            out_path,
            template,
        })
    }

    /// Returns true if newly added (not duplicate).
    pub fn add(&self, hit: Hit) -> bool {
        let key = hit.hash_key();
        {
            let mut seen = self.seen.lock();
            if !seen.insert(key) {
                return false;
            }
        }
        let line = if self.template.is_empty() {
            hit.default_format()
        } else {
            hit.format(&self.template)
        };
        if let Some(path) = &self.out_path {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
            {
                let _ = writeln!(f, "{line}");
            }
        }
        self.hits.lock().push(hit);
        true
    }

    pub fn count(&self) -> usize {
        self.hits.lock().len()
    }

    pub fn all(&self) -> Vec<Hit> {
        self.hits.lock().clone()
    }
}
