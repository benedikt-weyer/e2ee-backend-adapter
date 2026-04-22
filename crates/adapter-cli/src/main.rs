use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use e2ee_backend_adapter::{
    manifest::parse_manifest,
    schema::{diff::diff_database_against_manifest, export::export_expected_schema},
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
    },
    ValidateManifest {
        #[arg(long)]
        manifest: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Diff {
            database_url,
            manifest,
            out,
        } => {
            let manifest = load_manifest(&manifest)?;
            let diff = diff_database_against_manifest(&manifest, &database_url).await?;
            fs::write(out, diff)?;
        }
        Command::ExportExpectedSchema { manifest, out } => {
            let manifest = load_manifest(&manifest)?;
            let expected = export_expected_schema(&manifest)?;
            fs::write(out, expected)?;
        }
        Command::ValidateManifest { manifest } => {
            let _ = load_manifest(&manifest)?;
        }
    }

    Ok(())
}

fn load_manifest(path: &PathBuf) -> Result<e2ee_backend_adapter::manifest::BackendAdapterManifest> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read manifest file at {}", path.display()))?;
    parse_manifest(&content)
}

#[cfg(test)]
mod tests {
        use super::{load_manifest, Args, Command};
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
                            "version": 1,
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
                        "--manifest",
                        "/tmp/manifest.json",
                        "--out",
                        "/tmp/diff.json",
                ])
                .expect("arguments should parse");

                match args.command {
                        Command::Diff {
                                database_url,
                                manifest,
                                out,
                        } => {
                                assert_eq!(database_url, "postgres://postgres:postgres@localhost:5432/app");
                                assert_eq!(manifest, PathBuf::from("/tmp/manifest.json"));
                                assert_eq!(out, PathBuf::from("/tmp/diff.json"));
                        }
                        other => panic!("expected diff command, got {other:?}"),
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
