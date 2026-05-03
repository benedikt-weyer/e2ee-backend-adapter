use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::manifest::ExpectedSchemaManifest;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendAdapterSchemaConfig {
    pub expected_schema: ExpectedSchemaManifest,
}

impl BackendAdapterSchemaConfig {
    pub fn validate(&self) -> Result<()> {
        self.expected_schema.validate()
    }
}

pub fn parse_schema_config(json: &str) -> Result<BackendAdapterSchemaConfig> {
    let config: BackendAdapterSchemaConfig = serde_json::from_str(json)?;
    config.validate()?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::{parse_schema_config, BackendAdapterSchemaConfig};

    #[test]
    fn parses_schema_config_with_encrypted_field_metadata() {
        let config = parse_schema_config(
            r#"
            {
                "expectedSchema": {
                    "authTables": ["users", "sessions"],
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
            }
            "#,
        )
        .expect("schema config should parse");

        assert_eq!(config.expected_schema.entities.len(), 1);
        assert!(config.expected_schema.entities[0].fields[0].encrypted);
    }

    #[test]
    fn rejects_invalid_schema_config() {
        let error = parse_schema_config(r#"{"expectedSchema":{"authTables":[],"entities":[],"entityTables":[]}}"#)
            .expect_err("invalid config should fail");

        assert!(error.to_string().contains("Expected schema must define auth tables"));
    }

    #[test]
    fn serializes_schema_config_in_camel_case() {
        let config = parse_schema_config(
            r#"
            {
                "expectedSchema": {
                    "authTables": ["users"],
                    "entities": [
                        {
                            "fields": [
                                {
                                    "encrypted": false,
                                    "entityPath": "id",
                                    "entityType": "string",
                                    "nullable": false,
                                    "optional": false,
                                    "remotePath": "id",
                                    "remoteType": "string"
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
            }
            "#,
        )
        .expect("schema config should parse");

        let json = serde_json::to_string(&config).expect("schema config should serialize");
        let round_trip: BackendAdapterSchemaConfig = serde_json::from_str(&json)
            .expect("schema config should deserialize");

        assert_eq!(round_trip.expected_schema.entities[0].name, "note");
        assert!(json.contains("expectedSchema"));
    }
}