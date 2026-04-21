use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use serde::Serialize;
use sqlx::{FromRow, PgPool};

use crate::{db::PostgresBackend, manifest::BackendAdapterManifest};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ActualTableSchema {
    primary_key: Vec<String>,
    table_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct PrimaryKeyMismatch {
    actual: Vec<String>,
    expected: Vec<String>,
    table_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct SchemaDiffReport {
    actual_tables: Vec<ActualTableSchema>,
    entity_primary_key_mismatches: Vec<PrimaryKeyMismatch>,
    expected_table_names: Vec<String>,
    matches: bool,
    missing_tables: Vec<String>,
    unexpected_tables: Vec<String>,
}

#[derive(Debug, FromRow)]
struct TableNameRow {
    table_name: String,
}

#[derive(Debug, FromRow)]
struct PrimaryKeyRow {
    column_name: String,
}

pub async fn diff_database_against_manifest(
    manifest: &BackendAdapterManifest,
    database_url: &str,
) -> Result<String> {
    let database = PostgresBackend::connect(database_url).await?;
    let report = build_schema_diff_report(manifest, database.pool()).await?;
    Ok(format!("{}\n", serde_json::to_string_pretty(&report)?))
}

async fn build_schema_diff_report(
    manifest: &BackendAdapterManifest,
    pool: &PgPool,
) -> Result<SchemaDiffReport> {
    let table_names = sqlx::query_as::<_, TableNameRow>(
        r#"
        SELECT table_name
        FROM information_schema.tables
        WHERE table_schema = 'public' AND table_type = 'BASE TABLE'
        ORDER BY table_name
        "#,
    )
    .fetch_all(pool)
    .await?;

    let mut actual_by_name = BTreeMap::new();
    for row in table_names {
        let primary_key = fetch_primary_key_columns(pool, &row.table_name).await?;
        actual_by_name.insert(
            row.table_name.clone(),
            ActualTableSchema {
                primary_key,
                table_name: row.table_name,
            },
        );
    }

    Ok(compare_schema(manifest, actual_by_name.into_values().collect()))
}

fn compare_schema(
    manifest: &BackendAdapterManifest,
    actual_tables: Vec<ActualTableSchema>,
) -> SchemaDiffReport {
    let expected_table_names = manifest
        .database
        .expected_schema
        .auth_tables
        .iter()
        .cloned()
        .chain(
            manifest
                .database
                .expected_schema
                .entity_tables
                .iter()
                .map(|value| value.table_name.clone()),
        )
        .collect::<BTreeSet<_>>();
    let actual_table_names = actual_tables
        .iter()
        .map(|value| value.table_name.clone())
        .collect::<BTreeSet<_>>();
    let actual_by_name = actual_tables
        .iter()
        .map(|value| (value.table_name.clone(), value))
        .collect::<BTreeMap<_, _>>();

    let missing_tables = expected_table_names
        .difference(&actual_table_names)
        .cloned()
        .collect::<Vec<_>>();
    let unexpected_tables = actual_table_names
        .difference(&expected_table_names)
        .cloned()
        .collect::<Vec<_>>();
    let entity_primary_key_mismatches = manifest
        .database
        .expected_schema
        .entity_tables
        .iter()
        .filter_map(|expected| {
            let actual = actual_by_name.get(&expected.table_name)?;
            let expected_primary_key = vec![expected.primary_key.clone()];
            (actual.primary_key != expected_primary_key).then(|| PrimaryKeyMismatch {
                actual: actual.primary_key.clone(),
                expected: expected_primary_key,
                table_name: expected.table_name.clone(),
            })
        })
        .collect::<Vec<_>>();

    SchemaDiffReport {
        actual_tables,
        entity_primary_key_mismatches: entity_primary_key_mismatches.clone(),
        expected_table_names: expected_table_names.into_iter().collect(),
        matches: missing_tables.is_empty()
            && unexpected_tables.is_empty()
            && entity_primary_key_mismatches.is_empty(),
        missing_tables,
        unexpected_tables,
    }
}

async fn fetch_primary_key_columns(pool: &PgPool, table_name: &str) -> Result<Vec<String>> {
    let rows = sqlx::query_as::<_, PrimaryKeyRow>(
        r#"
        SELECT attribute.attname AS column_name
        FROM pg_index index_info
        JOIN pg_class class_info ON class_info.oid = index_info.indrelid
        JOIN pg_namespace namespace_info ON namespace_info.oid = class_info.relnamespace
        JOIN pg_attribute attribute
            ON attribute.attrelid = class_info.oid
           AND attribute.attnum = ANY(index_info.indkey)
        WHERE namespace_info.nspname = 'public'
          AND class_info.relname = $1
          AND index_info.indisprimary
        ORDER BY array_position(index_info.indkey, attribute.attnum)
        "#,
    )
    .bind(table_name)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|value| value.column_name).collect())
}

#[cfg(test)]
mod tests {
    use super::{compare_schema, ActualTableSchema};
    use crate::manifest::{
        AuthManifest, BackendAdapterManifest, DatabaseManifest, EntityFieldManifest,
        EntityManifest, EntityRestManifest, ExpectedEntityTableManifest, ExpectedSchemaManifest,
        RestAuthManifest, RestAuthPaths, SessionCookieNames, SessionManifest,
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
                    refresh_duration_seconds: 60,
                    session_duration_seconds: 60,
                },
            },
            database: DatabaseManifest {
                engine: "postgres".to_owned(),
                expected_schema: ExpectedSchemaManifest {
                    auth_tables: vec!["users".to_owned(), "sessions".to_owned()],
                    entity_tables: vec![ExpectedEntityTableManifest {
                        primary_key: "id".to_owned(),
                        table_name: "notes".to_owned(),
                    }],
                },
            },
            entities: vec![EntityManifest {
                fields: vec![EntityFieldManifest {
                    encrypted: false,
                    entity_path: "id".to_owned(),
                    entity_type: "string".to_owned(),
                    nullable: false,
                    optional: false,
                    remote_path: "id".to_owned(),
                    remote_type: "string".to_owned(),
                    strategy_id: None,
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
            name: "test".to_owned(),
            realtime: None,
            version: 1,
        }
    }

    #[test]
    fn reports_missing_and_unexpected_tables() {
        let report = compare_schema(
            &manifest(),
            vec![ActualTableSchema {
                primary_key: vec!["id".to_owned()],
                table_name: "users".to_owned(),
            }],
        );

        assert_eq!(report.missing_tables, vec!["notes".to_owned(), "sessions".to_owned()]);
        assert!(report.unexpected_tables.is_empty());
        assert!(!report.matches);
    }

    #[test]
    fn reports_primary_key_mismatches() {
        let report = compare_schema(
            &manifest(),
            vec![
                ActualTableSchema {
                    primary_key: vec!["id".to_owned()],
                    table_name: "users".to_owned(),
                },
                ActualTableSchema {
                    primary_key: vec!["id".to_owned()],
                    table_name: "sessions".to_owned(),
                },
                ActualTableSchema {
                    primary_key: vec!["note_id".to_owned()],
                    table_name: "notes".to_owned(),
                },
                ActualTableSchema {
                    primary_key: vec!["id".to_owned()],
                    table_name: "audit_entries".to_owned(),
                },
            ],
        );

        assert_eq!(report.unexpected_tables, vec!["audit_entries".to_owned()]);
        assert_eq!(report.entity_primary_key_mismatches.len(), 1);
        assert_eq!(
            report.entity_primary_key_mismatches[0].table_name,
            "notes".to_owned(),
        );
        assert!(!report.matches);
    }
}
