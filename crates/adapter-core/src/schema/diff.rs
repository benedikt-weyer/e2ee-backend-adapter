use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use serde::Serialize;
use sqlx::{FromRow, PgPool};

use crate::{
    db::{postgres::auth_schema_statements, PostgresBackend},
    manifest::{
        BackendAdapterManifest, EntityFieldManifest, ExpectedEntityTableManifest,
        ExpectedSchemaEntityManifest,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchemaDiffOutputFormat {
    Json,
    Seaorm,
    Sql,
}

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
    format: SchemaDiffOutputFormat,
) -> Result<String> {
    let database = PostgresBackend::connect(database_url).await?;
    let report = build_schema_diff_report(manifest, database.pool()).await?;
    render_schema_diff_report(manifest, &report, format)
}

fn render_schema_diff_report(
    manifest: &BackendAdapterManifest,
    report: &SchemaDiffReport,
    format: SchemaDiffOutputFormat,
) -> Result<String> {
    match format {
        SchemaDiffOutputFormat::Json => {
            Ok(format!("{}\n", serde_json::to_string_pretty(report)?))
        }
        SchemaDiffOutputFormat::Sql => Ok(render_sql_reconciliation(manifest, report)),
        SchemaDiffOutputFormat::Seaorm => Ok(render_seaorm_migration(manifest, report)),
    }
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

fn render_sql_reconciliation(
    manifest: &BackendAdapterManifest,
    report: &SchemaDiffReport,
) -> String {
    let statements = sql_reconciliation_statements(manifest, report);
    if statements.is_empty() {
        return "-- Database schema already matches the manifest.\n".to_owned();
    }

    let mut output = String::from("-- Generated by e2ee-backend-adapter diff --format sql\nBEGIN;\n\n");
    output.push_str(&statements.join("\n\n"));
    output.push_str("\n\nCOMMIT;\n");
    output
}

fn render_seaorm_migration(
    manifest: &BackendAdapterManifest,
    report: &SchemaDiffReport,
) -> String {
    let sql = render_sql_reconciliation(manifest, report);

    format!(
        "use sea_orm_migration::prelude::*;\n\n#[derive(DeriveMigrationName)]\npub struct Migration;\n\n#[async_trait::async_trait]\nimpl MigrationTrait for Migration {{\n    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {{\n        manager\n            .get_connection()\n            .execute_unprepared(r#\"{sql}\"#)\n            .await?;\n        Ok(())\n    }}\n\n    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {{\n        Ok(())\n    }}\n}}\n"
    )
}

fn sql_reconciliation_statements(
    manifest: &BackendAdapterManifest,
    report: &SchemaDiffReport,
) -> Vec<String> {
    let mut statements = Vec::new();

    let missing_tables = report
        .missing_tables
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();

    if missing_tables.contains("users") {
        statements.push(normalize_sql_statement(auth_schema_statements()[0]));
    }

    if missing_tables.contains("sessions") {
        statements.push(normalize_sql_statement(auth_schema_statements()[1]));
        statements.push(normalize_sql_statement(auth_schema_statements()[2]));
        statements.push(normalize_sql_statement(auth_schema_statements()[3]));
    }

    for expected_table in &manifest.database.expected_schema.entity_tables {
        if missing_tables.contains(expected_table.table_name.as_str()) {
            statements.push(render_missing_entity_table_sql(manifest, expected_table));
        }
    }

    for mismatch in &report.entity_primary_key_mismatches {
        statements.push(render_primary_key_fix_sql(
            &mismatch.table_name,
            &mismatch.expected,
        ));
    }

    for table_name in &report.unexpected_tables {
        statements.push(format!(
            "-- Unexpected table present in the live schema.\nDROP TABLE IF EXISTS {} CASCADE;",
            quote_ident(table_name),
        ));
    }

    statements
}

fn render_missing_entity_table_sql(
    manifest: &BackendAdapterManifest,
    expected_table: &ExpectedEntityTableManifest,
) -> String {
    let column_type = manifest
        .database
        .expected_schema
        .entities
        .iter()
        .find(|entity| entity.table_name == expected_table.table_name)
        .map(|entity| primary_key_column_type(entity, &expected_table.primary_key))
        .unwrap_or("TEXT");

    format!(
        "-- The adapter currently validates entity table presence and primary keys only.\nCREATE TABLE IF NOT EXISTS {} (\n    {} {} PRIMARY KEY\n);",
        quote_ident(&expected_table.table_name),
        quote_ident(&expected_table.primary_key),
        column_type,
    )
}

fn render_primary_key_fix_sql(table_name: &str, expected_columns: &[String]) -> String {
    let expected_columns = expected_columns
        .iter()
        .map(|column| quote_ident(column))
        .collect::<Vec<_>>()
        .join(", ");
    let escaped_table_name = table_name.replace('\'', "''");

    format!(
        "DO $$\nDECLARE existing_primary_key text;\nBEGIN\n    SELECT constraint_name\n    INTO existing_primary_key\n    FROM information_schema.table_constraints\n    WHERE table_schema = 'public'\n      AND table_name = '{escaped_table_name}'\n      AND constraint_type = 'PRIMARY KEY'\n    LIMIT 1;\n\n    IF existing_primary_key IS NOT NULL THEN\n        EXECUTE format('ALTER TABLE %I.%I DROP CONSTRAINT %I', 'public', '{escaped_table_name}', existing_primary_key);\n    END IF;\nEND $$;\nALTER TABLE {} ADD PRIMARY KEY ({});",
        quote_ident(table_name),
        expected_columns,
    )
}

fn primary_key_column_type(entity: &ExpectedSchemaEntityManifest, primary_key: &str) -> &'static str {
    entity
        .fields
        .iter()
        .find(|field| field.remote_path == primary_key || field.entity_path == entity.id_path)
        .map(field_sql_type)
        .unwrap_or("TEXT")
}

fn field_sql_type(field: &EntityFieldManifest) -> &'static str {
    match field.remote_type.as_str() {
        "boolean" => "BOOLEAN",
        "number" => "DOUBLE PRECISION",
        "array" | "json" | "object" | "unknown" => "JSONB",
        _ => "TEXT",
    }
}

