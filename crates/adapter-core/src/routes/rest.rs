use axum::{
    extract::{Extension, Path, Query, State},
    http::HeaderMap,
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::{
    auth::{
        authenticated_user_from_headers,
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
    db::{entity_store, DatabaseBackend},
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
struct RuntimeSummary {
    auth: AuthRouteSummary,
    entity_count: usize,
    manifest_name: String,
    realtime_path: Option<String>,
}

#[derive(Clone, Serialize)]
struct ErrorResponse {
    message: String,
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
        router = router.route("/", get(list_entity_handler));
    }
    if entity.rest.allow_create {
        router = router.route("/", post(create_entity_handler));
    }
    if entity.rest.allow_get_by_id {
        router = router.route("/{id}", get(get_entity_by_id_handler));
    }
    if entity.rest.allow_update {
        router = router.route("/{id}", put(update_entity_handler));
    }
    if entity.rest.allow_delete {
        router = router.route("/{id}", delete(delete_entity_handler));
    }

    router.layer(Extension(entity))
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

async fn create_entity_handler(
    headers: HeaderMap,
    Extension(entity): Extension<EntityManifest>,
    State(state): State<AdapterRuntimeState>,
    Json(body): Json<Value>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    ensure_authenticated_entity_request(&headers, &state).await?;
    let payload = require_object_body(body)?;
    let created = entity_store::create_entity_record(
        state.database.pool(),
        state.manifest.as_ref(),
        &entity,
        &payload,
    )
    .await
    .map_err(internal_server_error)?;

    Ok((StatusCode::CREATED, Json(created)))
}

async fn delete_entity_handler(
    headers: HeaderMap,
    Extension(entity): Extension<EntityManifest>,
    Path(id): Path<String>,
    State(state): State<AdapterRuntimeState>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    ensure_authenticated_entity_request(&headers, &state).await?;
    entity_store::delete_entity_record(
        state.database.pool(),
        state.manifest.as_ref(),
        &entity,
        &id,
    )
    .await
    .map_err(internal_server_error)?;

    Ok(StatusCode::NO_CONTENT)
}

async fn get_entity_by_id_handler(
    headers: HeaderMap,
    Extension(entity): Extension<EntityManifest>,
    Path(id): Path<String>,
    State(state): State<AdapterRuntimeState>,
) -> Result<Json<Value>, (StatusCode, Json<ErrorResponse>)> {
    ensure_authenticated_entity_request(&headers, &state).await?;
    let record = entity_store::get_entity_record_by_id(
        state.database.pool(),
        state.manifest.as_ref(),
        &entity,
        &id,
    )
    .await
    .map_err(internal_server_error)?
    .unwrap_or(Value::Null);

    Ok(Json(record))
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

async fn list_entity_handler(
    headers: HeaderMap,
    Extension(entity): Extension<EntityManifest>,
    State(state): State<AdapterRuntimeState>,
) -> Result<Json<Value>, (StatusCode, Json<ErrorResponse>)> {
    ensure_authenticated_entity_request(&headers, &state).await?;
    let records = entity_store::list_entity_records(
        state.database.pool(),
        state.manifest.as_ref(),
        &entity,
    )
    .await
    .map_err(internal_server_error)?;

    Ok(Json(Value::Array(records)))
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

async fn update_entity_handler(
    headers: HeaderMap,
    Extension(entity): Extension<EntityManifest>,
    Path(id): Path<String>,
    State(state): State<AdapterRuntimeState>,
    Json(body): Json<Value>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    ensure_authenticated_entity_request(&headers, &state).await?;
    let payload = require_object_body(body)?;
    let Some(updated) = entity_store::update_entity_record(
        state.database.pool(),
        state.manifest.as_ref(),
        &entity,
        &id,
        &payload,
    )
    .await
    .map_err(internal_server_error)? else {
        return Err(not_found_error(format!(
            "Entity '{}' with id '{}' was not found.",
            entity.name, id
        )));
    };

    Ok((StatusCode::OK, Json(updated)))
}

fn internal_server_error(error: anyhow::Error) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse {
            message: error.to_string(),
        }),
    )
}

async fn ensure_authenticated_entity_request(
    headers: &HeaderMap,
    state: &AdapterRuntimeState,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    if !rest_entity_requests_require_authentication(state) {
        return Ok(());
    }

    let user = authenticated_user_from_headers(
        headers,
        state.database.pool(),
        &state.manifest.auth.session,
    )
    .await
    .map_err(auth_error_response)?;

    if user.is_some() {
        Ok(())
    } else {
        Err(unauthorized_error("Authentication required.".to_owned()))
    }
}

fn auth_error_response(error: AuthError) -> (StatusCode, Json<ErrorResponse>) {
    (
        error.status(),
        Json(ErrorResponse {
            message: error.message().to_owned(),
        }),
    )
}

fn not_found_error(message: String) -> (StatusCode, Json<ErrorResponse>) {
    (StatusCode::NOT_FOUND, Json(ErrorResponse { message }))
}

fn rest_entity_requests_require_authentication(state: &AdapterRuntimeState) -> bool {
    state
        .manifest
        .database
        .expected_schema
        .api
        .rest
        .as_ref()
        .is_some_and(|rest| rest.authenticated)
}

fn unauthorized_error(message: String) -> (StatusCode, Json<ErrorResponse>) {
    (StatusCode::UNAUTHORIZED, Json(ErrorResponse { message }))
}

fn require_object_body(
    value: Value,
) -> Result<Map<String, Value>, (StatusCode, Json<ErrorResponse>)> {
    value.as_object().cloned().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                message: "REST entity request body must be a JSON object.".to_owned(),
            }),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{require_object_body, unauthorized_error};
    use axum::http::StatusCode;
    use serde_json::json;

    #[test]
    fn require_object_body_rejects_non_objects() {
        let result = require_object_body(json!("nope"));

        assert!(result.is_err());
        let (status, _) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn unauthorized_error_uses_http_401() {
        let (status, _) = unauthorized_error("Authentication required.".to_owned());

        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}
