//! HTTP client — builds and executes requests from RequestBlock.

use crate::ls::{condition, RequestBlock, VariableList};
use crate::proxy::Proxy;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::time::Duration;

pub struct HttpResponseData {
    pub status: u16,
    pub body: String,
    pub headers: HeaderMap,
    pub cookies: Vec<(String, String)>,
}

pub async fn execute_request(
    rb: &RequestBlock,
    vars: &VariableList,
    proxy: Option<&Proxy>,
    timeout: Duration,
    user_agent: Option<&str>,
) -> anyhow::Result<HttpResponseData> {
    // replace variables in URL / post data / headers
    let source = vars.get("SOURCE").unwrap_or("").to_string();
    let url = condition::replace_values(&rb.url, vars, &source);
    let method = reqwest::Method::from_bytes(rb.method.as_bytes())?;

    let mut builder = reqwest::Client::builder()
        .timeout(timeout)
        .danger_accept_invalid_certs(true)
        .cookie_store(true);

    if !rb.auto_redirect {
        builder = builder.redirect(reqwest::redirect::Policy::none());
    }

    if let Some(ua) = user_agent {
        if !ua.is_empty() {
            builder = builder.user_agent(ua);
        }
    }

    if let Some(p) = proxy {
        builder = builder.proxy(p.to_reqwest()?);
    }

    let client = builder.build()?;
    let mut req = client.request(method, &url);

    // headers
    let mut headers = HeaderMap::new();
    for (k, v) in &rb.headers {
        let k = condition::replace_values(k, vars, &source);
        let v = condition::replace_values(v, vars, &source);
        if let (Ok(name), Ok(val)) = (
            HeaderName::from_bytes(k.as_bytes()),
            HeaderValue::from_str(&v),
        ) {
            headers.insert(name, val);
        }
    }
    req = req.headers(headers);

    // body
    match rb.content_type.as_str() {
        "multipart/form-data" => {
            // simplified: treat as raw body with content-type header
            let data = condition::replace_values(&rb.raw_data, vars, &source);
            req = req.header("Content-Type", "multipart/form-data").body(data);
        }
        "application/x-www-form-urlencoded" => {
            let data = condition::replace_values(&rb.post_data, vars, &source);
            req = req
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(data);
        }
        "basic" => {
            req = req.basic_auth(&rb.auth_user, Some(&rb.auth_pass));
        }
        _ if !rb.post_data.is_empty() => {
            let data = condition::replace_values(&rb.post_data, vars, &source);
            req = req.body(data);
        }
        _ if !rb.raw_data.is_empty() => {
            let data = condition::replace_values(&rb.raw_data, vars, &source);
            req = req.body(data);
        }
        _ => {}
    }

    // cookies as Cookie header
    if !rb.cookies.is_empty() {
        let cookie_str = rb
            .cookies
            .iter()
            .map(|(k, v)| {
                let k = condition::replace_values(k, vars, &source);
                let v = condition::replace_values(v, vars, &source);
                format!("{k}={v}")
            })
            .collect::<Vec<_>>()
            .join("; ");
        req = req.header("Cookie", cookie_str);
    }

    let resp = req.send().await?;
    let status = resp.status().as_u16();
    let resp_headers = resp.headers().clone();
    let body = if rb.read_response_source {
        resp.text().await.unwrap_or_default()
    } else {
        String::new()
    };
    let cookies: Vec<(String, String)> = resp_headers
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .filter_map(|s| {
            let first = s.split(';').next()?;
            let (k, val) = first.split_once('=')?;
            Some((k.trim().to_string(), val.trim().to_string()))
        })
        .collect();

    Ok(HttpResponseData {
        status,
        body,
        headers: resp_headers,
        cookies,
    })
}
