//! The on-disk image cache under `<data_dir>/images`.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::AppResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    Poster,
    Backdrop,
    Profile,
    Still,
}

impl ImageKind {
    const ALL: [Self; 4] = [Self::Poster, Self::Backdrop, Self::Profile, Self::Still];

    fn folder(self) -> &'static str {
        match self {
            Self::Poster => "posters",
            Self::Backdrop => "backdrops",
            Self::Profile => "profiles",
            Self::Still => "stills",
        }
    }

    /// The TMDB size to download.
    pub fn size(self) -> &'static str {
        match self {
            Self::Poster => "w342",
            Self::Backdrop => "w1280",
            Self::Profile => "w185",
            Self::Still => "w300",
        }
    }
}

/// The file name in a TMDB image path such as `/abc.jpg`. Remote data is not
/// trusted: anything that could leave the cache folder (or alter a URL) is
/// rejected.
pub fn file_name(remote: &str) -> Option<&str> {
    let name = remote.strip_prefix('/')?;
    let safe = !name.is_empty()
        && !name.starts_with('.')
        && !name.contains("..")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    safe.then_some(name)
}

pub struct ImageCache {
    root: PathBuf,
}

impl ImageCache {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            root: data_dir.join("images"),
        }
    }

    /// Returns the local file for `name` (see [`file_name`]), calling
    /// `download` only if it is not cached yet. A partly written file is never
    /// visible under its final name.
    pub fn fetch(
        &self,
        kind: ImageKind,
        name: &str,
        download: impl FnOnce() -> AppResult<Vec<u8>>,
    ) -> AppResult<PathBuf> {
        static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

        let folder = self.root.join(kind.folder());
        let target = folder.join(name);
        if fs::metadata(&target).is_ok_and(|meta| meta.is_file() && meta.len() > 0) {
            return Ok(target);
        }
        let bytes = download()?;
        fs::create_dir_all(&folder)?;
        // Cached names never start with a dot, so this cannot collide with one.
        let temp = folder.join(format!(
            ".{}-{}.part",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let written = fs::write(&temp, bytes).and_then(|()| fs::rename(&temp, &target));
        if let Err(error) = written {
            let _ = fs::remove_file(&temp);
            return Err(error.into());
        }
        Ok(target)
    }

    pub fn clear(&self) -> AppResult<()> {
        for kind in ImageKind::ALL {
            match fs::remove_dir_all(self.root.join(kind.folder())) {
                Err(error) if error.kind() != ErrorKind::NotFound => return Err(error.into()),
                _ => {}
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AppError;
    use crate::tmdb::refresh::testing::TempDir;

    #[test]
    fn only_plain_file_names_are_accepted() {
        assert_eq!(file_name("/aB3_-x.jpg"), Some("aB3_-x.jpg"));
        for bad in [
            "",
            "/",
            "abc.jpg",
            "//etc/passwd",
            "/../x.jpg",
            "/a/b.jpg",
            "/a\\b.jpg",
            "/..",
            "/.hidden",
            "/a..jpg",
            "/a.jpg?x=1",
            "/%2e%2e.jpg",
        ] {
            assert_eq!(file_name(bad), None, "{bad}");
        }
    }

    #[test]
    fn downloads_once_and_clears() {
        let dir = TempDir::new("cache");
        let cache = ImageCache::new(dir.path());
        let path = cache
            .fetch(ImageKind::Poster, "a.jpg", || Ok(b"img".to_vec()))
            .unwrap();
        assert_eq!(path, dir.path().join("images/posters/a.jpg"));
        let again = cache
            .fetch(ImageKind::Poster, "a.jpg", || Err(AppError::Offline))
            .unwrap();
        assert_eq!(again, path);
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);

        cache.clear().unwrap();
        assert!(!path.exists());
        cache.clear().unwrap();
    }

    #[test]
    fn a_failed_download_leaves_nothing_behind() {
        let dir = TempDir::new("cache-fail");
        let cache = ImageCache::new(dir.path());
        let result = cache.fetch(ImageKind::Still, "a.jpg", || Err(AppError::Offline));
        assert!(matches!(result, Err(AppError::Offline)));
        assert!(!dir.path().join("images/stills/a.jpg").exists());
    }
}
