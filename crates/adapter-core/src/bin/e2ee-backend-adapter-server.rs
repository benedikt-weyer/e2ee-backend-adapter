use std::{net::SocketAddr, path::PathBuf};

use anyhow::{Context, Result};
use clap::Parser;
use e2ee_backend_adapter::{manifest::parse_manifest, AdapterRuntime, AdapterRuntimeOptions};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(author, version, about = "Run the e2ee backend adapter server")]
struct Args {
    #[arg(long, env = "DATABASE_URL")]
    database_url: String,

    #[arg(long, default_value = "127.0.0.1:8080", env = "E2EE_ADAPTER_BIND")]
    bind: SocketAddr,

    #[arg(long, env = "E2EE_ADAPTER_MANIFEST")]
    manifest: PathBuf,

    #[arg(long, default_value_t = false, env = "E2EE_ADAPTER_SECURE_COOKIES")]
    secure_cookies: bool,
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
    let runtime = AdapterRuntime::from_manifest_with_options(
        manifest,
        &args.database_url,
        AdapterRuntimeOptions {
            secure_cookies: args.secure_cookies,
            ..AdapterRuntimeOptions::default()
        },
    )
    .await?;
    runtime.verify_database().await?;

    let router = runtime
        .router()
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(args.bind).await?;
    axum::serve(listener, router).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Args;
    use clap::Parser;
    use std::{
        net::{IpAddr, Ipv4Addr, SocketAddr},
        path::PathBuf,
    };

    #[test]
    fn parses_required_server_arguments() {
        let args = Args::try_parse_from([
            "adapter-server",
            "--database-url",
            "postgres://postgres:postgres@localhost:5432/app",
            "--manifest",
            "/tmp/manifest.json",
        ])
        .expect("arguments should parse");

        assert_eq!(
            args.bind,
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8080),
        );
        assert_eq!(args.manifest, PathBuf::from("/tmp/manifest.json"));
        assert!(!args.secure_cookies);
    }

    #[test]
    fn parses_explicit_bind_and_secure_cookie_flag() {
        let args = Args::try_parse_from([
            "adapter-server",
            "--database-url",
            "postgres://postgres:postgres@localhost:5432/app",
            "--manifest",
            "/tmp/manifest.json",
            "--bind",
            "0.0.0.0:9090",
            "--secure-cookies",
        ])
        .expect("arguments should parse");

        assert_eq!(
            args.bind,
            SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 9090),
        );
        assert!(args.secure_cookies);
    }
}