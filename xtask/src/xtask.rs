use std::env;
use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::{Parser, Subcommand};
use xshell::{cmd, Shell};

#[derive(Debug, Parser)]
struct XTask {
    #[clap(subcommand)]
    cmd: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Format, build, test, and lint.
    CI,
}

fn main() -> Result<()> {
    let _ = XTask::parse();

    let sh = Shell::new()?;
    sh.change_dir(project_root());

    cmd!(sh, "cargo fmt --check").run()?;
    cmd!(sh, "cargo build --all-targets --all-features").run()?;
    cmd!(sh, "cargo test --all-features").run()?;
    cmd!(sh, "cargo clippy --all-features --tests --benches").run()?;

    Ok(())
}

fn project_root() -> PathBuf {
    Path::new(
        &env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| env!("CARGO_MANIFEST_DIR").to_owned()),
    )
    .ancestors()
    .nth(1)
    .unwrap()
    .to_path_buf()
}