fn normalize_sql_statement(statement: &str) -> String {
    let trimmed = statement.trim();
    if trimmed.ends_with(';') {
        trimmed.to_owned()
    } else {
        format!("{trimmed};")
    }
}

fn quote_ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
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
    use super::{
        compare_schema, render_schema_diff_report, ActualTableSchema, SchemaDiffOutputFormat,
    };
    use crate::manifest::{
        AuthManifest, BackendAdapterManifest, DatabaseManifest, EntityFieldManifest,
        EntityManifest, EntityRestManifest, ExpectedEntityTableManifest,
        ExpectedSchemaApiManifest, ExpectedSchemaEntityApiManifest,
        ExpectedSchemaRestApiManifest,
        ExpectedSchemaEntityManifest, ExpectedSchemaManifest, RestAuthManifest, RestAuthPaths,
        SessionCookieNames, SessionManifest,
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
                    api: ExpectedSchemaApiManifest {
                        rest: ExpectedSchemaRestApiManifest {
                            base_url: "/api".to_owned(),
                            default_headers: None,
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

    #[test]
    fn renders_sql_reconciliation_for_missing_table_and_primary_key_changes() {
        let manifest = manifest();
        let report = compare_schema(
            &manifest,
            vec![
                ActualTableSchema {
                    primary_key: vec!["id".to_owned()],
                    table_name: "users".to_owned(),
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

        let output = render_schema_diff_report(&manifest, &report, SchemaDiffOutputFormat::Sql)
            .expect("sql render should succeed");

        assert!(output.contains("BEGIN;"));
        assert!(output.contains("CREATE TABLE IF NOT EXISTS sessions"));
        assert!(output.contains("ALTER TABLE \"notes\" ADD PRIMARY KEY (\"id\")"));
        assert!(output.contains("DROP TABLE IF EXISTS \"audit_entries\" CASCADE;"));
        assert!(output.contains("COMMIT;"));
    }

    #[test]
    fn renders_seaorm_migration_wrapper() {
        let manifest = manifest();
        let report = compare_schema(&manifest, vec![]);

        let output = render_schema_diff_report(&manifest, &report, SchemaDiffOutputFormat::Seaorm)
            .expect("seaorm render should succeed");

        assert!(output.contains("use sea_orm_migration::prelude::*;"));
        assert!(output.contains("execute_unprepared"));
        assert!(output.contains("CREATE TABLE IF NOT EXISTS users"));
    }
}
