use anyhow::Result;

use crate::manifest::BackendAdapterManifest;

pub async fn diff_database_against_manifest(
    manifest: &BackendAdapterManifest,
) -> Result<String> {
    let expected = manifest
        .database
        .expected_schema
        .entity_tables
        .iter()
        .map(|table| format!("- expected table {} with primary key {}", table.table_name, table.primary_key))
        .collect::<Vec<_>>()
        .join("\n");

    Ok(format!(
        "Database diff generation is scaffolded. The adapter currently expects:\n{}",
        expected,
    ))
}
