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

    /// Database commands.
    DB {
        #[clap(subcommand)]
        cmd: DatabaseCommand,
    },
}

#[derive(Debug, Subcommand)]
enum DatabaseCommand {
    Setup,
    Reset,
    Drop,
    Migrate,
    Prepare,
}

fn main() -> Result<()> {
    let xtask = XTask::parse();

    let sh = Shell::new()?;
    sh.change_dir(project_root());

    match xtask.cmd.unwrap_or(Command::CI) {
        Command::CI => ci(&sh),
        Command::DB { cmd } => match cmd {
            DatabaseCommand::Setup => db_setup(&sh),
            DatabaseCommand::Reset => db_reset(&sh),
            DatabaseCommand::Drop => db_drop(&sh),
            DatabaseCommand::Migrate => db_migrate(&sh),
            DatabaseCommand::Prepare => db_prepare(&sh),
        },
    }
}

fn ci(sh: &Shell) -> Result<(), anyhow::Error> {
    cmd!(sh, "cargo fmt --check").run()?;
    cmd!(sh, "cargo build --all-targets --all-features").run()?;
    cmd!(sh, "cargo test --all-features").run()?;
    cmd!(sh, "cargo clippy --all-features --tests --benches").run()?;
    Ok(())
}

fn db_setup(sh: &Shell) -> Result<(), anyhow::Error> {
    cmd!(sh, "sqlx db setup").run()?;
    Ok(())
}

fn db_reset(sh: &Shell) -> Result<(), anyhow::Error> {
    cmd!(sh, "sqlx db reset").run()?;
    Ok(())
}

fn db_drop(sh: &Shell) -> Result<(), anyhow::Error> {
    cmd!(sh, "sqlx db drop").run()?;
    Ok(())
}

fn db_migrate(sh: &Shell) -> Result<(), anyhow::Error> {
    cmd!(sh, "sqlx migrate run").run()?;
    Ok(())
}

fn db_prepare(sh: &Shell) -> Result<(), anyhow::Error> {
    cmd!(sh, "cargo sqlx prepare -- --tests").run()?;
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
