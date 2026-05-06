use axum::{
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::{
    auth::{
        authenticated_user_from_headers, get_kdf_salt, login, logout, refresh, register_begin,
        register_complete, AuthKeyBody, EmailBody,
    },
    db::entity_store,
    AdapterRuntimeState,
};

pub fn build_router(state: AdapterRuntimeState, endpoint_path: &str) -> Router {
    Router::new()
        .route(endpoint_path, post(graphql_handler))
        .with_state(state)
}

#[derive(Debug, Deserialize)]
struct GraphqlRequest {
    query: String,
    #[serde(default)]
    variables: Option<Value>,
}

#[derive(Debug, Serialize)]
struct GraphqlError {
    message: String,
}

#[derive(Debug, Serialize)]
struct GraphqlResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    errors: Vec<GraphqlError>,
}

struct GraphqlExecutionResult {
    cookies: Vec<HeaderValue>,
    data: Value,
}

async fn graphql_handler(
    headers: HeaderMap,
    State(state): State<AdapterRuntimeState>,
    Json(request): Json<GraphqlRequest>,
) -> Response {
    match execute_graphql(headers, state, request).await {
        Ok(result) => attach_graphql_response(Some(result.data), Vec::new(), result.cookies),
        Err(message) => attach_graphql_response(
            None,
            vec![GraphqlError { message }],
            Vec::new(),
        ),
    }
}

async fn execute_graphql(
    headers: HeaderMap,
    state: AdapterRuntimeState,
    request: GraphqlRequest,
) -> Result<GraphqlExecutionResult, String> {
    let variables = request.variables.unwrap_or(Value::Object(Map::new()));
    let root_field = extract_root_field(&request.query)
        .ok_or_else(|| "Could not determine GraphQL root field from request query.".to_owned())?;

    match root_field.as_str() {
        "kdfSalt" => {
            let email = required_string_variable(&variables, "email")?;
            let response = get_kdf_salt(state.database.pool(), &email)
                .await
                .map_err(|error| format!("{error:?}"))?;
            Ok(GraphqlExecutionResult {
                cookies: Vec::new(),
                data: json!({ root_field: response.kdf_salt_base64 }),
            })
        }
        "registerBegin" => {
            let email = required_string_variable(&variables, "email")?;
            let response = register_begin(state.database.pool(), EmailBody { email })
                .await
                .map_err(|error| format!("{error:?}"))?;
            Ok(GraphqlExecutionResult {
                cookies: Vec::new(),
                data: json!({ root_field: response }),
            })
        }
        "login" => {
            let response = login(
                state.database.pool(),
                AuthKeyBody {
                    auth_key_material_hex: required_string_variable(&variables, "authKeyMaterialHex")?,
                    email: required_string_variable(&variables, "email")?,
                },
                &state.manifest.auth.session,
                state.secure_cookies,
            )
            .await
            .map_err(|error| format!("{error:?}"))?;
            Ok(GraphqlExecutionResult {
                cookies: response.cookies,
                data: json!({ root_field: response.payload }),
            })
        }
        "logout" => {
            let cookies = logout(
                &headers,
                state.database.pool(),
                &state.manifest.auth.session,
                state.secure_cookies,
            )
            .await
            .map_err(|error| format!("{error:?}"))?;
            Ok(GraphqlExecutionResult {
                cookies,
                data: json!({ root_field: true }),
            })
        }
        "refreshSession" => {
            let response = refresh(
                &headers,
                state.database.pool(),
                &state.manifest.auth.session,
                state.secure_cookies,
            )
            .await
            .map_err(|error| format!("{error:?}"))?;
            Ok(GraphqlExecutionResult {
                cookies: response.cookies,
                data: json!({ root_field: response.payload }),
            })
        }
        "registerComplete" => {
            let response = register_complete(
                state.database.pool(),
                AuthKeyBody {
                    auth_key_material_hex: required_string_variable(&variables, "authKeyMaterialHex")?,
                    email: required_string_variable(&variables, "email")?,
                },
                &state.manifest.auth.session,
                state.secure_cookies,
            )
            .await
            .map_err(|error| format!("{error:?}"))?;
            Ok(GraphqlExecutionResult {
                cookies: response.cookies,
                data: json!({ root_field: response.payload }),
            })
        }
        _ => execute_entity_graphql(&headers, &root_field, &variables, &state).await,
    }
}

