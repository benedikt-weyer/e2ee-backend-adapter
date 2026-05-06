use std::collections::BTreeMap;

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};

pub const MANIFEST_VERSION: u32 = 4;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendAdapterManifest {
    pub auth: AuthManifest,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_operations: Vec<CustomOperationManifest>,
    pub database: DatabaseManifest,
    pub entities: Vec<EntityManifest>,
    pub name: String,
    pub realtime: Option<RealtimeManifest>,
    pub version: u32,
}

impl BackendAdapterManifest {
    pub fn validate(&self) -> Result<()> {
        if self.version != MANIFEST_VERSION {
            bail!(
                "Unsupported backend adapter manifest version {}. Expected {}.",
                self.version,
                MANIFEST_VERSION,
            );
        }

        if self.name.trim().is_empty() {
            bail!("Manifest name must not be empty.");
        }

        if self.entities.is_empty() {
            bail!("Manifest must define at least one entity.");
        }

        for entity in &self.entities {
            entity.validate()?;
        }
        for operation in &self.custom_operations {
            operation.validate()?;
        }

        self.auth.validate()?;
        self.database.validate()?;

        if let Some(realtime) = &self.realtime {
            realtime.validate()?;
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthManifest {
    pub mode: String,
    pub rest: RestAuthManifest,
    pub session: SessionManifest,
}

impl AuthManifest {
    pub fn validate(&self) -> Result<()> {
        if self.mode.trim().is_empty() {
            bail!("Auth mode must not be empty.");
        }

        self.rest.validate()?;
        self.session.validate()?;
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestAuthManifest {
    pub paths: RestAuthPaths,
}

impl RestAuthManifest {
    pub fn validate(&self) -> Result<()> {
        self.paths.validate()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestAuthPaths {
    pub get_kdf_salt: String,
    pub login: String,
    pub logout: String,
    pub refresh: String,
    pub register_begin: String,
    pub register_complete: String,
}

impl RestAuthPaths {
    pub fn validate(&self) -> Result<()> {
        for path in [
            &self.get_kdf_salt,
            &self.login,
            &self.logout,
            &self.refresh,
            &self.register_begin,
            &self.register_complete,
        ] {
            if !path.starts_with('/') {
                bail!("REST auth paths must start with '/'. Invalid path: {}", path);
            }
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionManifest {
    pub cookie_names: SessionCookieNames,
    pub refresh_duration_seconds: u64,
    pub session_duration_seconds: u64,
}

impl SessionManifest {
    pub fn validate(&self) -> Result<()> {
        if self.session_duration_seconds == 0 {
            bail!("Session duration must be greater than zero.");
        }
        if self.refresh_duration_seconds == 0 {
            bail!("Refresh duration must be greater than zero.");
        }
        self.cookie_names.validate()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCookieNames {
    pub refresh: String,
    pub session: String,
}

impl SessionCookieNames {
    pub fn validate(&self) -> Result<()> {
        if self.session.trim().is_empty() || self.refresh.trim().is_empty() {
            bail!("Cookie names must not be empty.");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseManifest {
    pub engine: String,
    pub expected_schema: ExpectedSchemaManifest,
}

impl DatabaseManifest {
    pub fn validate(&self) -> Result<()> {
        if self.engine != "postgres" {
            bail!("Only postgres is supported in v1.");
        }
        self.expected_schema.validate()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedSchemaManifest {
    pub api: ExpectedSchemaApiManifest,
    pub auth_tables: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_operations: Vec<ExpectedSchemaCustomOperationManifest>,
    pub entities: Vec<ExpectedSchemaEntityManifest>,
    pub entity_tables: Vec<ExpectedEntityTableManifest>,
}

impl ExpectedSchemaManifest {
    pub fn validate(&self) -> Result<()> {
        self.api.validate()?;
        if self.auth_tables.is_empty() {
            bail!("Expected schema must define auth tables.");
        }
        if self.entities.is_empty() {
            bail!("Expected schema must define entities.");
        }
        if self.entity_tables.is_empty() {
            bail!("Expected schema must define entity tables.");
        }
        for operation in &self.custom_operations {
            operation.validate()?;
        }
        for entity in &self.entities {
            entity.validate()?;
            if entity.filter_by_user {
                let table = self
                    .entity_tables
                    .iter()
                    .find(|candidate| candidate.table_name == entity.table_name)
                    .ok_or_else(|| {
                        anyhow!(
                            "Expected schema entity '{}' is missing table metadata for '{}'.",
                            entity.name,
                            entity.table_name,
                        )
                    })?;

                if !table.columns.iter().any(|column| column.column_name == "user_id") {
                    bail!(
                        "Expected schema entity '{}' enables filterByUser but table '{}' has no 'user_id' column.",
                        entity.name,
                        entity.table_name,
                    );
                }
            }
        }
        for entity_table in &self.entity_tables {
            entity_table.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedSchemaCustomOperationManifest {
    pub api: ExpectedSchemaCustomOperationApiManifest,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_schema: Option<SchemaNodeManifest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_schema: Option<SchemaNodeManifest>,
}

impl ExpectedSchemaCustomOperationManifest {
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            bail!("Expected schema custom operation name must not be empty.");
        }

        self.api.validate()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedSchemaCustomOperationApiManifest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graphql: Option<CustomOperationGraphqlManifest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rest: Option<CustomOperationRestManifest>,
    #[serde(rename = "type")]
    pub api_type: String,
}

impl ExpectedSchemaCustomOperationApiManifest {
    pub fn validate(&self) -> Result<()> {
        match self.api_type.as_str() {
            "graphql" => self
                .graphql
                .as_ref()
                .ok_or_else(|| anyhow!("Expected schema custom operation GraphQL metadata is missing."))?
                .validate(),
            "rest" => self
                .rest
                .as_ref()
                .ok_or_else(|| anyhow!("Expected schema custom operation REST metadata is missing."))?
                .validate(),
            _ => bail!(
                "Unsupported custom operation API type '{}'.",
                self.api_type
            ),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedSchemaApiManifest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graphql: Option<ExpectedSchemaGraphqlApiManifest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rest: Option<ExpectedSchemaRestApiManifest>,
    #[serde(rename = "type")]
    pub api_type: String,
}

impl ExpectedSchemaApiManifest {
    pub fn validate(&self) -> Result<()> {
        match self.api_type.as_str() {
            "graphql" => self
                .graphql
                .as_ref()
                .ok_or_else(|| anyhow!("Expected schema GraphQL metadata is missing."))?
                .validate(),
            "rest" => self
                .rest
                .as_ref()
                .ok_or_else(|| anyhow!("Expected schema REST metadata is missing."))?
                .validate(),
            _ => bail!(
                "Unsupported generated schema API type '{}'.",
                self.api_type
            ),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedSchemaGraphqlApiManifest {
    #[serde(default)]
    pub authenticated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_headers: Option<BTreeMap<String, String>>,
    pub endpoint_path: String,
}

impl ExpectedSchemaGraphqlApiManifest {
    pub fn validate(&self) -> Result<()> {
        if !self.endpoint_path.starts_with('/') {
            bail!("Expected schema GraphQL endpoint_path must start with '/'.");
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedSchemaRestApiManifest {
    #[serde(default)]
    pub authenticated: bool,
    pub base_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_headers: Option<BTreeMap<String, String>>,
}

impl ExpectedSchemaRestApiManifest {
    pub fn validate(&self) -> Result<()> {
        if self.base_url.trim().is_empty() {
            bail!("Expected schema REST base_url must not be empty.");
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedEntityColumnManifest {
    pub column_name: String,
    pub nullable: bool,
    pub sql_type: String,
}

impl ExpectedEntityColumnManifest {
    pub fn validate(&self) -> Result<()> {
        if self.column_name.trim().is_empty() {
            bail!("Expected entity column names must not be empty.");
        }
        if self.sql_type.trim().is_empty() {
            bail!(
                "Expected entity column '{}' must define a SQL type.",
                self.column_name,
            );
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedEntityTableManifest {
    pub columns: Vec<ExpectedEntityColumnManifest>,
    pub primary_key: String,
    pub table_name: String,
}

impl ExpectedEntityTableManifest {
    pub fn validate(&self) -> Result<()> {
        if self.table_name.trim().is_empty() {
            bail!("Expected entity table names must not be empty.");
        }
        if self.primary_key.trim().is_empty() {
            bail!(
                "Expected entity table '{}' must define a primary key column.",
                self.table_name,
            );
        }
        if self.columns.is_empty() {
            bail!(
                "Expected entity table '{}' must define at least one column.",
                self.table_name,
            );
        }
        for column in &self.columns {
            column.validate()?;
        }
        if !self.columns.iter().any(|column| column.column_name == self.primary_key) {
            bail!(
                "Expected entity table '{}' must include primary key column '{}' in its column list.",
                self.table_name,
                self.primary_key,
            );
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedSchemaEntityManifest {
    pub api: ExpectedSchemaEntityApiManifest,
    #[serde(default)]
    pub filter_by_user: bool,
    pub fields: Vec<EntityFieldManifest>,
    pub id_path: String,
    pub name: String,
    #[serde(default)]
    pub only_allow_authed_user_filter: bool,
    pub primary_key: String,
    pub table_name: String,
}

impl ExpectedSchemaEntityManifest {
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            bail!("Expected schema entity name must not be empty.");
        }
        if self.id_path.trim().is_empty() {
            bail!("Expected schema entity id_path must not be empty.");
        }
        if self.fields.is_empty() {
            bail!(
                "Expected schema entity '{}' must define at least one field.",
                self.name
            );
        }
        if self.only_allow_authed_user_filter && !self.filter_by_user {
            bail!(
                "Expected schema entity '{}' cannot enable onlyAllowAuthedUserFilter without filterByUser.",
                self.name,
            );
        }
        self.api.validate()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedSchemaEntityApiManifest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graphql: Option<EntityGraphqlManifest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rest: Option<EntityRestManifest>,
    #[serde(rename = "type")]
    pub api_type: String,
}

impl ExpectedSchemaEntityApiManifest {
    pub fn validate(&self) -> Result<()> {
        match self.api_type.as_str() {
            "graphql" => self
                .graphql
                .as_ref()
                .ok_or_else(|| anyhow!("Expected schema entity GraphQL metadata is missing."))?
                .validate(),
            "rest" => self
                .rest
                .as_ref()
                .ok_or_else(|| anyhow!("Expected schema entity REST metadata is missing."))?
                .validate(),
            _ => bail!("Unsupported entity API type '{}'.", self.api_type),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityManifest {
    #[serde(default)]
    pub filter_by_user: bool,
    pub fields: Vec<EntityFieldManifest>,
    pub graphql: EntityGraphqlManifest,
    pub id_path: String,
    pub name: String,
    #[serde(default)]
    pub only_allow_authed_user_filter: bool,
    pub rest: EntityRestManifest,
    pub table_name: String,
}

impl EntityManifest {
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            bail!("Entity name must not be empty.");
        }
        if self.id_path.trim().is_empty() {
            bail!("Entity id_path must not be empty.");
        }
        if self.fields.is_empty() {
            bail!("Entity '{}' must define at least one field.", self.name);
        }
        if self.only_allow_authed_user_filter && !self.filter_by_user {
            bail!(
                "Entity '{}' cannot enable onlyAllowAuthedUserFilter without filterByUser.",
                self.name,
            );
        }
        self.rest.validate()?;
        self.graphql.validate()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomOperationManifest {
    pub graphql: CustomOperationGraphqlManifest,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_schema: Option<SchemaNodeManifest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_schema: Option<SchemaNodeManifest>,
    pub rest: CustomOperationRestManifest,
}

impl CustomOperationManifest {
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            bail!("Custom operation name must not be empty.");
        }

        self.rest.validate()?;
        self.graphql.validate()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomOperationGraphqlManifest {
    pub field_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_type_name: Option<String>,
    pub operation_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection_set: Option<String>,
}

impl CustomOperationGraphqlManifest {
    pub fn validate(&self) -> Result<()> {
        if self.field_name.trim().is_empty() {
            bail!("Custom operation GraphQL field name must not be empty.");
        }
        if !matches!(self.operation_type.as_str(), "mutation" | "query") {
            bail!(
                "Custom operation GraphQL operation type must be 'query' or 'mutation'."
            );
        }
        if self
            .input_type_name
            .as_ref()
            .is_some_and(|value| value.trim().is_empty())
        {
            bail!("Custom operation GraphQL input type name must not be empty when provided.");
        }
        if self
            .selection_set
            .as_ref()
            .is_some_and(|value| value.trim().is_empty())
        {
            bail!("Custom operation GraphQL selection set must not be empty when provided.");
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomOperationRestManifest {
    pub method: String,
    pub path: String,
}

impl CustomOperationRestManifest {
    pub fn validate(&self) -> Result<()> {
        if !self.path.starts_with('/') {
            bail!("Custom operation REST path must start with '/'.");
        }
        if !matches!(
            self.method.as_str(),
            "DELETE" | "GET" | "PATCH" | "POST" | "PUT"
        ) {
            bail!(
                "Custom operation REST method must be one of DELETE, GET, PATCH, POST, or PUT."
            );
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityGraphqlManifest {
    pub allow_create: bool,
    pub allow_delete: bool,
    pub allow_get_by_id: bool,
    pub allow_list: bool,
    pub allow_update: bool,
    pub create_mutation: String,
    pub delete_mutation: String,
    pub get_by_id_query: String,
    pub list_query: String,
    pub update_mutation: String,
}

impl EntityGraphqlManifest {
    pub fn validate(&self) -> Result<()> {
        if !(self.allow_create
            || self.allow_delete
            || self.allow_get_by_id
            || self.allow_list
            || self.allow_update)
        {
            bail!("Entity GraphQL manifest must enable at least one operation.");
        }

        for operation_name in [
            &self.create_mutation,
            &self.delete_mutation,
            &self.get_by_id_query,
            &self.list_query,
            &self.update_mutation,
        ] {
            if operation_name.trim().is_empty() {
                bail!("Entity GraphQL operation names must not be empty.");
            }
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityRestManifest {
    pub allow_create: bool,
    pub allow_delete: bool,
    pub allow_get_by_id: bool,
    pub allow_list: bool,
    pub allow_update: bool,
    pub base_path: String,
}

impl EntityRestManifest {
    pub fn validate(&self) -> Result<()> {
        if !self.base_path.starts_with('/') {
            bail!("Entity REST base_path must start with '/'.");
        }
        if !(self.allow_create
            || self.allow_delete
            || self.allow_get_by_id
            || self.allow_list
            || self.allow_update)
        {
            bail!("Entity REST manifest must enable at least one operation.");
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityFieldManifest {
    pub encrypted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_schema: Option<SchemaNodeManifest>,
    pub entity_path: String,
    pub entity_type: String,
    pub nullable: bool,
    pub optional: bool,
    pub remote_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_schema: Option<SchemaNodeManifest>,
    pub remote_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaNodeManifest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nullable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optional: Option<bool>,
    pub schema: SchemaDescriptorManifest,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum SchemaDescriptorManifest {
    Array {
        items: Box<SchemaNodeManifest>,
    },
    Boolean,
    DiscriminatedUnion {
        discriminator: String,
        options: Vec<SchemaNodeManifest>,
    },
    Enum {
        values: Vec<String>,
    },
    Literal {
        value: serde_json::Value,
    },
    Number {
        #[serde(skip_serializing_if = "Option::is_none")]
        integer: Option<bool>,
    },
    Object {
        #[serde(skip_serializing_if = "Option::is_none")]
        additional_properties: Option<SchemaAdditionalPropertiesManifest>,
        #[serde(skip_serializing_if = "Option::is_none")]
        properties: Option<std::collections::BTreeMap<String, SchemaNodeManifest>>,
    },
    Record {
        values: Box<SchemaNodeManifest>,
    },
    String,
    Union {
        options: Vec<SchemaNodeManifest>,
    },
    Unknown,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(untagged)]
pub enum SchemaAdditionalPropertiesManifest {
    Boolean(bool),
    Schema(Box<SchemaNodeManifest>),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RealtimeManifest {
    pub entities: Vec<RealtimeEntityManifest>,
    pub path: String,
    pub protocol: String,
}

impl RealtimeManifest {
    pub fn validate(&self) -> Result<()> {
        if self.protocol != "websocket" {
            bail!("Only websocket realtime is supported in v1.");
        }
        if !self.path.starts_with('/') {
            bail!("Realtime path must start with '/'.");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RealtimeEntityManifest {
    pub entity_name: String,
    pub topic: String,
}

pub fn parse_manifest(json: &str) -> Result<BackendAdapterManifest> {
    let manifest: BackendAdapterManifest = serde_json::from_str(json)?;
    manifest.validate()?;
    Ok(manifest)
}

pub fn expected_schema_to_pretty_json(manifest: &BackendAdapterManifest) -> Result<String> {
    if manifest.database.expected_schema.entity_tables.is_empty() {
        return Err(anyhow!("Manifest does not define entity table expectations."));
    }

    Ok(serde_json::to_string_pretty(&manifest.database.expected_schema)?)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

        use super::{
            expected_schema_to_pretty_json, parse_manifest, AuthManifest,
            BackendAdapterManifest, DatabaseManifest, EntityFieldManifest, EntityGraphqlManifest,
            EntityManifest, EntityRestManifest, ExpectedEntityColumnManifest,
            ExpectedEntityTableManifest, ExpectedSchemaApiManifest,
            ExpectedSchemaRestApiManifest,
            ExpectedSchemaEntityApiManifest, ExpectedSchemaEntityManifest,
            ExpectedSchemaManifest, MANIFEST_VERSION, RealtimeManifest,
                RealtimeEntityManifest, RestAuthManifest, RestAuthPaths, SessionCookieNames,
                SessionManifest,
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
                                                refresh: "refresh_cookie".to_owned(),
                                                session: "session_cookie".to_owned(),
                                        },
                                        refresh_duration_seconds: 3600,
                                        session_duration_seconds: 600,
                                },
                        },
                                custom_operations: vec![],
                        database: DatabaseManifest {
                                engine: "postgres".to_owned(),
                                expected_schema: ExpectedSchemaManifest {
                                        api: ExpectedSchemaApiManifest {
                                            graphql: None,
                                            rest: Some(ExpectedSchemaRestApiManifest {
                                                authenticated: false,
                                                base_url: "/api".to_owned(),
                                                default_headers: Some(BTreeMap::from([(
                                                    "accept".to_owned(),
                                                    "application/json".to_owned(),
                                                )])),
                                            }),
                                            api_type: "rest".to_owned(),
                                        },
                                        auth_tables: vec!["users".to_owned(), "sessions".to_owned()],
                                        custom_operations: vec![],
                                entities: vec![ExpectedSchemaEntityManifest {
                                    api: ExpectedSchemaEntityApiManifest {
                                        graphql: None,
                                        rest: Some(EntityRestManifest {
                                            allow_create: true,
                                            allow_delete: true,
                                            allow_get_by_id: true,
                                            allow_list: true,
                                            allow_update: true,
                                            base_path: "/entities/note".to_owned(),
                                        }),
                                        api_type: "rest".to_owned(),
                                    },
                                    filter_by_user: false,
                                    fields: vec![EntityFieldManifest {
                                        encrypted: true,
                                        entity_schema: None,
                                        entity_path: "content".to_owned(),
                                        entity_type: "string".to_owned(),
                                        nullable: false,
                                        optional: false,
                                        remote_path: "ciphertext".to_owned(),
                                        remote_schema: None,
                                        remote_type: "string".to_owned(),
                                        strategy_id: Some("aes-256-gcm".to_owned()),
                                    }],
                                    id_path: "id".to_owned(),
                                    name: "note".to_owned(),
                                    only_allow_authed_user_filter: false,
                                    primary_key: "id".to_owned(),
                                    table_name: "notes".to_owned(),
                                }],
                                        entity_tables: vec![ExpectedEntityTableManifest {
                                            columns: vec![
                                                ExpectedEntityColumnManifest {
                                                    column_name: "ciphertext".to_owned(),
                                                    nullable: false,
                                                    sql_type: "TEXT".to_owned(),
                                                },
                                                ExpectedEntityColumnManifest {
                                                    column_name: "id".to_owned(),
                                                    nullable: false,
                                                    sql_type: "TEXT".to_owned(),
                                                },
                                            ],
                                                primary_key: "id".to_owned(),
                                                table_name: "notes".to_owned(),
                                        }],
                                },
                        },
                        entities: vec![EntityManifest {
                            filter_by_user: false,
                                fields: vec![EntityFieldManifest {
                                        encrypted: true,
                                    entity_schema: None,
                                        entity_path: "content".to_owned(),
                                        entity_type: "string".to_owned(),
                                        nullable: false,
                                        optional: false,
                                        remote_path: "ciphertext".to_owned(),
                                    remote_schema: None,
                                        remote_type: "string".to_owned(),
                                        strategy_id: Some("aes-256-gcm".to_owned()),
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
                                only_allow_authed_user_filter: false,
                                rest: EntityRestManifest {
                                        allow_create: true,
                                        allow_delete: true,
                                        allow_get_by_id: true,
                                        allow_list: true,
                                        allow_update: true,
                                        base_path: "/entities/note".to_owned(),
                                },
                                table_name: "notes".to_owned(),
                        }],
                        name: "notes-service".to_owned(),
                        realtime: Some(RealtimeManifest {
                                entities: vec![RealtimeEntityManifest {
                                        entity_name: "note".to_owned(),
                                        topic: "notes".to_owned(),
                                }],
                                path: "/ws/realtime".to_owned(),
                                protocol: "websocket".to_owned(),
                        }),
                        version: MANIFEST_VERSION,
                }
        }

        #[test]
        fn parse_manifest_accepts_camel_case_json() {
                let json = r#"
                {
                    "version": 4,
                    "name": "notes-service",
                    "auth": {
                        "mode": "password-session",
                        "rest": {
                            "paths": {
                                "getKdfSalt": "/auth/kdf-salt",
                                "login": "/auth/login",
                                "logout": "/auth/logout",
                                "refresh": "/auth/refresh",
                                "registerBegin": "/auth/register-begin",
                                "registerComplete": "/auth/register-complete"
                            }
                        },
                        "session": {
                            "cookieNames": {
                                "refresh": "refresh_cookie",
                                "session": "session_cookie"
                            },
                            "refreshDurationSeconds": 3600,
                            "sessionDurationSeconds": 600
                        }
                    },
                    "database": {
                        "engine": "postgres",
                        "expectedSchema": {
                            "api": {
                                "rest": {
                                    "baseUrl": "/api",
                                    "defaultHeaders": {
                                        "accept": "application/json"
                                    }
                                },
                                "type": "rest"
                            },
                            "authTables": ["users", "sessions"],
                            "entities": [
                                {
                                    "api": {
                                        "rest": {
                                            "allowCreate": true,
                                            "allowDelete": true,
                                            "allowGetById": true,
                                            "allowList": true,
                                            "allowUpdate": true,
                                            "basePath": "/entities/note"
                                        },
                                        "type": "rest"
                                    },
                                    "fields": [
                                        {
                                            "encrypted": true,
                                            "entityPath": "content",
                                            "entityType": "string",
                                            "nullable": false,
                                            "optional": false,
                                            "remotePath": "ciphertext",
                                            "remoteType": "string",
                                            "strategyId": "aes-256-gcm"
                                        }
                                    ],
                                    "idPath": "id",
                                    "name": "note",
                                    "primaryKey": "id",
                                    "tableName": "notes"
                                }
                            ],
                            "entityTables": [
                                {
                                    "columns": [
                                        {
                                            "columnName": "ciphertext",
                                            "nullable": false,
                                            "sqlType": "TEXT"
                                        },
                                        {
                                            "columnName": "id",
                                            "nullable": false,
                                            "sqlType": "TEXT"
                                        }
                                    ],
                                    "primaryKey": "id",
                                    "tableName": "notes"
                                }
                            ]
                        }
                    },
                    "entities": [
                        {
                            "fields": [
                                {
                                    "encrypted": true,
                                    "entityPath": "content",
                                    "entityType": "string",
                                    "nullable": false,
                                    "optional": false,
                                    "remotePath": "ciphertext",
                                    "remoteType": "string",
                                    "strategyId": "aes-256-gcm"
                                }
                            ],
                            "graphql": {
                                "allowCreate": true,
                                "allowDelete": true,
                                "allowGetById": true,
                                "allowList": true,
                                "allowUpdate": true,
                                "createMutation": "createNote",
                                "deleteMutation": "deleteNote",
                                "getByIdQuery": "note",
                                "listQuery": "notes",
                                "updateMutation": "updateNote"
                            },
                            "idPath": "id",
                            "name": "note",
                            "rest": {
                                "allowCreate": true,
                                "allowDelete": true,
                                "allowGetById": true,
                                "allowList": true,
                                "allowUpdate": true,
                                "basePath": "/entities/note"
                            },
                            "tableName": "notes"
                        }
                    ],
                    "realtime": {
                        "entities": [{ "entityName": "note", "topic": "notes" }],
                        "path": "/ws/realtime",
                        "protocol": "websocket"
                    }
                }
                "#;

                let parsed = parse_manifest(json).expect("manifest should parse");

                assert_eq!(parsed.name, "notes-service");
                assert_eq!(parsed.entities[0].rest.base_path, "/entities/note");
                assert_eq!(parsed.auth.session.cookie_names.session, "session_cookie");
        }

        #[test]
        fn validate_rejects_unsupported_version() {
                let mut invalid = manifest();
                invalid.version = 99;

                let error = invalid.validate().expect_err("manifest should be rejected");

                assert!(error.to_string().contains("Unsupported backend adapter manifest version"));
        }

        #[test]
        fn expected_schema_export_is_pretty_json() {
                let json = expected_schema_to_pretty_json(&manifest()).expect("schema export should succeed");

                assert!(json.contains("\n"));
                assert!(json.contains("\"api\""));
                assert!(json.contains("\"baseUrl\": \"/api\""));
                assert!(json.contains("\"type\": \"rest\""));
                assert!(json.contains("\"authTables\""));
                assert!(json.contains("\"entities\""));
                assert!(json.contains("\"entityPath\": \"content\""));
                assert!(json.contains("\"columns\""));
                assert!(json.contains("\"sqlType\": \"TEXT\""));
                assert!(json.contains("\"tableName\": \"notes\""));
        }

}
