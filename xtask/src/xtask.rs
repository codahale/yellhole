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
    Ci,

    /// Database commands.
    Db {
        #[clap(subcommand)]
        cmd: DatabaseCommand,
    },

    /// Run the server, watch for changes, and restart as needed.
    Watch,
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

    match xtask.cmd.unwrap_or(Command::Ci) {
        Command::Ci => ci(&sh),
        Command::Db { cmd } => match cmd {
            DatabaseCommand::Setup => db_setup(&sh),
            DatabaseCommand::Reset => db_reset(&sh),
            DatabaseCommand::Drop => db_drop(&sh),
            DatabaseCommand::Migrate => db_migrate(&sh),
            DatabaseCommand::Prepare => db_prepare(&sh),
        },
        Command::Watch => watch(&sh),
    }
}

fn ci(sh: &Shell) -> Result<(), anyhow::Error> {
    cmd!(sh, "cargo fmt --check").env("SQLX_OFFLINE", "true").run()?;
    cmd!(sh, "cargo build --all-targets --all-features").env("SQLX_OFFLINE", "true").run()?;
    cmd!(sh, "cargo test --all-features").env("SQLX_OFFLINE", "true").run()?;
    cmd!(sh, "cargo clippy --all-features --tests --benches").env("SQLX_OFFLINE", "true").run()?;
    Ok(())
}

const DB_URL: &str = "sqlite:./data/yellhole.db";

fn db_setup(sh: &Shell) -> Result<(), anyhow::Error> {
    cmd!(sh, "sqlx db setup --database-url={DB_URL}").run()?;
    Ok(())
}

fn db_reset(sh: &Shell) -> Result<(), anyhow::Error> {
    cmd!(sh, "sqlx db reset --database-url={DB_URL}").run()?;
    Ok(())
}

fn db_drop(sh: &Shell) -> Result<(), anyhow::Error> {
    cmd!(sh, "sqlx db drop --database-url={DB_URL}").run()?;
    Ok(())
}

fn db_migrate(sh: &Shell) -> Result<(), anyhow::Error> {
    cmd!(sh, "sqlx migrate run --database-url={DB_URL}").run()?;
    Ok(())
}

fn db_prepare(sh: &Shell) -> Result<(), anyhow::Error> {
    cmd!(sh, "rm -f sqlx-data.json").run()?;
    cmd!(sh, "cargo sqlx prepare -- --tests").env("DATABASE_URL", DB_URL).run()?;
    Ok(())
}

fn watch(sh: &Shell) -> Result<()> {
    cmd!(sh, "cargo watch -x run").run()?;

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
