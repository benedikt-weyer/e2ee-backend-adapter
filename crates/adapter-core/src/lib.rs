pub mod auth;
pub mod db;
pub mod manifest;
pub mod routes;
pub mod schema;

use std::{net::SocketAddr, sync::Arc};

use anyhow::Result;
use axum::Router;
use db::{DatabaseBackend, PostgresBackend};
use manifest::BackendAdapterManifest;

#[derive(Clone, Copy, Debug, Default)]
pub struct AdapterRuntimeOptions {
    pub secure_cookies: bool,
}

#[derive(Clone)]
pub struct AdapterRuntimeState {
    pub database: Arc<PostgresBackend>,
    pub manifest: Arc<BackendAdapterManifest>,
    pub secure_cookies: bool,
}

pub struct AdapterRuntime {
    state: AdapterRuntimeState,
}

impl AdapterRuntime {
    pub async fn from_manifest(
        manifest: BackendAdapterManifest,
        database_url: &str,
    ) -> Result<Self> {
        Self::from_manifest_with_options(manifest, database_url, AdapterRuntimeOptions::default())
            .await
    }

    pub async fn from_manifest_with_options(
        manifest: BackendAdapterManifest,
        database_url: &str,
        options: AdapterRuntimeOptions,
    ) -> Result<Self> {
        manifest.validate()?;
        let database = PostgresBackend::connect(database_url).await?;

        Ok(Self {
            state: AdapterRuntimeState {
                database: Arc::new(database),
                manifest: Arc::new(manifest),
                secure_cookies: options.secure_cookies,
            },
        })
    }

    pub fn router(&self) -> Router {
        routes::rest::build_router(self.state.clone())
    }

    pub async fn verify_database(&self) -> Result<()> {
        self.state.database.ping().await
    }

    pub fn manifest(&self) -> &BackendAdapterManifest {
        self.state.manifest.as_ref()
    }

    pub fn state(&self) -> AdapterRuntimeState {
        self.state.clone()
    }
}

pub async fn serve(runtime: AdapterRuntime, bind: SocketAddr) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(listener, runtime.router()).await?;
    Ok(())
}
