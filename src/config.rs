use std::path::PathBuf;

use clap::Parser;
use url::Url;

#[derive(Debug, Clone, Parser)]
pub struct Config {
    /// The port on which to listen. Binds to 0.0.0.0.
    #[clap(long, default_value = "3000", env("PORT"))]
    pub port: u16,

    /// The base URL of the server.
    #[clap(long, default_value = "http://localhost:3000", env("BASE_URL"))]
    pub base_url: Url,

    /// The directory in which all persistent data is stored.
    #[clap(long, default_value = "./data", env("DATA_DIR"))]
    pub data_dir: PathBuf,

    /// The title of the Yellhole instance.
    #[clap(long, default_value = "Yellhole", env("TITLE"))]
    pub title: String,

    /// The name of the person posting this crap.
    #[clap(long, default_value = "Luther Blissett", env("AUTHOR"))]
    pub author: String,
}
