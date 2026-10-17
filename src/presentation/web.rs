//! HTTP presentation adapter for the lightweight management dashboard.

use axum::{
    extract::{Path, Request, State},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{delete, get},
    Json, Router,
};
use serde::Deserialize;
use std::{net::SocketAddr, sync::Arc};

const INDEX: &str = include_str!("web/index.html");
const STYLES: &str = include_str!("web/styles.css");
const APP: &str = include_str!("web/app.js");

#[derive(Clone)]
struct WebState {
    access_key: Arc<str>,
}

#[derive(Deserialize)]
struct StartRequest {
    id: String,
    config: String,
    wordlist: String,
    bots: Option<u32>,
    max_cpm: Option<u32>,
}
#[derive(Deserialize)]
struct ProxyRequest {
    kind: crate::config::ProxyKind,
    url: String,
}

pub async fn serve(host: &str, port: u16) -> anyhow::Result<()> {
    let address: SocketAddr = format!("{host}:{port}").parse()?;
    let state = WebState {
        access_key: crate::settings::dashboard_access_key()?.into(),
    };
    let app = Router::new()
        .route("/", get(dashboard))
        .route(
            "/assets/styles.css",
            get(|| async { ([(header::CONTENT_TYPE, "text/css")], STYLES) }),
        )
        .route(
            "/assets/app.js",
            get(|| async { ([(header::CONTENT_TYPE, "application/javascript")], APP) }),
        )
        .route("/api/services", get(services).post(start))
        .route("/api/services/{id}", delete(stop))
        .route("/api/proxies", get(proxies).post(add_proxy))
        .route("/api/proxies/{id}", delete(remove_proxy))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_access,
        ))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind(address).await?;
    println!(
        "SulphurCK dashboard listening on http://{address}/?KEY={}",
        state.access_key
    );
    axum::serve(listener, app).await?;
    Ok(())
}

async fn dashboard(State(state): State<WebState>) -> Html<String> {
    Html(INDEX.replace("__DASHBOARD_KEY__", &state.access_key))
}

/// Applies query-key authentication to the dashboard, its assets and every API
/// endpoint. `KEY` is intentionally accepted in the format exposed by the CLI.
async fn require_access(State(state): State<WebState>, request: Request, next: Next) -> Response {
    let authorized = request.uri().query().is_some_and(|query| {
        url::form_urlencoded::parse(query.as_bytes())
            .any(|(name, value)| name == "KEY" && value == state.access_key.as_ref())
    });

    if authorized {
        next.run(request).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            "Unauthorized. Open the dashboard using its ?KEY= URL.",
        )
            .into_response()
    }
}

async fn services() -> Response {
    response(crate::services::list())
}
async fn proxies() -> Response {
    response(crate::proxy_sources::list())
}
async fn start(Json(request): Json<StartRequest>) -> Response {
    response(crate::services::start(
        request.id,
        std::path::Path::new(&request.config),
        std::path::Path::new(&request.wordlist),
        request.bots,
        request.max_cpm.unwrap_or(0),
    ))
}
async fn stop(Path(id): Path<String>) -> Response {
    empty(crate::services::stop(&id))
}
async fn add_proxy(Json(request): Json<ProxyRequest>) -> Response {
    response(crate::proxy_sources::add(request.kind, request.url))
}
async fn remove_proxy(Path(id): Path<String>) -> Response {
    empty(crate::proxy_sources::remove(&id))
}
fn response<T: serde::Serialize>(result: anyhow::Result<T>) -> Response {
    match result {
        Ok(value) => Json(value).into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}
fn empty(result: anyhow::Result<()>) -> Response {
    match result {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}
