use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use e2ee_backend_adapter::{
    manifest::{parse_manifest, BackendAdapterManifest},
    schema::{
        diff::{diff_database_against_manifest, SchemaDiffOutputFormat},
        export::{export_expected_schema, export_typescript_client_bindings},
    },
};

#[derive(Debug, Parser)]
#[command(author, version, about = "CLI for e2ee-backend-adapter tooling")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Diff {
        #[arg(long, env = "DATABASE_URL")]
        database_url: String,
        #[arg(long, value_enum, default_value_t = DiffFormat::Sql)]
        format: DiffFormat,
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    ExportExpectedSchema {
        #[arg(long)]
        manifest: PathBuf,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        typescript_out: Option<PathBuf>,
    },
    ValidateManifest {
        #[arg(long)]
        manifest: PathBuf,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum DiffFormat {
    Json,
    Seaorm,
    Sql,
}

impl From<DiffFormat> for SchemaDiffOutputFormat {
    fn from(value: DiffFormat) -> Self {
        match value {
            DiffFormat::Json => SchemaDiffOutputFormat::Json,
            DiffFormat::Seaorm => SchemaDiffOutputFormat::Seaorm,
            DiffFormat::Sql => SchemaDiffOutputFormat::Sql,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Diff {
            database_url,
            format,
            manifest,
            out,
        } => {
            let manifest = load_manifest(&manifest)?;
            let diff = diff_database_against_manifest(&manifest, &database_url, format.into()).await?;
            fs::write(out, diff)?;
        }
        Command::ExportExpectedSchema {
            manifest,
            out,
            typescript_out,
        } => {
            let manifest = load_manifest(&manifest)?;
            let expected = export_expected_schema(&manifest)?;
            fs::write(out, expected)?;
            if let Some(typescript_out) = typescript_out {
                let typescript = export_typescript_client_bindings(&manifest)?;
                fs::write(typescript_out, typescript)?;
            }
        }
        Command::ValidateManifest { manifest } => {
            let _ = load_manifest(&manifest)?;
        }
    }

    Ok(())
}

fn load_manifest(path: &PathBuf) -> Result<BackendAdapterManifest> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read manifest file at {}", path.display()))?;
    parse_manifest(&content)
}

#[cfg(test)]
mod tests {
    use super::{load_manifest, Args, Command, DiffFormat};
        use clap::Parser;
        use std::{
                fs,
                path::PathBuf,
                time::{SystemTime, UNIX_EPOCH},
        };

        fn create_temp_manifest_file() -> PathBuf {
                let file_name = format!(
                        "e2ee-backend-adapter-test-{}.json",
                        SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .expect("time should be after epoch")
                                .as_nanos(),
                );
                let path = std::env::temp_dir().join(file_name);
                fs::write(
                        &path,
                        r#"{
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
                                    "authTables": ["users", "sessions"],
                                    "entityTables": [{ "primaryKey": "id", "tableName": "notes" }]
                                }
                            },
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
                            ]
                        }"#,
                )
                .expect("manifest file should be written");
                path
        }

        #[test]
        fn parses_diff_command_arguments() {
                let args = Args::try_parse_from([
                        "adapter-cli",
                        "diff",
                        "--database-url",
                        "postgres://postgres:postgres@localhost:5432/app",
                        "--format",
                        "seaorm",
                        "--manifest",
                        "/tmp/manifest.json",
                        "--out",
                        "/tmp/migration.rs",
                ])
                .expect("arguments should parse");

                match args.command {
                        Command::Diff {
                                database_url,
                            format,
                                manifest,
                                out,
                        } => {
                                assert_eq!(database_url, "postgres://postgres:postgres@localhost:5432/app");
                            assert_eq!(format, DiffFormat::Seaorm);
                                assert_eq!(manifest, PathBuf::from("/tmp/manifest.json"));
                            assert_eq!(out, PathBuf::from("/tmp/migration.rs"));
                        }
                        other => panic!("expected diff command, got {other:?}"),
                }
        }

        #[test]
        fn parses_export_expected_schema_typescript_output_argument() {
            let args = Args::try_parse_from([
                "adapter-cli",
                "export-expected-schema",
                "--manifest",
                "/tmp/manifest.json",
                "--out",
                "/tmp/expected-schema.json",
                "--typescript-out",
                "/tmp/generated-types.ts",
            ])
            .expect("arguments should parse");

            match args.command {
                Command::ExportExpectedSchema {
                    manifest,
                    out,
                    typescript_out,
                } => {
                    assert_eq!(manifest, PathBuf::from("/tmp/manifest.json"));
                    assert_eq!(out, PathBuf::from("/tmp/expected-schema.json"));
                    assert_eq!(typescript_out, Some(PathBuf::from("/tmp/generated-types.ts")));
                }
                other => panic!("expected export-expected-schema command, got {other:?}"),
            }
        }

        #[test]
        fn load_manifest_reads_valid_manifest_file() {
                let path = create_temp_manifest_file();

                let result = load_manifest(&path);
                let _ = fs::remove_file(&path);

                let manifest = result.expect("manifest should load");
                assert_eq!(manifest.name, "notes-service");
                assert_eq!(manifest.entities.len(), 1);
        }

        #[test]
        fn load_manifest_returns_context_for_missing_file() {
                let path = std::env::temp_dir().join("e2ee-backend-adapter-missing.json");
                let result = load_manifest(&path);

                let error = result.expect_err("missing manifest should fail");
                assert!(error.to_string().contains("Failed to read manifest file"));
        }
}
