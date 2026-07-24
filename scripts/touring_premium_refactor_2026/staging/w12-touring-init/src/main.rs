//! `touring-init` — rustup-like installer for per-project Touring.
//!
//! Subcommands:
//!   - init       Bootstrap <project>/.touring/ structure
//!   - set-channel Pin <project> to a specific Touring toolchain
//!   - status     Report toolchain versions + per-project config
//!   - update     Pull latest stable channel into ~/.touring/toolchains/

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "touring-init", version, about)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Initialize <project>/.touring/ structure
    Init {
        #[arg(long, default_value = "stable")]
        channel: String,
    },
    /// Pin project to a specific channel/version
    SetChannel { channel: String },
    /// Show toolchain + per-project status
    Status,
    /// Update toolchain cache
    Update,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Init { channel } => init(&channel),
        Cmd::SetChannel { channel } => set_channel(&channel),
        Cmd::Status => status(),
        Cmd::Update => update(),
    }
}

fn init(channel: &str) -> anyhow::Result<()> {
    println!("Initializing project with channel: {channel}");
    // TODO: W12.2 — actual scaffolding logic
    Ok(())
}

fn set_channel(channel: &str) -> anyhow::Result<()> {
    println!("Pinning to channel: {channel}");
    Ok(())
}

fn status() -> anyhow::Result<()> {
    println!("Touring status: (W12.3 placeholder)");
    Ok(())
}

fn update() -> anyhow::Result<()> {
    println!("Updating toolchain cache: (W12.4 placeholder)");
    Ok(())
}
