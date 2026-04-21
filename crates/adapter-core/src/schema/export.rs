use anyhow::Result;

use crate::manifest::{expected_schema_to_pretty_json, BackendAdapterManifest};

pub fn export_expected_schema(manifest: &BackendAdapterManifest) -> Result<String> {
    expected_schema_to_pretty_json(manifest)
}
