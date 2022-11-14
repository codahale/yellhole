use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::{fs, io};

#[derive(Debug, Clone)]
pub struct Author(pub String);

impl FromStr for Author {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Author(s.into()))
    }
}

#[derive(Debug, Clone)]
pub struct Title(pub String);

impl FromStr for Title {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Title(s.into()))
    }
}

#[derive(Debug, Clone)]
pub struct DataDir {
    dir: PathBuf,
    images_dir: PathBuf,
    uploads_dir: PathBuf,
}

impl DataDir {
    pub fn new(dir: impl AsRef<Path>) -> Result<DataDir, io::Error> {
        let dir = dir.as_ref().canonicalize()?;

        let images_dir = dir.join("images");
        tracing::info!(?images_dir, "creating directory");
        fs::create_dir_all(&images_dir)?;

        let uploads_dir = dir.join("uploads");
        tracing::info!(?uploads_dir, "creating directory");
        fs::create_dir_all(&uploads_dir)?;

        Ok(DataDir { dir, images_dir, uploads_dir })
    }

    pub fn db_path(&self) -> PathBuf {
        self.dir.join("yellhole.db")
    }

    pub fn images_dir(&self) -> &Path {
        self.images_dir.as_path()
    }

    pub fn original_path(&self, image_id: &str, content_type: &mime::Mime) -> PathBuf {
        self.uploads_dir.join(format!("{image_id}.orig.{}", content_type.subtype()))
    }

    pub fn main_path(&self, image_id: &str) -> PathBuf {
        self.images_dir.join(format!("{image_id}.main.webp"))
    }

    pub fn thumbnail_path(&self, image_id: &str) -> PathBuf {
        self.images_dir.join(format!("{image_id}.thumb.webp"))
    }
}
