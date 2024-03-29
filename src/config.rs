use std::{net::IpAddr, path::PathBuf};

use clap::Parser;
use url::Url;

#[derive(Debug, Parser)]
pub struct Config {
    /// The address on which to listen.
    #[arg(long, default_value = "127.0.0.1", env("ADDR"))]
    pub addr: IpAddr,

    /// The port on which to listen.
    #[arg(long, default_value = "3000", env("PORT"))]
    pub port: u16,

    /// The base URL of the server.
    #[arg(long, default_value = "http://localhost:3000", env("BASE_URL"))]
    pub base_url: Url,

    /// The directory in which all persistent data is stored.
    #[arg(long, default_value = "./data", env("DATA_DIR"))]
    pub data_dir: PathBuf,

    /// The title of the Yellhole instance.
    #[arg(long, default_value = "Yellhole", env("TITLE"))]
    pub title: String,

    /// The description of the Yellhole instance.
    #[arg(long, default_value = "Obscurantist filth.", env("DESCRIPTION"))]
    pub description: String,

    /// The name of the person posting this crap.
    #[arg(long, default_value = "Luther Blissett", env("AUTHOR"))]
    pub author: String,
}
