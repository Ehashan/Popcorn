//! A small blocking TMDB client with retries.
//!
//! Errors never carry the request URL, because it can contain the API key.

use std::thread;
use std::time::Duration;

use serde_json::Value;
use ureq::http::Response;
use ureq::{Agent, Body};

use crate::error::{AppError, AppResult};

const API_BASE: &str = "https://api.themoviedb.org/3";
const IMAGE_BASE: &str = "https://image.tmdb.org/t/p";
const MAX_ATTEMPTS: u32 = 4;
const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Auth {
    /// A v3 API key, sent as the `api_key` query parameter.
    ApiKey(String),
    /// A v4 read access token, sent as a bearer token.
    Bearer(String),
}

impl Auth {
    pub fn detect(key: &str) -> Self {
        let key = key.trim();
        if key.starts_with("eyJ") {
            Self::Bearer(key.to_string())
        } else {
            Self::ApiKey(key.to_string())
        }
    }
}

#[derive(Clone)]
pub struct Client {
    agent: Agent,
    auth: Auth,
}

impl Client {
    pub fn new(key: &str) -> Self {
        let config = Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(20)))
            .http_status_as_error(false)
            .build();
        Self {
            agent: config.into(),
            auth: Auth::detect(key),
        }
    }

    /// Succeeds only if TMDB accepts the key.
    pub fn test_key(&self) -> AppResult<()> {
        self.get_json("/authentication", &[]).map(drop)
    }

    pub fn get_json(&self, path: &str, query: &[(&str, &str)]) -> AppResult<Value> {
        let url = format!("{API_BASE}{path}");
        let mut response = with_retries(|| {
            let mut request = self.agent.get(&url);
            for (name, value) in query {
                request = request.query(*name, *value);
            }
            request = match &self.auth {
                Auth::ApiKey(key) => request.query("api_key", key),
                Auth::Bearer(token) => request.header("Authorization", format!("Bearer {token}")),
            };
            request.call()
        })?;
        let text = response
            .body_mut()
            .read_to_string()
            .map_err(|_| AppError::Offline)?;
        Ok(serde_json::from_str(&text)?)
    }

    /// `remote` is a TMDB image path such as `/abc.jpg`.
    pub fn download_image(&self, size: &str, remote: &str) -> AppResult<Vec<u8>> {
        let url = format!("{IMAGE_BASE}/{size}{remote}");
        let mut response = with_retries(|| self.agent.get(&url).call())?;
        response
            .body_mut()
            .with_config()
            .limit(MAX_IMAGE_BYTES)
            .read_to_vec()
            .map_err(|_| AppError::Offline)
    }
}

/// Retries rate limits and server errors with exponential backoff. A network
/// failure is retried once, so that being offline is noticed quickly.
fn with_retries(
    mut send: impl FnMut() -> Result<Response<Body>, ureq::Error>,
) -> AppResult<Response<Body>> {
    let mut delay = Duration::from_millis(500);
    let mut attempt = 1;
    loop {
        let (error, wait) = match send() {
            Ok(response) => {
                let status = response.status().as_u16();
                match status {
                    200..=299 => return Ok(response),
                    401 | 403 => return Err(AppError::InvalidTmdbKey),
                    429 | 500..=599 => {
                        let asked = response
                            .headers()
                            .get("retry-after")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|v| v.parse().ok())
                            .map(|secs: u64| Duration::from_secs(secs.min(30)));
                        (AppError::Tmdb(status), asked.unwrap_or(delay))
                    }
                    _ => return Err(AppError::Tmdb(status)),
                }
            }
            Err(_) if attempt >= 2 => return Err(AppError::Offline),
            Err(_) => (AppError::Offline, delay),
        };
        if attempt >= MAX_ATTEMPTS {
            return Err(error);
        }
        thread::sleep(wait);
        delay *= 2;
        attempt += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_the_kind_of_key() {
        assert_eq!(Auth::detect(" abc123 "), Auth::ApiKey("abc123".into()));
        assert_eq!(
            Auth::detect("eyJhbGci.x.y"),
            Auth::Bearer("eyJhbGci.x.y".into())
        );
    }
}
