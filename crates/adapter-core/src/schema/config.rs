use std::collections::BTreeMap;

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

use crate::manifest::{
    AuthManifest, BackendAdapterManifest, DatabaseManifest, EntityFieldManifest,
    EntityGraphqlManifest, EntityManifest, EntityRestManifest, ExpectedEntityColumnManifest,
    ExpectedEntityTableManifest, ExpectedSchemaApiManifest, ExpectedSchemaEntityApiManifest,
    ExpectedSchemaEntityManifest, ExpectedSchemaGraphqlApiManifest, ExpectedSchemaManifest,
    ExpectedSchemaRestApiManifest, RestAuthManifest, RestAuthPaths,
    SchemaAdditionalPropertiesManifest, SchemaDescriptorManifest, SchemaNodeManifest,
    SessionCookieNames, SessionManifest,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExportApiKind {
    Graphql,
    Rest,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendDbSchemaConfig {
    #[serde(default)]
    pub api: Option<BackendSchemaApiConfig>,
    pub entities: Vec<BackendDbSchemaEntityConfig>,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendSchemaApiConfig {
    #[serde(default)]
    pub graphql: Option<BackendSchemaGraphqlApiConfig>,
    #[serde(default)]
    pub rest: Option<BackendSchemaRestApiConfig>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendSchemaGraphqlApiConfig {
    #[serde(default)]
    pub authenticated: Option<bool>,
    #[serde(default)]
    pub default_headers: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub endpoint_path: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendSchemaRestApiConfig {
    #[serde(default)]
    pub authenticated: Option<bool>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub default_headers: Option<BTreeMap<String, String>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendDbSchemaEntityConfig {
    pub database: BackendDbSchemaEntityDatabaseConfig,
    pub fields: Vec<BackendDbSchemaFieldConfig>,
    #[serde(default)]
    pub filter_by_user: Option<bool>,
    pub id_path: String,
    pub name: String,
    #[serde(default)]
    pub only_allow_authed_user_filter: Option<bool>,
    pub table_name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendSchemaEntityGraphqlConfig {
    #[serde(default)]
    pub create_mutation: Option<String>,
    #[serde(default)]
    pub delete_mutation: Option<String>,
    #[serde(default)]
    pub get_by_id_query: Option<String>,
    #[serde(default)]
    pub list_query: Option<String>,
    #[serde(default)]
    pub update_mutation: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendSchemaEntityRestConfig {
    #[serde(default)]
    pub base_path: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendDbSchemaEntityDatabaseConfig {
    pub columns: Vec<BackendDbSchemaColumnConfig>,
    pub primary_key: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendDbSchemaColumnConfig {
    pub column_name: String,
    pub nullable: bool,
    pub sql_type: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendDbSchemaFieldConfig {
    pub encrypted: bool,
    pub entity_path: String,
    pub entity_schema: SchemaConfigNode,
    #[serde(default)]
    pub remote_path: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EncryptedSchemaConfig {
    #[serde(default)]
    pub api: Option<BackendSchemaApiConfig>,
    #[serde(default)]
    pub entity_api_overrides: Vec<EntityApiOverrideConfig>,
    #[serde(default)]
    pub encrypted_fields: Vec<EncryptedFieldConfig>,
    #[serde(default)]
    pub types: BTreeMap<String, SchemaConfigNode>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityApiOverrideConfig {
    #[serde(default)]
    pub entity_name: Option<String>,
    #[serde(default)]
    pub filter_by_user: Option<bool>,
    #[serde(default)]
    pub graphql: Option<BackendSchemaEntityGraphqlConfig>,
    #[serde(default)]
    pub only_allow_authed_user_filter: Option<bool>,
    #[serde(default)]
    pub rest: Option<BackendSchemaEntityRestConfig>,
    #[serde(default)]
    pub table_name: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EncryptedFieldConfig {
    pub entity_path: String,
    pub entity_schema: SchemaConfigNode,
    #[serde(default)]
    pub entity_name: Option<String>,
    #[serde(default)]
    pub remote_path: Option<String>,
    #[serde(default)]
    pub remote_schema: Option<SchemaConfigNode>,
    #[serde(default)]
    pub strategy_id: Option<String>,
    #[serde(default)]
    pub table_name: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaConfigNode {
    #[serde(default)]
    pub nullable: Option<bool>,
    #[serde(default)]
    pub optional: Option<bool>,
    #[serde(default, rename = "ref")]
    pub ref_name: Option<String>,
    #[serde(default)]
    pub schema: Option<SchemaConfigDescriptor>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum SchemaConfigDescriptor {
    Array {
        items: Box<SchemaConfigNode>,
    },
    Boolean,
    DiscriminatedUnion {
        discriminator: String,
        options: Vec<SchemaConfigNode>,
    },
    Enum {
        values: Vec<String>,
    },
    Literal {
        value: serde_json::Value,
    },
    Number {
        #[serde(default)]
        integer: Option<bool>,
    },
    Object {
        #[serde(default)]
        additional_properties: Option<SchemaConfigAdditionalProperties>,
        #[serde(default)]
        properties: Option<BTreeMap<String, SchemaConfigNode>>,
    },
    Record {
        values: Box<SchemaConfigNode>,
    },
    String,
    Union {
        options: Vec<SchemaConfigNode>,
    },
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum SchemaConfigAdditionalProperties {
    Boolean(bool),
    Schema(Box<SchemaConfigNode>),
}

#[derive(Debug, FromRow)]
struct ScaffoldTableRow {
    table_name: String,
}

#[derive(Debug, FromRow)]
struct ScaffoldColumnRow {
    column_name: String,
    data_type: String,
    is_nullable: String,
    udt_name: String,
}

#[derive(Debug, FromRow)]
struct ScaffoldPrimaryKeyRow {
    column_name: String,
}

pub fn manifest_from_db_schema_config(
    config: &BackendDbSchemaConfig,
    api: ExportApiKind,
) -> Result<BackendAdapterManifest> {
    if config.name.trim().is_empty() {
        bail!("Schema config name must not be empty.");
    }
    if config.entities.is_empty() {
        bail!("Schema config must define at least one entity.");
    }

    let entity_manifests = config
        .entities
        .iter()
        .map(build_entity_manifest)
        .collect::<Result<Vec<_>>>()?;

    let expected_entities = config
        .entities
        .iter()
        .zip(entity_manifests.iter())
        .map(|(entity, fields)| build_expected_schema_entity(entity, fields.clone(), api))
        .collect::<Vec<_>>();

    let entity_tables = config
        .entities
        .iter()
        .map(|entity| ExpectedEntityTableManifest {
            columns: entity
                .database
                .columns
                .iter()
                .map(|column| ExpectedEntityColumnManifest {
                    column_name: column.column_name.clone(),
                    nullable: column.nullable,
                    sql_type: column.sql_type.clone(),
                })
                .collect(),
            primary_key: entity.database.primary_key.clone(),
            table_name: entity.table_name.clone(),
        })
        .collect::<Vec<_>>();

    let entities = config
        .entities
        .iter()
        .zip(entity_manifests.iter())
        .map(|(entity, fields)| EntityManifest {
            filter_by_user: entity.filter_by_user.unwrap_or(false),
            fields: fields.clone(),
            graphql: derived_graphql_manifest(&entity.name, &entity.table_name, None),
            id_path: entity.id_path.clone(),
            name: entity.name.clone(),
            only_allow_authed_user_filter: entity.only_allow_authed_user_filter.unwrap_or(false),
            rest: derived_rest_manifest(&entity.name, None),
            table_name: entity.table_name.clone(),
        })
        .collect::<Vec<_>>();

    let manifest = BackendAdapterManifest {
        auth: default_auth_manifest(),
        database: DatabaseManifest {
            engine: "postgres".to_owned(),
            expected_schema: ExpectedSchemaManifest {
                api: build_expected_schema_api(config.api.as_ref(), api),
                auth_tables: vec!["users".to_owned(), "sessions".to_owned()],
                entities: expected_entities,
                entity_tables,
            },
        },
        entities,
        name: config.name.clone(),
        realtime: None,
        version: crate::manifest::MANIFEST_VERSION,
    };

    manifest.validate()?;
    Ok(manifest)
}

pub async fn scaffold_db_schema_config_from_database(
    database_url: &str,
    name: &str,
) -> Result<String> {
    if name.trim().is_empty() {
        bail!("Schema config name must not be empty.");
    }

    let pool = PgPool::connect(database_url).await?;
    let tables = sqlx::query_as::<_, ScaffoldTableRow>(
        r#"
        SELECT table_name
        FROM information_schema.tables
        WHERE table_schema = 'public'
          AND table_type = 'BASE TABLE'
          AND table_name NOT IN ('users', 'sessions')
        ORDER BY table_name
        "#,
    )
    .fetch_all(&pool)
    .await?;

    let mut entities = Vec::new();
    for table in tables {
        let columns = fetch_table_columns(&pool, &table.table_name).await?;
        if columns.is_empty() {
            continue;
        }

        let primary_keys = fetch_primary_key_columns(&pool, &table.table_name).await?;
        let primary_key = primary_keys
            .first()
            .cloned()
            .unwrap_or_else(|| "id".to_owned());

        entities.push(scaffold_entity_config(&table.table_name, &primary_key, &columns));
    }

    let config = BackendDbSchemaConfig {
        api: None,
        entities,
        name: name.to_owned(),
    };

    Ok(format!("{}\n", serde_json::to_string_pretty(&config)?))
}

async fn fetch_table_columns(pool: &PgPool, table_name: &str) -> Result<Vec<ScaffoldColumnRow>> {
    Ok(sqlx::query_as::<_, ScaffoldColumnRow>(
        r#"
        SELECT column_name, data_type, is_nullable, udt_name
        FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = $1
        ORDER BY ordinal_position
        "#,
    )
    .bind(table_name)
    .fetch_all(pool)
    .await?)
}

async fn fetch_primary_key_columns(pool: &PgPool, table_name: &str) -> Result<Vec<String>> {
    let rows = sqlx::query_as::<_, ScaffoldPrimaryKeyRow>(
        r#"
        SELECT kcu.column_name
        FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu
          ON tc.constraint_name = kcu.constraint_name
         AND tc.table_schema = kcu.table_schema
        WHERE tc.table_schema = 'public'
          AND tc.table_name = $1
          AND tc.constraint_type = 'PRIMARY KEY'
        ORDER BY kcu.ordinal_position
        "#,
    )
    .bind(table_name)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|row| row.column_name).collect())
}

fn scaffold_entity_config(
    table_name: &str,
    primary_key: &str,
    columns: &[ScaffoldColumnRow],
) -> BackendDbSchemaEntityConfig {
    let mut fields = Vec::new();
    let mut consumed = std::collections::BTreeSet::new();

    for column in columns {
        if consumed.contains(&column.column_name) {
            continue;
        }

        if let Some(prefix) = column.column_name.strip_suffix("_ciphertext") {
            let nonce_column = format!("{prefix}_nonce");
            if columns.iter().any(|candidate| candidate.column_name == nonce_column) {
                consumed.insert(column.column_name.clone());
                consumed.insert(nonce_column);
                fields.push(BackendDbSchemaFieldConfig {
                    encrypted: true,
                    entity_path: camel_case(prefix),
                    entity_schema: SchemaConfigNode {
                        nullable: Some(is_nullable(column)),
                        optional: None,
                        ref_name: None,
                        schema: Some(SchemaConfigDescriptor::Unknown),
                    },
                    remote_path: Some(format!("{}Envelope", camel_case(prefix))),
                });
                continue;
            }
        }

        consumed.insert(column.column_name.clone());
        fields.push(BackendDbSchemaFieldConfig {
            encrypted: false,
            entity_path: camel_case(&column.column_name),
            entity_schema: SchemaConfigNode {
                nullable: Some(is_nullable(column)),
                optional: None,
                ref_name: None,
                schema: Some(schema_descriptor_for_column(column)),
            },
            remote_path: None,
        });
    }

    BackendDbSchemaEntityConfig {
        database: BackendDbSchemaEntityDatabaseConfig {
            columns: columns
                .iter()
                .map(|column| BackendDbSchemaColumnConfig {
                    column_name: column.column_name.clone(),
                    nullable: is_nullable(column),
                    sql_type: column.udt_name.to_ascii_uppercase(),
                })
                .collect(),
            primary_key: primary_key.to_owned(),
        },
        fields,
        filter_by_user: None,
        id_path: camel_case(primary_key),
        name: singularize(table_name),
        only_allow_authed_user_filter: None,
        table_name: table_name.to_owned(),
    }
}

fn schema_descriptor_for_column(column: &ScaffoldColumnRow) -> SchemaConfigDescriptor {
    match column.udt_name.as_str() {
        "bool" => SchemaConfigDescriptor::Boolean,
        "int2" | "int4" | "int8" => SchemaConfigDescriptor::Number {
            integer: Some(true),
        },
        "float4" | "float8" | "numeric" => SchemaConfigDescriptor::Number {
            integer: None,
        },
        "json" | "jsonb" => SchemaConfigDescriptor::Unknown,
        _ => match column.data_type.as_str() {
            "boolean" => SchemaConfigDescriptor::Boolean,
            "smallint" | "integer" | "bigint" => SchemaConfigDescriptor::Number {
                integer: Some(true),
            },
            "numeric" | "real" | "double precision" => SchemaConfigDescriptor::Number {
                integer: None,
            },
            _ => SchemaConfigDescriptor::String,
        },
    }
}

fn is_nullable(column: &ScaffoldColumnRow) -> bool {
    column.is_nullable.eq_ignore_ascii_case("YES")
}

fn singularize(table_name: &str) -> String {
    table_name
        .strip_suffix('s')
        .filter(|value| !value.is_empty())
        .map(camel_case)
        .unwrap_or_else(|| camel_case(table_name))
}

fn build_entity_manifest(
    entity: &BackendDbSchemaEntityConfig,
) -> Result<Vec<EntityFieldManifest>> {
    if entity.name.trim().is_empty() {
        bail!("Schema config entity name must not be empty.");
    }
    if entity.table_name.trim().is_empty() {
        bail!("Schema config entity tableName must not be empty.");
    }
    if entity.fields.is_empty() {
        bail!("Schema config entity '{}' must define at least one field.", entity.name);
    }

    entity
        .fields
        .iter()
        .map(build_field_manifest)
        .collect::<Result<Vec<_>>>()
}

fn build_field_manifest(
    field: &BackendDbSchemaFieldConfig,
) -> Result<EntityFieldManifest> {
    let mut resolution_path = Vec::new();
    let entity_schema = resolve_schema_node(&field.entity_schema, &BTreeMap::new(), &mut resolution_path)?;
    let remote_schema = resolve_schema_node(&field.entity_schema, &BTreeMap::new(), &mut resolution_path)?;

    Ok(EntityFieldManifest {
        encrypted: field.encrypted,
        entity_schema: Some(entity_schema.clone()),
        entity_path: field.entity_path.clone(),
        entity_type: infer_schema_type(&entity_schema.schema).to_owned(),
        nullable: entity_schema.nullable == Some(true),
        optional: entity_schema.optional == Some(true),
        remote_path: field
            .remote_path
            .clone()
            .unwrap_or_else(|| field.entity_path.clone()),
        remote_schema: Some(remote_schema.clone()),
        remote_type: infer_schema_type(&remote_schema.schema).to_owned(),
        strategy_id: None,
    })
}

fn build_expected_schema_entity(
    entity: &BackendDbSchemaEntityConfig,
    fields: Vec<EntityFieldManifest>,
    api: ExportApiKind,
) -> ExpectedSchemaEntityManifest {
    ExpectedSchemaEntityManifest {
        api: match api {
            ExportApiKind::Graphql => ExpectedSchemaEntityApiManifest {
                graphql: Some(derived_graphql_manifest(&entity.name, &entity.table_name, None)),
                rest: None,
                api_type: "graphql".to_owned(),
            },
            ExportApiKind::Rest => ExpectedSchemaEntityApiManifest {
                graphql: None,
                rest: Some(derived_rest_manifest(&entity.name, None)),
                api_type: "rest".to_owned(),
            },
        },
        filter_by_user: entity.filter_by_user.unwrap_or(false),
        fields,
        id_path: entity.id_path.clone(),
        name: entity.name.clone(),
        only_allow_authed_user_filter: entity.only_allow_authed_user_filter.unwrap_or(false),
        primary_key: entity.database.primary_key.clone(),
        table_name: entity.table_name.clone(),
    }
}

fn update_entity_api_overrides(
    manifest: &mut BackendAdapterManifest,
    override_config: &EntityApiOverrideConfig,
    api: ExportApiKind,
) -> Result<()> {
    if override_config.entity_name.is_none() && override_config.table_name.is_none() {
        bail!("Entity API override must define entityName, tableName, or both.");
    }

    let entity_index = manifest
        .database
        .expected_schema
        .entities
        .iter()
        .position(|entity| {
            override_config
                .entity_name
                .as_ref()
                .is_none_or(|name| entity.name == *name)
                && override_config
                    .table_name
                    .as_ref()
                    .is_none_or(|table_name| entity.table_name == *table_name)
        })
        .ok_or_else(|| anyhow!("Entity API override did not match an exported entity."))?;

    let expected_entity = &mut manifest.database.expected_schema.entities[entity_index];
    let runtime_entity = manifest
        .entities
        .get_mut(entity_index)
        .ok_or_else(|| anyhow!("Entity API override matched an invalid manifest entity index."))?;

    if let Some(graphql) = override_config.graphql.as_ref() {
        runtime_entity.graphql = derived_graphql_manifest(
            &runtime_entity.name,
            &runtime_entity.table_name,
            Some(graphql),
        );

        if api == ExportApiKind::Graphql {
            expected_entity.api.graphql = Some(derived_graphql_manifest(
                &expected_entity.name,
                &expected_entity.table_name,
                Some(graphql),
            ));
        }
    }

    if let Some(rest) = override_config.rest.as_ref() {
        runtime_entity.rest = derived_rest_manifest(&runtime_entity.name, Some(rest));

        if api == ExportApiKind::Rest {
            expected_entity.api.rest = Some(derived_rest_manifest(&expected_entity.name, Some(rest)));
        }
    }

    if let Some(filter_by_user) = override_config.filter_by_user {
        runtime_entity.filter_by_user = filter_by_user;
        expected_entity.filter_by_user = filter_by_user;
    }
    if let Some(only_allow_authed_user_filter) = override_config.only_allow_authed_user_filter {
        runtime_entity.only_allow_authed_user_filter = only_allow_authed_user_filter;
        expected_entity.only_allow_authed_user_filter = only_allow_authed_user_filter;
    }

    Ok(())
}

fn update_api_overrides(
    manifest: &mut BackendAdapterManifest,
    config: &BackendSchemaApiConfig,
    api: ExportApiKind,
) -> Result<()> {
    match api {
        ExportApiKind::Graphql => {
            let Some(graphql_config) = config.graphql.as_ref() else {
                return Ok(());
            };
            let graphql = manifest
                .database
                .expected_schema
                .api
                .graphql
                .as_mut()
                .ok_or_else(|| anyhow!("GraphQL API metadata is missing from the manifest."))?;

            if let Some(authenticated) = graphql_config.authenticated {
                graphql.authenticated = authenticated;
            }
            if let Some(default_headers) = graphql_config.default_headers.as_ref() {
                graphql.default_headers = Some(default_headers.clone());
            }
            if let Some(endpoint_path) = graphql_config.endpoint_path.as_ref() {
                graphql.endpoint_path = endpoint_path.clone();
            }
        }
        ExportApiKind::Rest => {
            let Some(rest_config) = config.rest.as_ref() else {
                return Ok(());
            };
            let rest = manifest
                .database
                .expected_schema
                .api
                .rest
                .as_mut()
                .ok_or_else(|| anyhow!("REST API metadata is missing from the manifest."))?;

            if let Some(authenticated) = rest_config.authenticated {
                rest.authenticated = authenticated;
            }
            if let Some(base_url) = rest_config.base_url.as_ref() {
                rest.base_url = base_url.clone();
            }
            if let Some(default_headers) = rest_config.default_headers.as_ref() {
                rest.default_headers = Some(default_headers.clone());
            }
        }
    }

    Ok(())
}

fn update_encrypted_field_overrides(
    manifest: &mut BackendAdapterManifest,
    mapping: &EncryptedFieldConfig,
    types: &BTreeMap<String, SchemaConfigNode>,
) -> Result<()> {
    if mapping.entity_name.is_none() && mapping.table_name.is_none() {
        bail!(
            "Encrypted field config for '{}' must define entityName, tableName, or both.",
            mapping.entity_path
        );
    }

    let entity_index = manifest
        .database
        .expected_schema
        .entities
        .iter()
        .position(|entity| {
            mapping
                .entity_name
                .as_ref()
                .is_none_or(|name| entity.name == *name)
                && mapping
                    .table_name
                    .as_ref()
                    .is_none_or(|table_name| entity.table_name == *table_name)
        })
        .ok_or_else(|| {
            anyhow!(
                "Encrypted field config for '{}' did not match an exported entity.",
                mapping.entity_path
            )
        })?;

    let expected_entity = &mut manifest.database.expected_schema.entities[entity_index];
    let runtime_entity = manifest
        .entities
        .get_mut(entity_index)
        .ok_or_else(|| anyhow!("Encrypted field config matched an invalid manifest entity index."))?;

    let expected_field_index = expected_entity
        .fields
        .iter()
        .position(|field| field.entity_path == mapping.entity_path)
        .ok_or_else(|| {
            anyhow!(
                "Encrypted field config for '{}' did not match a field on entity '{}'.",
                mapping.entity_path,
                expected_entity.name
            )
        })?;
    let runtime_field_index = runtime_entity
        .fields
        .iter()
        .position(|field| field.entity_path == mapping.entity_path)
        .ok_or_else(|| {
            anyhow!(
                "Encrypted field config for '{}' did not match a runtime field on entity '{}'.",
                mapping.entity_path,
                runtime_entity.name
            )
        })?;

    let expected_field = &mut expected_entity.fields[expected_field_index];
    let runtime_field = &mut runtime_entity.fields[runtime_field_index];

    if !expected_field.encrypted || !runtime_field.encrypted {
        bail!(
            "Encrypted field config for '{}.{}' targets a field that is not marked encrypted.",
            expected_entity.name,
            mapping.entity_path
        );
    }

    let mut resolution_path = Vec::new();
    let entity_schema = resolve_schema_node(&mapping.entity_schema, types, &mut resolution_path)?;
    let remote_schema = mapping
        .remote_schema
        .as_ref()
        .map(|schema| resolve_schema_node(schema, types, &mut resolution_path))
        .transpose()?;

    for field in [expected_field, runtime_field] {
        field.entity_type = infer_schema_type(&entity_schema.schema).to_owned();
        field.entity_schema = Some(entity_schema.clone());

        if let Some(remote_path) = mapping.remote_path.as_ref() {
            field.remote_path = remote_path.clone();
        }
        if let Some(remote_schema) = remote_schema.as_ref() {
            field.remote_type = infer_schema_type(&remote_schema.schema).to_owned();
            field.remote_schema = Some(remote_schema.clone());
        }
        if let Some(strategy_id) = mapping.strategy_id.as_ref() {
            field.strategy_id = Some(strategy_id.clone());
        }
    }

    Ok(())
}

fn default_auth_manifest() -> AuthManifest {
    AuthManifest {
        mode: "password-session".to_owned(),
        rest: RestAuthManifest {
            paths: RestAuthPaths {
                get_kdf_salt: "/auth/kdf-salt".to_owned(),
                login: "/auth/login".to_owned(),
                logout: "/auth/logout".to_owned(),
                refresh: "/auth/refresh".to_owned(),
                register_begin: "/auth/register-begin".to_owned(),
                register_complete: "/auth/register-complete".to_owned(),
            },
        },
        session: SessionManifest {
            cookie_names: SessionCookieNames {
                refresh: "refresh".to_owned(),
                session: "session".to_owned(),
            },
            refresh_duration_seconds: 60 * 60 * 24 * 30,
            session_duration_seconds: 60 * 60,
        },
    }
}

fn derived_rest_manifest(
    entity_name: &str,
    override_config: Option<&BackendSchemaEntityRestConfig>,
) -> EntityRestManifest {
    EntityRestManifest {
        allow_create: true,
        allow_delete: true,
        allow_get_by_id: true,
        allow_list: true,
        allow_update: true,
        base_path: override_config
            .and_then(|rest| rest.base_path.clone())
            .unwrap_or_else(|| format!("/entities/{}", kebab_case(entity_name))),
    }
}

fn derived_graphql_manifest(
    entity_name: &str,
    table_name: &str,
    overrides: Option<&BackendSchemaEntityGraphqlConfig>,
) -> EntityGraphqlManifest {
    let pascal_name = pascal_case(entity_name);
    let camel_name = camel_case(entity_name);
    let list_name = camel_case(table_name);

    EntityGraphqlManifest {
        allow_create: true,
        allow_delete: true,
        allow_get_by_id: true,
        allow_list: true,
        allow_update: true,
        create_mutation: overrides
            .and_then(|graphql| graphql.create_mutation.clone())
            .unwrap_or_else(|| format!("create{pascal_name}")),
        delete_mutation: overrides
            .and_then(|graphql| graphql.delete_mutation.clone())
            .unwrap_or_else(|| format!("delete{pascal_name}")),
        get_by_id_query: overrides
            .and_then(|graphql| graphql.get_by_id_query.clone())
            .unwrap_or(camel_name),
        list_query: overrides
            .and_then(|graphql| graphql.list_query.clone())
            .unwrap_or(list_name),
        update_mutation: overrides
            .and_then(|graphql| graphql.update_mutation.clone())
            .unwrap_or_else(|| format!("update{pascal_name}")),
    }
}

fn pascal_case(value: &str) -> String {
    let mut output = String::new();
    let mut capitalize = true;

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            if capitalize {
                output.push(ch.to_ascii_uppercase());
                capitalize = false;
            } else {
                output.push(ch);
            }
        } else {
            capitalize = true;
        }
    }

    if output.is_empty() {
        "Entity".to_owned()
    } else {
        output
    }
}

fn camel_case(value: &str) -> String {
    let pascal = pascal_case(value);
    let mut chars = pascal.chars();
    let Some(first) = chars.next() else {
        return "entity".to_owned();
    };

    format!("{}{}", first.to_ascii_lowercase(), chars.collect::<String>())
}

fn kebab_case(value: &str) -> String {
    value
        .trim()
        .replace(|ch: char| !ch.is_ascii_alphanumeric(), "-")
        .chars()
        .fold(String::new(), |mut output, ch| {
            if ch.is_ascii_uppercase() {
                if !output.is_empty() && !output.ends_with('-') {
                    output.push('-');
                }
                output.push(ch.to_ascii_lowercase());
            } else {
                output.push(ch.to_ascii_lowercase());
            }
            output
        })
        .trim_matches('-')
        .to_owned()
}

pub fn apply_encrypted_schema_config(
    manifest: &mut BackendAdapterManifest,
    config: &EncryptedSchemaConfig,
    api: ExportApiKind,
) -> Result<()> {
    if let Some(api_config) = config.api.as_ref() {
        update_api_overrides(manifest, api_config, api)?;
    }

    for override_config in &config.entity_api_overrides {
        update_entity_api_overrides(manifest, override_config, api)?;
    }

    for mapping in &config.encrypted_fields {
        update_encrypted_field_overrides(manifest, mapping, &config.types)?;
    }

    Ok(())
}

fn build_expected_schema_api(
    overrides: Option<&BackendSchemaApiConfig>,
    api: ExportApiKind,
) -> ExpectedSchemaApiManifest {
    match api {
        ExportApiKind::Graphql => {
            let graphql = overrides.and_then(|value| value.graphql.as_ref());

            ExpectedSchemaApiManifest {
                graphql: Some(ExpectedSchemaGraphqlApiManifest {
                    authenticated: graphql.and_then(|value| value.authenticated).unwrap_or(false),
                    default_headers: Some(
                        graphql
                            .and_then(|value| value.default_headers.clone())
                            .unwrap_or_else(default_schema_api_headers),
                    ),
                    endpoint_path: graphql
                        .and_then(|value| value.endpoint_path.clone())
                        .unwrap_or_else(|| "/graphql".to_owned()),
                }),
                rest: None,
                api_type: "graphql".to_owned(),
            }
        }
        ExportApiKind::Rest => {
            let rest = overrides.and_then(|value| value.rest.as_ref());

            ExpectedSchemaApiManifest {
                graphql: None,
                rest: Some(ExpectedSchemaRestApiManifest {
                    authenticated: rest.and_then(|value| value.authenticated).unwrap_or(false),
                    base_url: rest
                        .and_then(|value| value.base_url.clone())
                        .unwrap_or_else(|| "/api".to_owned()),
                    default_headers: Some(
                        rest.and_then(|value| value.default_headers.clone())
                            .unwrap_or_else(default_schema_api_headers),
                    ),
                }),
                api_type: "rest".to_owned(),
            }
        }
    }
}

fn default_schema_api_headers() -> BTreeMap<String, String> {
    BTreeMap::from([("accept".to_owned(), "application/json".to_owned())])
}

fn resolve_schema_node(
    node: &SchemaConfigNode,
    types: &BTreeMap<String, SchemaConfigNode>,
    resolution_path: &mut Vec<String>,
) -> Result<SchemaNodeManifest> {
    match (&node.ref_name, &node.schema) {
        (Some(reference), None) => {
            if resolution_path.contains(reference) {
                let mut cycle = resolution_path.clone();
                cycle.push(reference.clone());
                bail!("Schema config contains a cyclic type reference: {}.", cycle.join(" -> "));
            }

            let referenced = types.get(reference).ok_or_else(|| {
                anyhow!("Schema config type reference '{}' is not defined.", reference)
            })?;

            resolution_path.push(reference.clone());
            let mut resolved = resolve_schema_node(referenced, types, resolution_path)?;
            resolution_path.pop();

            if node.nullable == Some(true) {
                resolved.nullable = Some(true);
            }
            if node.optional == Some(true) {
                resolved.optional = Some(true);
            }

            Ok(resolved)
        }
        (None, Some(schema)) => Ok(SchemaNodeManifest {
            nullable: node.nullable,
            optional: node.optional,
            schema: resolve_schema_descriptor(schema, types, resolution_path)?,
        }),
        (Some(_), Some(_)) => bail!("Schema config node must define either 'ref' or 'schema', not both."),
        (None, None) => bail!("Schema config node must define either 'ref' or 'schema'."),
    }
}

fn resolve_schema_descriptor(
    descriptor: &SchemaConfigDescriptor,
    types: &BTreeMap<String, SchemaConfigNode>,
    resolution_path: &mut Vec<String>,
) -> Result<SchemaDescriptorManifest> {
    Ok(match descriptor {
        SchemaConfigDescriptor::Array { items } => SchemaDescriptorManifest::Array {
            items: Box::new(resolve_schema_node(items, types, resolution_path)?),
        },
        SchemaConfigDescriptor::Boolean => SchemaDescriptorManifest::Boolean,
        SchemaConfigDescriptor::DiscriminatedUnion {
            discriminator,
            options,
        } => SchemaDescriptorManifest::DiscriminatedUnion {
            discriminator: discriminator.clone(),
            options: options
                .iter()
                .map(|option| resolve_schema_node(option, types, resolution_path))
                .collect::<Result<Vec<_>>>()?,
        },
        SchemaConfigDescriptor::Enum { values } => SchemaDescriptorManifest::Enum {
            values: values.clone(),
        },
        SchemaConfigDescriptor::Literal { value } => SchemaDescriptorManifest::Literal {
            value: value.clone(),
        },
        SchemaConfigDescriptor::Number { integer } => SchemaDescriptorManifest::Number {
            integer: *integer,
        },
        SchemaConfigDescriptor::Object {
            additional_properties,
            properties,
        } => SchemaDescriptorManifest::Object {
            additional_properties: additional_properties
                .as_ref()
                .map(|value| match value {
                    SchemaConfigAdditionalProperties::Boolean(flag) => Ok::<
                        SchemaAdditionalPropertiesManifest,
                        anyhow::Error,
                    >(SchemaAdditionalPropertiesManifest::Boolean(*flag)),
                    SchemaConfigAdditionalProperties::Schema(schema) => Ok(
                        SchemaAdditionalPropertiesManifest::Schema(Box::new(resolve_schema_node(
                            schema,
                            types,
                            resolution_path,
                        )?)),
                    ),
                })
                .transpose()?,
            properties: properties
                .as_ref()
                .map(|items| {
                    items
                        .iter()
                        .map(|(key, value)| {
                            Ok((key.clone(), resolve_schema_node(value, types, resolution_path)?))
                        })
                        .collect::<Result<BTreeMap<_, _>>>()
                })
                .transpose()?,
        },
        SchemaConfigDescriptor::Record { values } => SchemaDescriptorManifest::Record {
            values: Box::new(resolve_schema_node(values, types, resolution_path)?),
        },
        SchemaConfigDescriptor::String => SchemaDescriptorManifest::String,
        SchemaConfigDescriptor::Union { options } => SchemaDescriptorManifest::Union {
            options: options
                .iter()
                .map(|option| resolve_schema_node(option, types, resolution_path))
                .collect::<Result<Vec<_>>>()?,
        },
        SchemaConfigDescriptor::Unknown => SchemaDescriptorManifest::Unknown,
    })
}

fn infer_schema_type(schema: &SchemaDescriptorManifest) -> &'static str {
    match schema {
        SchemaDescriptorManifest::Array { .. } => "array",
        SchemaDescriptorManifest::Boolean => "boolean",
        SchemaDescriptorManifest::DiscriminatedUnion { .. } => "object",
        SchemaDescriptorManifest::Enum { .. } => "string",
        SchemaDescriptorManifest::Literal { value } => {
            if value.is_boolean() {
                "boolean"
            } else if value.is_number() {
                "number"
            } else if value.is_string() {
                "string"
            } else {
                "unknown"
            }
        }
        SchemaDescriptorManifest::Number { .. } => "number",
        SchemaDescriptorManifest::Object { .. } => "object",
        SchemaDescriptorManifest::Record { .. } => "object",
        SchemaDescriptorManifest::String => "string",
        SchemaDescriptorManifest::Union { .. } => "unknown",
        SchemaDescriptorManifest::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_encrypted_schema_config, EncryptedSchemaConfig, ExportApiKind};
    use crate::manifest::{
        AuthManifest, BackendAdapterManifest, DatabaseManifest, EntityFieldManifest,
        EntityGraphqlManifest, EntityManifest, EntityRestManifest, ExpectedEntityColumnManifest,
        ExpectedEntityTableManifest, ExpectedSchemaApiManifest,
        ExpectedSchemaEntityApiManifest, ExpectedSchemaEntityManifest,
        ExpectedSchemaManifest, ExpectedSchemaRestApiManifest, RestAuthManifest,
        RestAuthPaths, SchemaDescriptorManifest, SessionCookieNames, SessionManifest,
    };

    fn manifest() -> BackendAdapterManifest {
        BackendAdapterManifest {
            auth: AuthManifest {
                mode: "password-session".to_owned(),
                rest: RestAuthManifest {
                    paths: RestAuthPaths {
                        get_kdf_salt: "/auth/kdf-salt".to_owned(),
                        login: "/auth/login".to_owned(),
                        logout: "/auth/logout".to_owned(),
                        refresh: "/auth/refresh".to_owned(),
                        register_begin: "/auth/register-begin".to_owned(),
                        register_complete: "/auth/register-complete".to_owned(),
                    },
                },
                session: SessionManifest {
                    cookie_names: SessionCookieNames {
                        refresh: "refresh".to_owned(),
                        session: "session".to_owned(),
                    },
                    refresh_duration_seconds: 3600,
                    session_duration_seconds: 600,
                },
            },
            database: DatabaseManifest {
                engine: "postgres".to_owned(),
                expected_schema: ExpectedSchemaManifest {
                    api: ExpectedSchemaApiManifest {
                        graphql: None,
                        rest: Some(ExpectedSchemaRestApiManifest {
                            authenticated: false,
                            base_url: "/api".to_owned(),
                            default_headers: None,
                        }),
                        api_type: "rest".to_owned(),
                    },
                    auth_tables: vec!["users".to_owned(), "sessions".to_owned()],
                    entities: vec![ExpectedSchemaEntityManifest {
                        api: ExpectedSchemaEntityApiManifest {
                            graphql: None,
                            rest: Some(EntityRestManifest {
                                allow_create: true,
                                allow_delete: true,
                                allow_get_by_id: true,
                                allow_list: true,
                                allow_update: true,
                                base_path: "/integrations".to_owned(),
                            }),
                            api_type: "rest".to_owned(),
                        },
                        filter_by_user: false,
                        fields: vec![EntityFieldManifest {
                            encrypted: true,
                            entity_schema: None,
                            entity_path: "config".to_owned(),
                            entity_type: "object".to_owned(),
                            nullable: true,
                            optional: false,
                            remote_path: "configEnvelope".to_owned(),
                            remote_schema: None,
                            remote_type: "object".to_owned(),
                            strategy_id: None,
                        }],
                        id_path: "id".to_owned(),
                        name: "integration".to_owned(),
                        only_allow_authed_user_filter: false,
                        primary_key: "id".to_owned(),
                        table_name: "integrations".to_owned(),
                    }],
                    entity_tables: vec![ExpectedEntityTableManifest {
                        columns: vec![ExpectedEntityColumnManifest {
                            column_name: "config_ciphertext".to_owned(),
                            nullable: true,
                            sql_type: "BYTEA".to_owned(),
                        }],
                        primary_key: "id".to_owned(),
                        table_name: "integrations".to_owned(),
                    }],
                },
            },
            entities: vec![EntityManifest {
                filter_by_user: false,
                fields: vec![EntityFieldManifest {
                    encrypted: true,
                    entity_schema: None,
                    entity_path: "config".to_owned(),
                    entity_type: "object".to_owned(),
                    nullable: true,
                    optional: false,
                    remote_path: "configEnvelope".to_owned(),
                    remote_schema: None,
                    remote_type: "object".to_owned(),
                    strategy_id: None,
                }],
                graphql: EntityGraphqlManifest {
                    allow_create: true,
                    allow_delete: true,
                    allow_get_by_id: true,
                    allow_list: true,
                    allow_update: true,
                    create_mutation: "createIntegration".to_owned(),
                    delete_mutation: "deleteIntegration".to_owned(),
                    get_by_id_query: "integration".to_owned(),
                    list_query: "integrations".to_owned(),
                    update_mutation: "updateIntegration".to_owned(),
                },
                id_path: "id".to_owned(),
                name: "integration".to_owned(),
                only_allow_authed_user_filter: false,
                rest: EntityRestManifest {
                    allow_create: true,
                    allow_delete: true,
                    allow_get_by_id: true,
                    allow_list: true,
                    allow_update: true,
                    base_path: "/integrations".to_owned(),
                },
                table_name: "integrations".to_owned(),
            }],
            name: "notes-service".to_owned(),
            realtime: None,
            version: crate::manifest::MANIFEST_VERSION,
        }
    }

    #[test]
    fn applies_named_object_types_to_encrypted_fields() {
        let mut manifest = manifest();
                let config: EncryptedSchemaConfig = serde_json::from_str(
            r#"{
              "types": {
                "PlanderaConfig": {
                  "schema": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                      "apiUrl": { "schema": { "type": "string" } },
                      "authHash": { "nullable": true, "schema": { "type": "string" } },
                      "provider": { "schema": { "type": "enum", "values": ["manual", "oauth"] } }
                    }
                  }
                }
              },
              "encryptedFields": [
                {
                  "tableName": "integrations",
                  "entityPath": "config",
                  "entitySchema": { "ref": "PlanderaConfig" }
                }
              ]
            }"#,
        )
        .expect("config should parse");

        apply_encrypted_schema_config(&mut manifest, &config, ExportApiKind::Rest)
            .expect("config should apply");

        let field = &manifest.database.expected_schema.entities[0].fields[0];
        assert_eq!(field.entity_type, "object");
        assert!(matches!(
            field.entity_schema.as_ref().map(|node| &node.schema),
            Some(SchemaDescriptorManifest::Object { .. })
        ));
    }

    #[test]
    fn applies_entity_api_overrides_from_encrypted_config() {
        let mut manifest = manifest();
        let config: EncryptedSchemaConfig = serde_json::from_str(
            r#"{
                "entityApiOverrides": [
                    {
                        "tableName": "integrations",
                        "graphql": {
                            "createMutation": "createIntegrationRecord",
                            "deleteMutation": "deleteIntegrationRecord",
                            "getByIdQuery": "integrationRecord",
                            "listQuery": "integrationRecords",
                            "updateMutation": "updateIntegrationRecord"
                        }
                    }
                ]
            }"#,
        )
        .expect("config should parse");

        apply_encrypted_schema_config(&mut manifest, &config, ExportApiKind::Graphql)
            .expect("config should apply");

        assert_eq!(
            manifest.database.expected_schema.entities[0]
                .api
                .graphql
                .as_ref()
                .expect("graphql config should exist")
                .create_mutation,
            "createIntegrationRecord"
        );
        assert_eq!(manifest.entities[0].graphql.list_query, "integrationRecords");
    }

    #[test]
    fn applies_api_authentication_overrides_from_encrypted_config() {
        let mut manifest = manifest();
        let config: EncryptedSchemaConfig = serde_json::from_str(
            r#"{
                "api": {
                    "rest": {
                        "authenticated": true
                    }
                }
            }"#,
        )
        .expect("config should parse");

        apply_encrypted_schema_config(&mut manifest, &config, ExportApiKind::Rest)
            .expect("config should apply");

        assert!(manifest
            .database
            .expected_schema
            .api
            .rest
            .as_ref()
            .expect("rest config should exist")
            .authenticated);
    }

    #[test]
    fn applies_entity_user_filter_overrides_from_encrypted_config() {
        let mut manifest = manifest();
        let config: EncryptedSchemaConfig = serde_json::from_str(
            r#"{
                "entityApiOverrides": [
                    {
                        "tableName": "integrations",
                        "filterByUser": true,
                        "onlyAllowAuthedUserFilter": true
                    }
                ]
            }"#,
        )
        .expect("config should parse");

        apply_encrypted_schema_config(&mut manifest, &config, ExportApiKind::Rest)
            .expect("config should apply");

        assert!(manifest.entities[0].filter_by_user);
        assert!(manifest.entities[0].only_allow_authed_user_filter);
        assert!(manifest.database.expected_schema.entities[0].filter_by_user);
        assert!(manifest.database.expected_schema.entities[0].only_allow_authed_user_filter);
    }
}