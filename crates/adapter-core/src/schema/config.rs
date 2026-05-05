use std::collections::BTreeMap;

use anyhow::{anyhow, bail, Result};
use serde::Deserialize;

use crate::manifest::{
    BackendAdapterManifest, SchemaAdditionalPropertiesManifest, SchemaDescriptorManifest,
    SchemaNodeManifest,
};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedSchemaConfig {
    #[serde(default)]
    pub encrypted_fields: Vec<EncryptedFieldTypeMapping>,
    #[serde(default)]
    pub types: BTreeMap<String, SchemaConfigNode>,
}

#[derive(Clone, Debug, Deserialize)]
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

#[derive(Clone, Debug, Deserialize)]
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

#[derive(Clone, Debug, Deserialize)]
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

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum SchemaConfigAdditionalProperties {
    Boolean(bool),
    Schema(Box<SchemaConfigNode>),
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