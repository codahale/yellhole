use std::io;
use std::path::Path;
use std::sync::Arc;

use include_dir::{include_dir, Dir};
use tempdir::TempDir;

#[derive(Debug, Clone)]
pub struct AssetService {
    dir: Arc<TempDir>,
}

impl AssetService {
    pub fn new() -> io::Result<AssetService> {
        let dir = TempDir::new("yellhole-assets")?;
        ASSET_DIR.extract(dir.path())?;
        Ok(AssetService { dir: Arc::new(dir) })
    }

    pub fn assets_dir(&self) -> &Path {
        self.dir.path()
    }
}

static ASSET_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/assets");
