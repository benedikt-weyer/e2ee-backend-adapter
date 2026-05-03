use std::collections::BTreeMap;

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};

pub const MANIFEST_VERSION: u32 = 2;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendAdapterManifest {
    pub auth: AuthManifest,
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
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedSchemaApiManifest {
    pub rest: ExpectedSchemaRestApiManifest,
    #[serde(rename = "type")]
    pub api_type: String,
}

impl ExpectedSchemaApiManifest {
    pub fn validate(&self) -> Result<()> {
        if self.api_type != "rest" {
            bail!("Only REST generated schema APIs are supported in v1.");
        }

        self.rest.validate()?;

        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedSchemaRestApiManifest {
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
pub struct ExpectedEntityTableManifest {
    pub primary_key: String,
    pub table_name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedSchemaEntityManifest {
    pub api: ExpectedSchemaEntityApiManifest,
    pub fields: Vec<EntityFieldManifest>,
    pub id_path: String,
    pub name: String,
    pub primary_key: String,
    pub table_name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedSchemaEntityApiManifest {
    pub rest: EntityRestManifest,
    #[serde(rename = "type")]
    pub api_type: String,
}

impl ExpectedSchemaEntityApiManifest {
    pub fn validate(&self) -> Result<()> {
        if self.api_type != "rest" {
            bail!("Only REST entity APIs are supported in v1.");
        }

        self.rest.validate()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityManifest {
    pub fields: Vec<EntityFieldManifest>,
    pub id_path: String,
    pub name: String,
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
        self.rest.validate()
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
    pub entity_path: String,
    pub entity_type: String,
    pub nullable: bool,
    pub optional: bool,
    pub remote_path: String,
    pub remote_type: String,
    pub strategy_id: Option<String>,
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
            BackendAdapterManifest, DatabaseManifest, EntityFieldManifest, EntityManifest,
            EntityRestManifest, ExpectedEntityTableManifest, ExpectedSchemaApiManifest,
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
                        database: DatabaseManifest {
                                engine: "postgres".to_owned(),
                                expected_schema: ExpectedSchemaManifest {
                                        api: ExpectedSchemaApiManifest {
                                            rest: ExpectedSchemaRestApiManifest {
                                                base_url: "/api".to_owned(),
                                                default_headers: Some(BTreeMap::from([(
                                                    "accept".to_owned(),
                                                    "application/json".to_owned(),
                                                )])),
                                            },
                                            api_type: "rest".to_owned(),
                                        },
                                        auth_tables: vec!["users".to_owned(), "sessions".to_owned()],
                                entities: vec![ExpectedSchemaEntityManifest {
                                    api: ExpectedSchemaEntityApiManifest {
                                        rest: EntityRestManifest {
                                            allow_create: true,
                                            allow_delete: true,
                                            allow_get_by_id: true,
                                            allow_list: true,
                                            allow_update: true,
                                            base_path: "/entities/note".to_owned(),
                                        },
                                        api_type: "rest".to_owned(),
                                    },
                                    fields: vec![EntityFieldManifest {
                                        encrypted: true,
                                        entity_path: "content".to_owned(),
                                        entity_type: "string".to_owned(),
                                        nullable: false,
                                        optional: false,
                                        remote_path: "ciphertext".to_owned(),
                                        remote_type: "string".to_owned(),
                                        strategy_id: Some("aes-256-gcm".to_owned()),
                                    }],
                                    id_path: "id".to_owned(),
                                    name: "note".to_owned(),
                                    primary_key: "id".to_owned(),
                                    table_name: "notes".to_owned(),
                                }],
                                        entity_tables: vec![ExpectedEntityTableManifest {
                                                primary_key: "id".to_owned(),
                                                table_name: "notes".to_owned(),
                                        }],
                                },
                        },
                        entities: vec![EntityManifest {
                                fields: vec![EntityFieldManifest {
                                        encrypted: true,
                                        entity_path: "content".to_owned(),
                                        entity_type: "string".to_owned(),
                                        nullable: false,
                                        optional: false,
                                        remote_path: "ciphertext".to_owned(),
                                        remote_type: "string".to_owned(),
                                        strategy_id: Some("aes-256-gcm".to_owned()),
                                }],
                                id_path: "id".to_owned(),
                                name: "note".to_owned(),
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
                    "version": 2,
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
                assert!(json.contains("\"tableName\": \"notes\""));
        }

}
