//! Test doubles shared by the TMDB tests.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::state::lock;
use crate::tmdb::fetch::Api;

/// A folder that is deleted when dropped.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("popcorn-tmdb-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

type Hook<'a> = Box<dyn Fn() + 'a>;

/// Answers from a table keyed by `path?name=value&…`. Anything else is offline.
#[derive(Default)]
pub struct FakeApi<'a> {
    responses: HashMap<String, Result<Value, u16>>,
    requests: Mutex<Vec<String>>,
    downloads: Mutex<usize>,
    images_offline: bool,
    /// Runs during every request, as something else happening meanwhile would.
    hook: Option<Hook<'a>>,
}

impl<'a> FakeApi<'a> {
    pub fn json(mut self, request: &str, response: Value) -> Self {
        self.responses.insert(request.to_string(), Ok(response));
        self
    }

    pub fn fail(mut self, request: &str, status: u16) -> Self {
        self.responses.insert(request.to_string(), Err(status));
        self
    }

    pub fn images_offline(mut self) -> Self {
        self.images_offline = true;
        self
    }

    pub fn during_requests(mut self, hook: impl Fn() + 'a) -> Self {
        self.hook = Some(Box::new(hook));
        self
    }

    pub fn requests(&self) -> Vec<String> {
        lock(&self.requests).clone()
    }

    pub fn downloads(&self) -> usize {
        *lock(&self.downloads)
    }
}

impl Api for FakeApi<'_> {
    fn get_json(&self, path: &str, query: &[(&str, &str)]) -> AppResult<Value> {
        let query: Vec<String> = query.iter().map(|(k, v)| format!("{k}={v}")).collect();
        let request = if query.is_empty() {
            path.to_string()
        } else {
            format!("{path}?{}", query.join("&"))
        };
        lock(&self.requests).push(request.clone());
        if let Some(hook) = &self.hook {
            hook();
        }
        match self.responses.get(&request) {
            Some(Ok(response)) => Ok(response.clone()),
            Some(Err(status)) => Err(AppError::Tmdb(*status)),
            None => Err(AppError::Offline),
        }
    }

    fn download_image(&self, _size: &str, _remote: &str) -> AppResult<Vec<u8>> {
        *lock(&self.downloads) += 1;
        if self.images_offline {
            Err(AppError::Offline)
        } else {
            Ok(b"image".to_vec())
        }
    }
}
