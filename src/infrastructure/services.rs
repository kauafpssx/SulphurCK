//! Local background-service registry for concurrent SulphurCK runs.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceRecord {
    pub id: String,
    pub pid: u32,
    pub config: PathBuf,
    pub wordlist: PathBuf,
    pub log: PathBuf,
}

fn root() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("sulphurck")
        .join("services")
}

fn registry() -> PathBuf {
    root().join("services.json")
}

fn load() -> anyhow::Result<Vec<ServiceRecord>> {
    let path = registry();
    if !path.exists() {
        return Ok(Vec::new());
    }
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}

fn save(items: &[ServiceRecord]) -> anyhow::Result<()> {
    std::fs::create_dir_all(root())?;
    std::fs::write(registry(), serde_json::to_string_pretty(items)?)?;
    Ok(())
}

pub fn start(
    id: String,
    config: &Path,
    wordlist: &Path,
    bots: u32,
    max_cpm: u32,
) -> anyhow::Result<ServiceRecord> {
    let mut records = list()?;
    if records.iter().any(|record| record.id == id) {
        anyhow::bail!("service '{id}' is already running");
    }
    std::fs::create_dir_all(root())?;
    let log = root().join(format!("{id}.log"));
    let output = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("--config")
        .arg(config)
        .arg("--wordlist")
        .arg(wordlist)
        .arg("--bots")
        .arg(bots.to_string())
        .arg("run")
        .arg("--max-cpm")
        .arg(max_cpm.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::from(output.try_clone()?))
        .stderr(Stdio::from(output));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let child = command.spawn()?;
    let record = ServiceRecord {
        id,
        pid: child.id(),
        config: config.into(),
        wordlist: wordlist.into(),
        log,
    };
    records.push(record.clone());
    save(&records)?;
    Ok(record)
}

pub fn list() -> anyhow::Result<Vec<ServiceRecord>> {
    let mut records = load()?;
    records.retain(|record| alive(record.pid));
    save(&records)?;
    Ok(records)
}

pub fn stop(id: &str) -> anyhow::Result<()> {
    let mut records = load()?;
    let Some(record) = records.iter().find(|record| record.id == id).cloned() else {
        anyhow::bail!("service '{id}' was not found");
    };
    #[cfg(windows)]
    Command::new("taskkill")
        .args(["/PID", &record.pid.to_string(), "/T", "/F"])
        .status()?;
    #[cfg(unix)]
    Command::new("kill")
        .args(["-TERM", &record.pid.to_string()])
        .status()?;
    records.retain(|item| item.id != id);
    save(&records)
}

pub fn log_path(id: &str) -> anyhow::Result<PathBuf> {
    load()?
        .into_iter()
        .find(|record| record.id == id)
        .map(|record| record.log)
        .ok_or_else(|| anyhow::anyhow!("service '{id}' was not found"))
}

#[cfg(windows)]
fn alive(pid: u32) -> bool {
    Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).contains(&pid.to_string()))
        .unwrap_or(false)
}

#[cfg(unix)]
fn alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}
