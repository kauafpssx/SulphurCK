//! Webhook — POST JSON on hit (SilverBullet WebhookFormat equivalent).

use crate::hits::Hit;
use serde_json::json;
use std::sync::Arc;

pub struct Webhook {
    url: String,
    user: String,
    client: reqwest::Client,
}

impl Webhook {
    pub fn new(url: String, user: String) -> Arc<Self> {
        Arc::new(Self {
            url,
            user,
            client: reqwest::Client::new(),
        })
    }

    pub async fn send(&self, hit: &Hit) -> anyhow::Result<()> {
        if self.url.is_empty() {
            return Ok(());
        }
        let body = json!({
            "username": self.user,
            "content": hit.format(&format!(
                "**{type}** | `{data}`\nCaptures: `{cap}`\nProxy: `{proxy}`\nConfig: `{cfg}`",
                type = hit.hit_type,
                data = hit.data,
                cap = hit.captured,
                proxy = hit.proxy,
                cfg = hit.config_name,
            )),
        });
        self.client
            .post(&self.url)
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("webhook send failed: {e}"))?;
        Ok(())
    }
}
