//! Runner — orchestrates bot pool, CPM throttle, data feed, stats.

use crate::config::AppConfig;
use crate::data::DataPool;
use crate::engine::Engine;
use crate::hits::HitStore;
use crate::ls::{BotStatus, SvbConfig};
use crate::proxy::ProxyPool;
use crate::webhook::Webhook;
use parking_lot::Mutex;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{error, info};

#[derive(Default)]
pub struct Stats {
    pub checked: AtomicU64,
    pub hits: AtomicU64,
    pub fails: AtomicU64,
    pub retries: AtomicU64,
    pub bans: AtomicU64,
    pub errors: AtomicU64,
    pub customs: AtomicU64,
    pub cpm: AtomicU64,
}

impl Stats {
    pub fn record(&self, status: &BotStatus, is_new_hit: bool) {
        self.checked.fetch_add(1, Ordering::Relaxed);
        match status {
            BotStatus::Success => {
                if is_new_hit {
                    self.hits.fetch_add(1, Ordering::Relaxed);
                }
            }
            BotStatus::Fail => {
                self.fails.fetch_add(1, Ordering::Relaxed);
            }
            BotStatus::Retry => {
                self.retries.fetch_add(1, Ordering::Relaxed);
            }
            BotStatus::Ban => {
                self.bans.fetch_add(1, Ordering::Relaxed);
            }
            BotStatus::Error => {
                self.errors.fetch_add(1, Ordering::Relaxed);
            }
            BotStatus::Custom => {
                self.customs.fetch_add(1, Ordering::Relaxed);
            }
            BotStatus::None => {}
        }
    }
}

pub struct RunConfig {
    pub max_cpm: u32,
    pub max_hits: u32,
    pub wait_time: u64,
    pub bots: u32,
}

pub struct Runner {
    pub stats: Arc<Stats>,
    pub running: Arc<AtomicBool>,
}

