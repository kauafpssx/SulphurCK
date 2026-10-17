//! SulphurCK — multiplatform HTTP checker (CLI / TUI / daemon)
//!
//! Rebuild of SilverBullet v1.1.4 engine subset: LoliScript REQUEST/KEYCHECK,
//! proxies, wordlists, hits, webhooks. No Selenium/OCR/browser.

pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod presentation;

// Compatibility facade. New code should import from its architectural layer.
pub use application::{engine, runner};
pub use domain::{config, ls};
pub use infrastructure::{data, hits, ipc, net, proxy, settings, webhook};
pub use presentation::tui;

pub use domain::config::AppConfig;
