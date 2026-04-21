use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use e2ee_backend_adapter_core::{
    manifest::parse_manifest,
    schema::{diff::diff_database_against_manifest, export::export_expected_schema},
};

#[derive(Debug, Parser)]
#[command(author, version, about = "CLI for e2ee-backend-adapter tooling")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Diff {
        #[arg(long, env = "DATABASE_URL")]
        database_url: String,
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    ExportExpectedSchema {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    ValidateManifest {
        #[arg(long)]
        manifest: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Diff {
            database_url,
            manifest,
            out,
        } => {
            let manifest = load_manifest(&manifest)?;
            let diff = diff_database_against_manifest(&manifest, &database_url).await?;
            fs::write(out, diff)?;
        }
        Command::ExportExpectedSchema { manifest, out } => {
            let manifest = load_manifest(&manifest)?;
            let expected = export_expected_schema(&manifest)?;
            fs::write(out, expected)?;
        }
        Command::ValidateManifest { manifest } => {
            let _ = load_manifest(&manifest)?;
        }
    }

    Ok(())
}

fn load_manifest(path: &PathBuf) -> Result<e2ee_backend_adapter_core::manifest::BackendAdapterManifest> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read manifest file at {}", path.display()))?;
    parse_manifest(&content)
}