impl Runner {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            stats: Arc::new(Stats::default()),
            running: Arc::new(AtomicBool::new(false)),
        })
    }

    pub async fn run(
        &self,
        app: Arc<AppConfig>,
        svb: Arc<SvbConfig>,
        config_name: String,
        wordlist_path: &Path,
        proxy_pool: Option<Arc<ProxyPool>>,
        run_cfg: RunConfig,
    ) -> anyhow::Result<()> {
        if self.running.swap(true, Ordering::SeqCst) {
            anyhow::bail!("runner already running");
        }

        let mut pool = DataPool::load(wordlist_path, false, true)?;
        let total = pool.len() as u64;
        info!("loaded {total} data lines");

        let hit_path = std::path::PathBuf::from(app.general.hit_output_path.clone());
        let hits = HitStore::new(Some(hit_path), svb.settings.hit_info_format.clone());
        let engine = Arc::new(Engine::new(
            svb.clone(),
            app.clone(),
            hits.clone(),
            config_name.clone(),
        ));

        let webhook = if app.general.webhook_enabled && !app.general.webhook_url.is_empty() {
            Some(Webhook::new(
                app.general.webhook_url.clone(),
                app.general.webhook_user.clone(),
            ))
        } else {
            None
        };

        let (tx, mut rx) = mpsc::channel::<(String, BotStatus, String, Option<String>)>(4096);

        // CPM limiter: sliding 60s window
        let cpm_window: Arc<Mutex<Vec<Instant>>> = Arc::new(Mutex::new(Vec::new()));
        let stats = self.stats.clone();
        let running = self.running.clone();
        let max_cpm = run_cfg.max_cpm;
        let wait = Duration::from_millis(run_cfg.wait_time * 1000);
        let max_hits = run_cfg.max_hits;
        let bots = run_cfg.bots.max(1) as usize;

        // Feed task
        let feed_running = running.clone();
        let feed_tx = tx.clone();
        let feed_stats = stats.clone();
        let feed_proxy = proxy_pool.clone();
        let feed_cpm = cpm_window.clone();
        let feed_handle = tokio::spawn(async move {
            let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(bots));
            let mut handles = Vec::new();
            loop {
                if !feed_running.load(Ordering::Relaxed) {
                    break;
                }
                if max_hits > 0 && feed_stats.hits.load(Ordering::Relaxed) >= max_hits as u64 {
                    info!("max hits reached");
                    break;
                }
                let Some(data) = pool.take_next() else {
                    info!("wordlist exhausted");
                    break;
                };

                // CPM throttle
                if max_cpm > 0 {
                    loop {
                        {
                            let mut w = feed_cpm.lock();
                            let now = Instant::now();
                            w.retain(|t| now.duration_since(*t) < Duration::from_secs(60));
                            if w.len() < max_cpm as usize {
                                w.push(now);
                                break;
                            }
                        }
                        tokio::time::sleep(Duration::from_millis(10)).await;
                        if !feed_running.load(Ordering::Relaxed) {
                            return;
                        }
                    }
                }

                let permit = match sem.clone().acquire_owned().await {
                    Ok(p) => p,
                    Err(_) => break,
                };
                let eng = engine.clone();
                let tx = feed_tx.clone();
                let proxy = feed_proxy.as_ref().and_then(|p| p.next());
                let h = tokio::spawn(async move {
                    let _permit = permit;
                    let outcome = eng.run_bot(data.clone(), proxy.clone()).await;
                    if wait > Duration::ZERO {
                        tokio::time::sleep(wait).await;
                    }
                    let _ = tx
                        .send((data, outcome.status, outcome.captured, outcome.error))
                        .await;
                });
                handles.push(h);
                if handles.len() > bots * 2 {
                    // reap finished
                    handles.retain(|h| !h.is_finished());
                }
            }
            for h in handles {
                let _ = h.await;
            }
        });

        // Stats / collector task
        let stats2 = self.stats.clone();
        let running2 = self.running.clone();
        let webhook2 = webhook.clone();
        let collector = tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(1));
            let mut last_checked = 0u64;
            let mut last_tick = Instant::now();
            while running2.load(Ordering::Relaxed) {
                tokio::select! {
                    msg = rx.recv() => {
                        match msg {
                            Some((data, status, captured, err)) => {
                                let is_hit = matches!(status, BotStatus::Success | BotStatus::Custom);
                                stats2.record(&status, is_hit);
                                if is_hit {
                                    info!("HIT [{status:?}] {data}");
                                    if !captured.is_empty() {
                                        let c = captured.chars().take(200).collect::<String>();
                                        info!("  captures: {c}");
                                    }
                                    if let Some(wh) = &webhook2 {
                                        let hit_type = match status {
                                            BotStatus::Success => "SUCCESS",
                                            BotStatus::Custom => "CUSTOM",
                                            _ => "UNKNOWN",
                                        };
                                        let hit = crate::hits::Hit::new(
                                            &data,
                                            &captured,
                                            "",
                                            hit_type,
                                            &config_name,
                                            "",
                                        );
                                        if let Err(e) = wh.send(&hit).await {
                                            error!("webhook send error: {e}");
                                        }
                                    }
                                }
                                if let Some(e) = err {
                                    debug_err(&e);
                                }
                            }
                            None => break,
                        }
                    }
                    _ = tick.tick() => {
                        let now = Instant::now();
                        let checked = stats2.checked.load(Ordering::Relaxed);
                        let dt = now.duration_since(last_tick).as_secs_f64();
                        if dt > 0.0 {
                            let delta = checked.saturating_sub(last_checked);
                            stats2.cpm.store((delta as f64 / dt * 60.0) as u64, Ordering::Relaxed);
                        }
                        last_checked = checked;
                        last_tick = now;
                    }
                }
            }
        });

        let _ = feed_handle.await;
        self.running.store(false, Ordering::SeqCst);
        let _ = collector.await;
        info!(
            "run complete: checked={} hits={} fails={} retries={} bans={} errors={}",
            self.stats.checked.load(Ordering::Relaxed),
            self.stats.hits.load(Ordering::Relaxed),
            self.stats.fails.load(Ordering::Relaxed),
            self.stats.retries.load(Ordering::Relaxed),
            self.stats.bans.load(Ordering::Relaxed),
            self.stats.errors.load(Ordering::Relaxed),
        );
        Ok(())
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

fn debug_err(e: &str) {
    error!("bot error: {e}");
}
