//! IPC — simple local daemon over a Unix socket / named pipe for remote control.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[cfg(unix)]
use tokio::net::{UnixListener, UnixStream};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcRequest {
    Start {
        config: String,
        wordlist: String,
        proxies: Option<String>,
        max_cpm: Option<u32>,
        max_hits: Option<u32>,
    },
    Stop,
    Status,
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcResponse {
    Ok,
    Pong,
    Status {
        running: bool,
        checked: u64,
        hits: u64,
        fails: u64,
        retries: u64,
        bans: u64,
        errors: u64,
        cpm: u64,
    },
    Error(String),
}

pub fn default_socket_path() -> PathBuf {
    #[cfg(unix)]
    {
        std::env::temp_dir().join("sulphurck.sock")
    }
    #[cfg(windows)]
    {
        // Windows: use a simple file-based mailbox via named pipe path
        std::env::temp_dir().join("sulphurck.ipc")
    }
}

/// Send a request to a running daemon and wait for the response.
pub async fn send_request(path: &PathBuf, req: &IpcRequest) -> anyhow::Result<IpcResponse> {
    let payload = bincode::serialize(req)?;
    let mut stream = connect(path).await?;
    stream.write_all(&payload).await?;
    stream.write_all(&[0]).await?; // null terminator
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    loop {
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.last() == Some(&0) {
            buf.pop();
            break;
        }
    }
    let resp: IpcResponse = bincode::deserialize(&buf)?;
    Ok(resp)
}

/// Start a Unix-socket listener; returns the path.
pub async fn serve<F, Fut>(_path: &PathBuf, handler: F) -> anyhow::Result<()>
where
    F: Fn(IpcRequest) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = IpcResponse> + Send + 'static,
{
    let handler = std::sync::Arc::new(handler);
    #[cfg(unix)]
    {
        let _ = std::fs::remove_file(path);
        let listener = UnixListener::bind(path)?;
        loop {
            let (stream, _) = listener.accept().await?;
            let h = handler.clone();
            tokio::spawn(async move {
                if let Err(e) = handle_conn(stream, h).await {
                    eprintln!("ipc connection error: {e}");
                }
            });
        }
    }
    #[cfg(windows)]
    {
        // Windows fallback: loopback TCP on a fixed port derived from username hash
        use tokio::net::TcpListener;
        let port = 38472u16; // fixed local port
        let listener = TcpListener::bind(("127.0.0.1", port)).await?;
        eprintln!("ipc listening on 127.0.0.1:{port}");
        loop {
            let (stream, _) = listener.accept().await?;
            let h = handler.clone();
            tokio::spawn(async move {
                if let Err(e) = handle_tcp(stream, h).await {
                    eprintln!("ipc connection error: {e}");
                }
            });
        }
    }
}

#[cfg(unix)]
async fn handle_conn<F, Fut>(
    mut stream: UnixStream,
    handler: std::sync::Arc<F>,
) -> anyhow::Result<()>
where
    F: Fn(IpcRequest) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = IpcResponse> + Send + 'static,
{
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    loop {
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.last() == Some(&0) {
            buf.pop();
            break;
        }
    }
    let req: IpcRequest = bincode::deserialize(&buf)?;
    let resp = handler(req).await;
    let payload = bincode::serialize(&resp)?;
    stream.write_all(&payload).await?;
    stream.write_all(&[0]).await?;
    Ok(())
}

#[cfg(windows)]
async fn handle_tcp<F, Fut>(
    mut stream: tokio::net::TcpStream,
    handler: std::sync::Arc<F>,
) -> anyhow::Result<()>
where
    F: Fn(IpcRequest) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = IpcResponse> + Send + 'static,
{
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    loop {
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.last() == Some(&0) {
            buf.pop();
            break;
        }
    }
    let req: IpcRequest = bincode::deserialize(&buf)?;
    let resp = handler(req).await;
    let payload = bincode::serialize(&resp)?;
    stream.write_all(&payload).await?;
    stream.write_all(&[0]).await?;
    Ok(())
}

#[cfg(unix)]
async fn connect(path: &PathBuf) -> anyhow::Result<UnixStream> {
    Ok(UnixStream::connect(path).await?)
}

#[cfg(windows)]
async fn connect(_path: &PathBuf) -> anyhow::Result<tokio::net::TcpStream> {
    Ok(tokio::net::TcpStream::connect("127.0.0.1:38472").await?)
}
