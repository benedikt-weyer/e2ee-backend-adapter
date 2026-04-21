use std::{net::SocketAddr, path::PathBuf};

use anyhow::{Context, Result};
use clap::Parser;
use e2ee_backend_adapter_core::{manifest::parse_manifest, AdapterRuntime};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(author, version, about = "Run the e2ee backend adapter REST server")]
struct Args {
    #[arg(long, env = "DATABASE_URL")]
    database_url: String,

    #[arg(long, default_value = "127.0.0.1:8080", env = "E2EE_ADAPTER_BIND")]
    bind: SocketAddr,

    #[arg(long, env = "E2EE_ADAPTER_MANIFEST")]
    manifest: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let args = Args::parse();
    let manifest_json = std::fs::read_to_string(&args.manifest)
        .with_context(|| format!("Failed to read manifest file at {}", args.manifest.display()))?;
    let manifest = parse_manifest(&manifest_json)?;
    let runtime = AdapterRuntime::from_manifest(manifest, &args.database_url).await?;
    runtime.verify_database().await?;

    let router = runtime
        .router()
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(args.bind).await?;
    axum::serve(listener, router).await?;
    Ok(())
}
