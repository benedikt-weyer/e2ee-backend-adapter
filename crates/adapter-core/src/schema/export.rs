use anyhow::Result;
use serde::Serialize;

use crate::manifest::{BackendAdapterManifest, ExpectedSchemaManifest};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeneratedSchemaFile<'a> {
    expected_schema: &'a ExpectedSchemaManifest,
}

pub fn export_expected_schema(manifest: &BackendAdapterManifest) -> Result<String> {
    Ok(format!(
        "{}\n",
        serde_json::to_string_pretty(&GeneratedSchemaFile {
            expected_schema: &manifest.database.expected_schema,
        })?
    ))
}