async fn execute_entity_graphql(
    headers: &HeaderMap,
    root_field: &str,
    variables: &Value,
    state: &AdapterRuntimeState,
) -> Result<GraphqlExecutionResult, String> {
    for entity in &state.manifest.entities {
        if root_field == entity.graphql.list_query && entity.graphql.allow_list {
            let current_user_id = resolve_entity_request_user_id(headers, state, entity).await?;
            let records = entity_store::list_entity_records(
                state.database.pool(),
                state.manifest.as_ref(),
                entity,
                current_user_id.as_deref(),
            )
            .await
            .map_err(|error| format!("{error:#}"))?;
            return Ok(GraphqlExecutionResult {
                cookies: Vec::new(),
                data: json!({ root_field: records }),
            });
        }

        if root_field == entity.graphql.get_by_id_query && entity.graphql.allow_get_by_id {
            let current_user_id = resolve_entity_request_user_id(headers, state, entity).await?;
            let id = required_id_variable(variables, "id")?;
            let record = entity_store::get_entity_record_by_id(
                state.database.pool(),
                state.manifest.as_ref(),
                entity,
                &id,
                current_user_id.as_deref(),
            )
            .await
            .map_err(|error| format!("{error:#}"))?;
            return Ok(GraphqlExecutionResult {
                cookies: Vec::new(),
                data: json!({ root_field: record }),
            });
        }

        if root_field == entity.graphql.delete_mutation && entity.graphql.allow_delete {
            let current_user_id = resolve_entity_request_user_id(headers, state, entity).await?;
            let id = required_id_variable(variables, "id")?;
            let deleted = entity_store::delete_entity_record(
                state.database.pool(),
                state.manifest.as_ref(),
                entity,
                &id,
                current_user_id.as_deref(),
            )
            .await
            .map_err(|error| format!("{error:#}"))?;
            return Ok(GraphqlExecutionResult {
                cookies: Vec::new(),
                data: json!({ root_field: deleted }),
            });
        }

        if root_field == entity.graphql.create_mutation && entity.graphql.allow_create {
            let current_user_id = resolve_entity_request_user_id(headers, state, entity).await?;
            let payload = required_object_variable(variables, "input")?;
            let created = entity_store::create_entity_record(
                state.database.pool(),
                state.manifest.as_ref(),
                entity,
                &payload,
                current_user_id.as_deref(),
            )
            .await
            .map_err(|error| format!("{error:#}"))?;

            return Ok(GraphqlExecutionResult {
                cookies: Vec::new(),
                data: json!({ root_field: created }),
            });
        }

        if root_field == entity.graphql.update_mutation && entity.graphql.allow_update {
            let current_user_id = resolve_entity_request_user_id(headers, state, entity).await?;
            let id = required_id_variable(variables, "id")?;
            let payload = required_object_variable(variables, "input")?;
            let updated = entity_store::update_entity_record(
                state.database.pool(),
                state.manifest.as_ref(),
                entity,
                &id,
                &payload,
                current_user_id.as_deref(),
            )
            .await
            .map_err(|error| format!("{error:#}"))?;

            return Ok(GraphqlExecutionResult {
                cookies: Vec::new(),
                data: json!({ root_field: updated }),
            });
        }
    }

    Err(format!("Unsupported GraphQL root field '{root_field}'."))
}

async fn ensure_authenticated_entity_request(
    headers: &HeaderMap,
    state: &AdapterRuntimeState,
    entity: &crate::manifest::EntityManifest,
) -> Result<(), String> {
    let requires_authentication = entity.only_allow_authed_user_filter || state
        .manifest
        .database
        .expected_schema
        .api
        .graphql
        .as_ref()
        .is_some_and(|graphql| graphql.authenticated);
    if !requires_authentication {
        return Ok(());
    }

    let user = authenticated_user_from_headers(
        headers,
        state.database.pool(),
        &state.manifest.auth.session,
    )
    .await
    .map_err(|error| error.message().to_owned())?;

    if user.is_some() {
        Ok(())
    } else {
        Err("Authentication required.".to_owned())
    }
}

async fn resolve_entity_request_user_id(
    headers: &HeaderMap,
    state: &AdapterRuntimeState,
    entity: &crate::manifest::EntityManifest,
) -> Result<Option<String>, String> {
    ensure_authenticated_entity_request(headers, state, entity).await?;

    let user = authenticated_user_from_headers(
        headers,
        state.database.pool(),
        &state.manifest.auth.session,
    )
    .await
    .map_err(|error| error.message().to_owned())?;

    Ok(user.map(|value| value.id))
}

fn attach_graphql_response(
    data: Option<Value>,
    errors: Vec<GraphqlError>,
    cookies: Vec<HeaderValue>,
) -> Response {
    let mut response = (
        StatusCode::OK,
        Json(GraphqlResponse { data, errors }),
    )
        .into_response();

    for cookie in cookies {
        response.headers_mut().append(axum::http::header::SET_COOKIE, cookie);
    }

    response
}

fn required_string_variable(variables: &Value, name: &str) -> Result<String, String> {
    variables
        .get(name)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("GraphQL variable '{name}' must be a string."))
}

fn required_object_variable(variables: &Value, name: &str) -> Result<Map<String, Value>, String> {
    variables
        .get(name)
        .and_then(Value::as_object)
        .cloned()
        .ok_or_else(|| format!("GraphQL variable '{name}' must be an object."))
}

fn required_id_variable(variables: &Value, name: &str) -> Result<String, String> {
    let value = variables
        .get(name)
        .ok_or_else(|| format!("GraphQL variable '{name}' is required."))?;

    if let Some(text) = value.as_str() {
        return Ok(text.to_owned());
    }
    if let Some(number) = value.as_i64() {
        return Ok(number.to_string());
    }
    if let Some(number) = value.as_u64() {
        return Ok(number.to_string());
    }

    Err(format!("GraphQL variable '{name}' must be a string or integer."))
}

fn extract_root_field(query: &str) -> Option<String> {
    let start = query.find('{')?;
    let rest = &query[start + 1..];
    let mut chars = rest.chars().peekable();

    while matches!(chars.peek(), Some(ch) if ch.is_whitespace()) {
        chars.next();
    }

    let mut field = String::new();
    while matches!(chars.peek(), Some(ch) if ch.is_ascii_alphanumeric() || *ch == '_') {
        field.push(chars.next()?);
    }

    if field.is_empty() {
        None
    } else {
        Some(field)
    }
}

#[cfg(test)]
mod tests {
    use super::{extract_root_field, required_id_variable};
    use serde_json::json;

    #[test]
    fn extracts_root_field_from_generated_query() {
        let query = "query GetKdfSalt($email: String!) {\n  kdfSalt(email: $email)\n}";
        assert_eq!(extract_root_field(query).as_deref(), Some("kdfSalt"));
    }

    #[test]
    fn required_id_variable_accepts_string_and_integer_values() {
        assert_eq!(required_id_variable(&json!({ "id": "abc" }), "id").unwrap(), "abc");
        assert_eq!(required_id_variable(&json!({ "id": 42 }), "id").unwrap(), "42");
    }
}