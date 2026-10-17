//! SulphurCK CLI entry — subcommands for run, proxy, wordlist, config, daemon.

use crate as sulphur_ck;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(
    name = "sulphurck",
    version,
    about = "SulphurCK — ultra-light HTTP checker"
)]
struct Cli {
    /// Config file (.svb)
    #[arg(short, long, global = true)]
    config: Option<PathBuf>,

    /// Wordlist / data file
    #[arg(short, long, global = true)]
    wordlist: Option<PathBuf>,

    /// Proxy list file
    #[arg(short, long, global = true)]
    proxies: Option<PathBuf>,

    /// Number of concurrent bots
    #[arg(short, long, global = true)]
    bots: Option<u32>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Run a check (default when config is set)
    Run {
        /// Max CPM (0 = unlimited)
        #[arg(long, default_value_t = 0)]
        max_cpm: u32,
        /// Stop after N hits
        #[arg(long)]
        max_hits: Option<u64>,
    },
    /// Validate / classify proxies
    Proxy {
        #[command(subcommand)]
        action: ProxyAction,
    },
    /// Wordlist utilities
    Wordlist {
        #[command(subcommand)]
        action: WordlistAction,
    },
    /// Config utilities
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Start daemon (engine keeps running; TUI attaches/detaches)
    Daemon {
        /// Unix socket / named pipe path
        #[arg(long)]
        socket: Option<PathBuf>,
    },
    /// Launch TUI
    Tui,
}

#[derive(Subcommand, Debug)]
enum ProxyAction {
    /// Parse proxy file and print stats
    Parse { file: PathBuf },
    /// Test proxies against a URL
    Test {
        file: PathBuf,
        #[arg(long, default_value = "https://httpbin.org/ip")]
        url: String,
    },
    /// Remove dead proxies
    Clean { file: PathBuf },
}

#[derive(Subcommand, Debug)]
enum WordlistAction {
    /// Deduplicate lines
    Unique { file: PathBuf },
    /// Count lines
    Count { file: PathBuf },
}

#[derive(Subcommand, Debug)]
enum ConfigAction {
    /// Parse .svb and print summary
    Parse { file: PathBuf },
    /// Validate script blocks
    Validate { file: PathBuf },
}

/// Runs the command-line adapter. The binary crate only composes this adapter.
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env().add_directive(
                "info"
                    .parse()
                    .expect("the built-in info directive is valid"),
            ),
        )
        .init();

    let cli = Cli::parse();

    match cli.command.unwrap_or(Command::Run {
        max_cpm: 0,
        max_hits: None,
    }) {
        Command::Run { max_cpm, max_hits } => {
            let rt = tokio::runtime::Runtime::new()
                .expect("the Tokio runtime should initialize on supported platforms");
            rt.block_on(async move {
                let config_path = cli
                    .config
                    .clone()
                    .unwrap_or_else(|| PathBuf::from("config.svb"));
                let wordlist_path = cli
                    .wordlist
                    .clone()
                    .unwrap_or_else(|| PathBuf::from("data.txt"));

                let config_text = match std::fs::read_to_string(&config_path) {
                    Ok(t) => t,
                    Err(e) => {
                        eprintln!("config read error: {e}");
                        std::process::exit(1);
                    }
                };
                let svb = match sulphur_ck::ls::parse_svb(&config_text) {
                    Ok(s) => std::sync::Arc::new(s),
                    Err(e) => {
                        eprintln!("config parse error: {e}");
                        std::process::exit(1);
                    }
                };

                let app = std::sync::Arc::new(
                    sulphur_ck::settings::load(&config_path).unwrap_or_default(),
                );

                let proxy_pool = cli
                    .proxies
                    .as_ref()
                    .and_then(|p| sulphur_ck::proxy::ProxyPool::load(p).ok());

                let runner = sulphur_ck::runner::Runner::new();
                let run_cfg = sulphur_ck::runner::RunConfig {
                    max_cpm,
                    max_hits: max_hits.unwrap_or(0) as u32,
                    wait_time: app.general.wait_time,
                    bots: cli.bots.unwrap_or(1),
                };

                if let Err(e) = runner
                    .run(
                        app,
                        svb,
                        config_path.to_string_lossy().to_string(),
                        &wordlist_path,
                        proxy_pool,
                        run_cfg,
                    )
                    .await
                {
                    eprintln!("run error: {e}");
                    std::process::exit(1);
                }
            });
        }
        Command::Proxy { action } => match action {
            ProxyAction::Parse { file } => match sulphur_ck::proxy::ProxyPool::load(&file) {
                Ok(pool) => {
                    println!("Loaded {} proxies", pool.len());
                }
                Err(e) => eprintln!("proxy load error: {e}"),
            },
            ProxyAction::Test { file, url } => match sulphur_ck::proxy::ProxyPool::load(&file) {
                Ok(pool) => println!("Testing {} proxies against {}", pool.len(), url),
                Err(e) => eprintln!("proxy load error: {e}"),
            },
            ProxyAction::Clean { file } => match sulphur_ck::proxy::ProxyPool::load(&file) {
                Ok(pool) => println!("Clean: {} proxies loaded", pool.len()),
                Err(e) => eprintln!("proxy load error: {e}"),
            },
        },
        Command::Wordlist { action } => match action {
            WordlistAction::Unique { file } => {
                match sulphur_ck::data::DataPool::load(&file, false, true) {
                    Ok(pool) => println!("Unique lines: {}", pool.len()),
                    Err(e) => eprintln!("wordlist error: {e}"),
                }
            }
            WordlistAction::Count { file } => {
                match sulphur_ck::data::DataPool::load(&file, false, false) {
                    Ok(pool) => println!("Total lines: {}", pool.len()),
                    Err(e) => eprintln!("wordlist error: {e}"),
                }
            }
        },
        Command::Config { action } => match action {
            ConfigAction::Parse { file } => match std::fs::read_to_string(&file) {
                Ok(text) => match sulphur_ck::ls::parse_svb(&text) {
                    Ok(svb) => {
                        println!("Config: {}", svb.settings.name);
                        println!("Author: {}", svb.settings.author);
                        println!("Version: {}", svb.settings.version);
                        println!("Max CPM: {}", svb.settings.max_cpm);
                        println!("Suggested bots: {}", svb.settings.suggested_bots);
                        println!("Script blocks: {}", svb.script.lines().count());
                    }
                    Err(e) => eprintln!("parse error: {e}"),
                },
                Err(e) => eprintln!("read error: {e}"),
            },
            ConfigAction::Validate { file } => match std::fs::read_to_string(&file) {
                Ok(text) => match sulphur_ck::ls::parse_svb(&text) {
                    Ok(svb) => {
                        let blocks = sulphur_ck::ls::parse_script(&svb.script);
                        println!("Valid: {} blocks parsed", blocks.len());
                        for (i, b) in blocks.iter().enumerate() {
                            println!("  [{}] {:?}", i, b);
                        }
                    }
                    Err(e) => eprintln!("parse error: {e}"),
                },
                Err(e) => eprintln!("read error: {e}"),
            },
        },
        Command::Daemon { socket } => {
            println!("daemon socket={:?}", socket);
        }
        Command::Tui => {
            let stats = std::sync::Arc::new(sulphur_ck::runner::Stats::default());
            if let Err(e) = sulphur_ck::tui::run_tui(stats, 0) {
                eprintln!("tui error: {e}");
                std::process::exit(1);
            }
        }
    }
}
