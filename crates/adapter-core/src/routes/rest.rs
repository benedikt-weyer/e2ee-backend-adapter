use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Serialize;

use crate::{
    auth::{
        attach_cookies,
        get_kdf_salt,
        login,
        logout,
        refresh,
        register_begin,
        register_complete,
        AuthError,
        AuthKeyBody,
        AuthRouteSummary,
        EmailBody,
        EmailQuery,
        KdfSaltResponse,
    },
    db::DatabaseBackend,
    manifest::{BackendAdapterManifest, EntityManifest},
    AdapterRuntimeState,
};

#[derive(Clone, Serialize)]
struct HealthResponse {
    database_engine: String,
    manifest_name: String,
    status: &'static str,
}

#[derive(Clone, Serialize)]
struct PlaceholderResponse {
    entity: Option<String>,
    message: &'static str,
    operation: String,
}

#[derive(Clone, Serialize)]
struct RuntimeSummary {
    auth: AuthRouteSummary,
    entity_count: usize,
    manifest_name: String,
    realtime_path: Option<String>,
}

pub fn build_router(state: AdapterRuntimeState) -> Router {
    let manifest = state.manifest.clone();

    let mut router = Router::new()
        .route("/health", get(health))
        .route("/adapter/manifest", get(get_manifest))
        .route("/adapter/runtime", get(runtime_summary));

    router = add_auth_routes(router, manifest.as_ref());

    for entity in &manifest.entities {
        router = router.nest(&entity.rest.base_path, entity_router(entity.clone()));
    }

    router.with_state(state)
}

fn add_auth_routes(router: Router<AdapterRuntimeState>, manifest: &BackendAdapterManifest) -> Router<AdapterRuntimeState> {
    router
    .route(&manifest.auth.rest.paths.get_kdf_salt, get(get_kdf_salt_handler))
    .route(&manifest.auth.rest.paths.login, post(login_handler))
    .route(&manifest.auth.rest.paths.logout, post(logout_handler))
    .route(&manifest.auth.rest.paths.refresh, post(refresh_handler))
    .route(&manifest.auth.rest.paths.register_begin, post(register_begin_handler))
    .route(&manifest.auth.rest.paths.register_complete, post(register_complete_handler))
}

fn entity_router(entity: EntityManifest) -> Router<AdapterRuntimeState> {
    let mut router = Router::new();

    if entity.rest.allow_list {
        router = router.route("/", get(list_placeholder));
    }
    if entity.rest.allow_create {
        router = router.route("/", post(create_placeholder));
    }
    if entity.rest.allow_get_by_id {
        router = router.route("/{id}", get(get_by_id_placeholder));
    }
    if entity.rest.allow_update {
        router = router.route("/{id}", put(update_placeholder));
    }
    if entity.rest.allow_delete {
        router = router.route("/{id}", delete(delete_placeholder));
    }

    router
}

async fn get_kdf_salt_handler(
    Query(query): Query<EmailQuery>,
    State(state): State<AdapterRuntimeState>,
) -> Result<Json<KdfSaltResponse>, AuthError> {
    Ok(Json(get_kdf_salt(state.database.pool(), &query.email).await?))
}

async fn login_handler(
    State(state): State<AdapterRuntimeState>,
    Json(body): Json<AuthKeyBody>,
) -> Result<impl IntoResponse, AuthError> {
    let result = login(
        state.database.pool(),
        body,
        &state.manifest.auth.session,
        state.secure_cookies,
    )
    .await?;
    Ok(attach_cookies(result.payload, result.cookies))
}

async fn logout_handler(
    headers: HeaderMap,
    State(state): State<AdapterRuntimeState>,
) -> Result<impl IntoResponse, AuthError> {
    let cookies = logout(
        &headers,
        state.database.pool(),
        &state.manifest.auth.session,
        state.secure_cookies,
    )
    .await?;
    Ok(attach_cookies(true, cookies))
}

async fn refresh_handler(
    headers: HeaderMap,
    State(state): State<AdapterRuntimeState>,
) -> Result<impl IntoResponse, AuthError> {
    let result = refresh(
        &headers,
        state.database.pool(),
        &state.manifest.auth.session,
        state.secure_cookies,
    )
    .await?;
    Ok(attach_cookies(result.payload, result.cookies))
}

async fn register_begin_handler(
    State(state): State<AdapterRuntimeState>,
    Json(body): Json<EmailBody>,
) -> Result<Json<KdfSaltResponse>, AuthError> {
    Ok(Json(register_begin(state.database.pool(), body).await?))
}

async fn register_complete_handler(
    State(state): State<AdapterRuntimeState>,
    Json(body): Json<AuthKeyBody>,
) -> Result<impl IntoResponse, AuthError> {
    let result = register_complete(
        state.database.pool(),
        body,
        &state.manifest.auth.session,
        state.secure_cookies,
    )
    .await?;
    Ok(attach_cookies(result.payload, result.cookies))
}

async fn create_placeholder() -> impl IntoResponse {
    placeholder_response("create", None)
}

async fn delete_placeholder(Path(id): Path<String>) -> impl IntoResponse {
    placeholder_response("delete", Some(id))
}

async fn get_by_id_placeholder(Path(id): Path<String>) -> impl IntoResponse {
    placeholder_response("get-by-id", Some(id))
}

async fn get_manifest(State(state): State<AdapterRuntimeState>) -> impl IntoResponse {
    Json(state.manifest.as_ref().clone())
}

async fn health(State(state): State<AdapterRuntimeState>) -> impl IntoResponse {
    Json(HealthResponse {
        database_engine: state.database.engine().to_owned(),
        manifest_name: state.manifest.name.clone(),
        status: "ok",
    })
}

async fn list_placeholder() -> impl IntoResponse {
    placeholder_response("list", None)
}

fn placeholder_response(operation: &str, id: Option<String>) -> impl IntoResponse {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(PlaceholderResponse {
            entity: id,
            message: "Generated REST routes are scaffolded but not implemented yet.",
            operation: operation.to_owned(),
        }),
    )
}

async fn runtime_summary(State(state): State<AdapterRuntimeState>) -> impl IntoResponse {
    Json(RuntimeSummary {
        auth: AuthRouteSummary {
            get_kdf_salt: state.manifest.auth.rest.paths.get_kdf_salt.clone(),
            login: state.manifest.auth.rest.paths.login.clone(),
            logout: state.manifest.auth.rest.paths.logout.clone(),
            refresh: state.manifest.auth.rest.paths.refresh.clone(),
            register_begin: state.manifest.auth.rest.paths.register_begin.clone(),
            register_complete: state.manifest.auth.rest.paths.register_complete.clone(),
        },
        entity_count: state.manifest.entities.len(),
        manifest_name: state.manifest.name.clone(),
        realtime_path: state.manifest.realtime.as_ref().map(|value| value.path.clone()),
    })
}

async fn update_placeholder(Path(id): Path<String>) -> impl IntoResponse {
    placeholder_response("update", Some(id))
}
