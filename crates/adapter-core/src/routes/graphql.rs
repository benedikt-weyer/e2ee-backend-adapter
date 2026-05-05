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
    auth::{get_kdf_salt, login, logout, refresh, register_begin, register_complete, AuthKeyBody, EmailBody},
    manifest::EntityManifest,
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
        _ => execute_entity_graphql(root_field, &variables, state.manifest.entities.as_slice()),
    }
}

fn execute_entity_graphql(
    root_field: String,
    variables: &Value,
    entities: &[EntityManifest],
) -> Result<GraphqlExecutionResult, String> {
    for entity in entities {
        if root_field == entity.graphql.list_query {
            return Ok(GraphqlExecutionResult {
                cookies: Vec::new(),
                data: json!({ root_field: [] }),
            });
        }

        if root_field == entity.graphql.get_by_id_query {
            return Ok(GraphqlExecutionResult {
                cookies: Vec::new(),
                data: json!({ root_field: Value::Null }),
            });
        }

        if root_field == entity.graphql.delete_mutation {
            return Ok(GraphqlExecutionResult {
                cookies: Vec::new(),
                data: json!({ root_field: true }),
            });
        }

        if root_field == entity.graphql.create_mutation {
            let mut payload = required_object_variable(variables, "input")?;
            ensure_entity_id_field(entity, &mut payload, None);

            return Ok(GraphqlExecutionResult {
                cookies: Vec::new(),
                data: json!({ root_field: Value::Object(payload) }),
            });
        }

        if root_field == entity.graphql.update_mutation {
            let mut payload = required_object_variable(variables, "input")?;
            ensure_entity_id_field(entity, &mut payload, variables.get("id").cloned());

            return Ok(GraphqlExecutionResult {
                cookies: Vec::new(),
                data: json!({ root_field: Value::Object(payload) }),
            });
        }
    }

    Err(format!("Unsupported GraphQL root field '{root_field}'."))
}

fn ensure_entity_id_field(
    entity: &EntityManifest,
    payload: &mut Map<String, Value>,
    id_override: Option<Value>,
) {
    let Some(id_field_name) = entity
        .fields
        .iter()
        .find(|field| field.entity_path == entity.id_path)
        .map(|field| field.remote_path.clone())
    else {
        return;
    };

    if payload.contains_key(&id_field_name) {
        return;
    }

    let id_value = id_override.unwrap_or_else(|| Value::String(uuid::Uuid::new_v4().to_string()));
    payload.insert(id_field_name, id_value);
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
    use super::{ensure_entity_id_field, extract_root_field};
    use crate::manifest::{EntityFieldManifest, EntityGraphqlManifest, EntityManifest, EntityRestManifest};
    use serde_json::{Map, Value};

    #[test]
    fn extracts_root_field_from_generated_query() {
        let query = "query GetKdfSalt($email: String!) {\n  kdfSalt(email: $email)\n}";
        assert_eq!(extract_root_field(query).as_deref(), Some("kdfSalt"));
    }

    #[test]
    fn ensure_entity_id_field_backfills_missing_id() {
        let entity = EntityManifest {
            fields: vec![EntityFieldManifest {
                encrypted: false,
                entity_schema: None,
                entity_path: "id".to_owned(),
                entity_type: "string".to_owned(),
                nullable: false,
                optional: false,
                remote_path: "id".to_owned(),
                remote_schema: None,
                remote_type: "string".to_owned(),
                strategy_id: None,
            }],
            graphql: EntityGraphqlManifest {
                allow_create: true,
                allow_delete: true,
                allow_get_by_id: true,
                allow_list: true,
                allow_update: true,
                create_mutation: "createNote".to_owned(),
                delete_mutation: "deleteNote".to_owned(),
                get_by_id_query: "note".to_owned(),
                list_query: "notes".to_owned(),
                update_mutation: "updateNote".to_owned(),
            },
            id_path: "id".to_owned(),
            name: "note".to_owned(),
            rest: EntityRestManifest {
                allow_create: true,
                allow_delete: true,
                allow_get_by_id: true,
                allow_list: true,
                allow_update: true,
                base_path: "/notes".to_owned(),
            },
            table_name: "notes".to_owned(),
        };
        let mut payload = Map::new();

        ensure_entity_id_field(&entity, &mut payload, Some(Value::String("abc".to_owned())));

        assert_eq!(payload.get("id"), Some(&Value::String("abc".to_owned())));
    }
}