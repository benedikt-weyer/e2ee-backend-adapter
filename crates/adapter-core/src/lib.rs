pub mod auth;
pub mod db;
pub mod manifest;
pub mod routes;
pub mod schema;

use std::{collections::BTreeMap, future::Future, net::SocketAddr, pin::Pin, sync::Arc};

use anyhow::Result;
use axum::Router;
use axum::http::{HeaderMap, HeaderValue};
use db::{DatabaseBackend, PostgresBackend};
use manifest::BackendAdapterManifest;
use serde_json::Value;

pub type CustomGraphqlHandlerFuture = Pin<Box<dyn Future<Output = Result<CustomOperationResponse, String>> + Send>>;
pub type CustomRestHandlerFuture = Pin<Box<dyn Future<Output = Result<CustomOperationResponse, String>> + Send>>;
pub type CustomGraphqlHandler = Arc<dyn Fn(CustomGraphqlRequest) -> CustomGraphqlHandlerFuture + Send + Sync>;
pub type CustomRestHandler = Arc<dyn Fn(CustomRestRequest) -> CustomRestHandlerFuture + Send + Sync>;

#[derive(Clone)]
pub struct CustomOperationResponse {
    pub cookies: Vec<HeaderValue>,
    pub data: Value,
}

#[derive(Clone)]
pub struct CustomGraphqlRequest {
    pub headers: HeaderMap,
    pub input: Option<Value>,
    pub operation_name: String,
    pub state: AdapterRuntimeState,
    pub variables: Value,
}

#[derive(Clone)]
pub struct CustomRestRequest {
    pub headers: HeaderMap,
    pub input: Option<Value>,
    pub operation_name: String,
    pub state: AdapterRuntimeState,
}

#[derive(Clone, Default)]
pub struct AdapterRuntimeOptions {
    pub custom_graphql_handlers: BTreeMap<String, CustomGraphqlHandler>,
    pub custom_rest_handlers: BTreeMap<String, CustomRestHandler>,
    pub secure_cookies: bool,
}

#[derive(Clone)]
pub struct AdapterRuntimeState {
    pub custom_graphql_handlers: Arc<BTreeMap<String, CustomGraphqlHandler>>,
    pub custom_rest_handlers: Arc<BTreeMap<String, CustomRestHandler>>,
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
                custom_graphql_handlers: Arc::new(options.custom_graphql_handlers),
                custom_rest_handlers: Arc::new(options.custom_rest_handlers),
                database: Arc::new(database),
                manifest: Arc::new(manifest),
                secure_cookies: options.secure_cookies,
            },
        })
    }

    pub fn router(&self) -> Router {
        let mut router = routes::rest::build_router(self.state.clone());

        if let Some(graphql) = self
            .state
            .manifest
            .database
            .expected_schema
            .api
            .graphql
            .as_ref()
        {
            router = router.merge(routes::graphql::build_router(
                self.state.clone(),
                &graphql.endpoint_path,
            ));
        }

        router
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
