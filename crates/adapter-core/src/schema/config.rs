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
pub struct BackendSchemaConfig {
    pub entities: Vec<BackendSchemaEntityConfig>,
    pub name: String,
    #[serde(default)]
    pub types: BTreeMap<String, SchemaConfigNode>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendSchemaEntityConfig {
    pub database: BackendSchemaEntityDatabaseConfig,
    pub fields: Vec<BackendSchemaFieldConfig>,
    #[serde(default)]
    pub graphql: Option<BackendSchemaEntityGraphqlConfig>,
    pub id_path: String,
    pub name: String,
    #[serde(default)]
    pub rest: Option<BackendSchemaEntityRestConfig>,
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
pub struct BackendSchemaEntityDatabaseConfig {
    pub columns: Vec<BackendSchemaColumnConfig>,
    pub primary_key: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendSchemaColumnConfig {
    pub column_name: String,
    pub nullable: bool,
    pub sql_type: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendSchemaFieldConfig {
    pub encrypted: bool,
    pub entity_path: String,
    pub entity_schema: SchemaConfigNode,
    #[serde(default)]
    pub remote_path: Option<String>,
    #[serde(default)]
    pub remote_schema: Option<SchemaConfigNode>,
    #[serde(default)]
    pub strategy_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedSchemaConfig {
    #[serde(default)]
    pub encrypted_fields: Vec<EncryptedFieldTypeMapping>,
    #[serde(default)]
    pub types: BTreeMap<String, SchemaConfigNode>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EncryptedFieldTypeMapping {
    pub entity_path: String,
    pub entity_schema: SchemaConfigNode,
    #[serde(default)]
    pub entity_name: Option<String>,
    #[serde(default)]
    pub remote_schema: Option<SchemaConfigNode>,
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

pub fn manifest_from_schema_config(
    config: &BackendSchemaConfig,
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
        .map(|entity| build_entity_manifest(entity, &config.types))
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
            fields: fields.clone(),
            graphql: derived_graphql_manifest(entity),
            id_path: entity.id_path.clone(),
            name: entity.name.clone(),
            rest: derived_rest_manifest(entity),
            table_name: entity.table_name.clone(),
        })
        .collect::<Vec<_>>();

    let manifest = BackendAdapterManifest {
        auth: default_auth_manifest(),
        database: DatabaseManifest {
            engine: "postgres".to_owned(),
            expected_schema: ExpectedSchemaManifest {
                api: match api {
                    ExportApiKind::Graphql => ExpectedSchemaApiManifest {
                        graphql: Some(ExpectedSchemaGraphqlApiManifest {
                            default_headers: Some(BTreeMap::from([(
                                "accept".to_owned(),
                                "application/json".to_owned(),
                            )])),
                            endpoint_path: "/graphql".to_owned(),
                        }),
                        rest: None,
                        api_type: "graphql".to_owned(),
                    },
                    ExportApiKind::Rest => ExpectedSchemaApiManifest {
                        graphql: None,
                        rest: Some(ExpectedSchemaRestApiManifest {
                            base_url: "/api".to_owned(),
                            default_headers: Some(BTreeMap::from([(
                                "accept".to_owned(),
                                "application/json".to_owned(),
                            )])),
                        }),
                        api_type: "rest".to_owned(),
                    },
                },
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

pub async fn scaffold_schema_config_from_database(
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

    let config = BackendSchemaConfig {
        entities,
        name: name.to_owned(),
        types: BTreeMap::new(),
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
) -> BackendSchemaEntityConfig {
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
                fields.push(BackendSchemaFieldConfig {
                    encrypted: true,
                    entity_path: camel_case(prefix),
                    entity_schema: SchemaConfigNode {
                        nullable: Some(is_nullable(column)),
                        optional: None,
                        ref_name: None,
                        schema: Some(SchemaConfigDescriptor::Unknown),
                    },
                    remote_path: Some(format!("{}Envelope", camel_case(prefix))),
                    remote_schema: None,
                    strategy_id: None,
                });
                continue;
            }
        }

        consumed.insert(column.column_name.clone());
        fields.push(BackendSchemaFieldConfig {
            encrypted: false,
            entity_path: camel_case(&column.column_name),
            entity_schema: SchemaConfigNode {
                nullable: Some(is_nullable(column)),
                optional: None,
                ref_name: None,
                schema: Some(schema_descriptor_for_column(column)),
            },
            remote_path: None,
            remote_schema: None,
            strategy_id: None,
        });
    }

    BackendSchemaEntityConfig {
        database: BackendSchemaEntityDatabaseConfig {
            columns: columns
                .iter()
                .map(|column| BackendSchemaColumnConfig {
                    column_name: column.column_name.clone(),
                    nullable: is_nullable(column),
                    sql_type: column.udt_name.to_ascii_uppercase(),
                })
                .collect(),
            primary_key: primary_key.to_owned(),
        },
        fields,
        graphql: None,
        id_path: camel_case(primary_key),
        name: singularize(table_name),
        rest: None,
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
    entity: &BackendSchemaEntityConfig,
    types: &BTreeMap<String, SchemaConfigNode>,
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
        .map(|field| build_field_manifest(field, types))
        .collect::<Result<Vec<_>>>()
}

fn build_field_manifest(
    field: &BackendSchemaFieldConfig,
    types: &BTreeMap<String, SchemaConfigNode>,
) -> Result<EntityFieldManifest> {
    let mut resolution_path = Vec::new();
    let entity_schema = resolve_schema_node(&field.entity_schema, types, &mut resolution_path)?;
    let remote_source = field.remote_schema.as_ref().unwrap_or(&field.entity_schema);
    let remote_schema = resolve_schema_node(remote_source, types, &mut resolution_path)?;

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
        strategy_id: field.strategy_id.clone(),
    })
}

fn build_expected_schema_entity(
    entity: &BackendSchemaEntityConfig,
    fields: Vec<EntityFieldManifest>,
    api: ExportApiKind,
) -> ExpectedSchemaEntityManifest {
    ExpectedSchemaEntityManifest {
        api: match api {
            ExportApiKind::Graphql => ExpectedSchemaEntityApiManifest {
                graphql: Some(derived_graphql_manifest(entity)),
                rest: None,
                api_type: "graphql".to_owned(),
            },
            ExportApiKind::Rest => ExpectedSchemaEntityApiManifest {
                graphql: None,
                rest: Some(derived_rest_manifest(entity)),
                api_type: "rest".to_owned(),
            },
        },
        fields,
        id_path: entity.id_path.clone(),
        name: entity.name.clone(),
        primary_key: entity.database.primary_key.clone(),
        table_name: entity.table_name.clone(),
    }
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

fn derived_rest_manifest(entity: &BackendSchemaEntityConfig) -> EntityRestManifest {
    EntityRestManifest {
        allow_create: true,
        allow_delete: true,
        allow_get_by_id: true,
        allow_list: true,
        allow_update: true,
        base_path: entity
            .rest
            .as_ref()
            .and_then(|rest| rest.base_path.clone())
            .unwrap_or_else(|| format!("/entities/{}", kebab_case(&entity.name))),
    }
}

fn derived_graphql_manifest(entity: &BackendSchemaEntityConfig) -> EntityGraphqlManifest {
    let overrides = entity.graphql.as_ref();
    let pascal_name = pascal_case(&entity.name);
    let camel_name = camel_case(&entity.name);
    let list_name = camel_case(&entity.table_name);

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

pub fn apply_generated_schema_config(
    manifest: &mut BackendAdapterManifest,
    config: &GeneratedSchemaConfig,
) -> Result<()> {
    for mapping in &config.encrypted_fields {
        if mapping.entity_name.is_none() && mapping.table_name.is_none() {
            bail!(
                "Encrypted field type mapping for '{}' must define entityName, tableName, or both.",
                mapping.entity_path
            );
        }

        let entity = manifest
            .database
            .expected_schema
            .entities
            .iter_mut()
            .find(|entity| {
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
                    "Encrypted field type mapping for '{}' did not match an exported entity.",
                    mapping.entity_path
                )
            })?;

        let field = entity
            .fields
            .iter_mut()
            .find(|field| field.entity_path == mapping.entity_path)
            .ok_or_else(|| {
                anyhow!(
                    "Encrypted field type mapping for '{}' did not match a field on entity '{}'.",
                    mapping.entity_path,
                    entity.name
                )
            })?;

        if !field.encrypted {
            bail!(
                "Encrypted field type mapping for '{}.{}' targets a field that is not marked encrypted.",
                entity.name,
                mapping.entity_path
            );
        }

        let mut resolution_path = Vec::new();
        let entity_schema = resolve_schema_node(&mapping.entity_schema, &config.types, &mut resolution_path)?;
        field.entity_type = infer_schema_type(&entity_schema.schema).to_owned();
        field.entity_schema = Some(entity_schema);

        if let Some(remote_schema) = &mapping.remote_schema {
            let remote_schema = resolve_schema_node(remote_schema, &config.types, &mut resolution_path)?;
            field.remote_type = infer_schema_type(&remote_schema.schema).to_owned();
            field.remote_schema = Some(remote_schema);
        }
    }

    Ok(())
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
    use super::{apply_generated_schema_config, GeneratedSchemaConfig};
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
                fields: vec![],
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
        let config: GeneratedSchemaConfig = serde_json::from_str(
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

        apply_generated_schema_config(&mut manifest, &config).expect("config should apply");

        let field = &manifest.database.expected_schema.entities[0].fields[0];
        assert_eq!(field.entity_type, "object");
        assert!(matches!(
            field.entity_schema.as_ref().map(|node| &node.schema),
            Some(SchemaDescriptorManifest::Object { .. })
        ));
    }
}