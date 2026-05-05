use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::{Map, Number, Value};
use sqlx::{postgres::PgRow, types::Json, PgPool, Postgres, QueryBuilder, Row};

use crate::manifest::{
    BackendAdapterManifest, EntityManifest, ExpectedEntityColumnManifest, ExpectedEntityTableManifest,
};

const LEGACY_ENCRYPTION_ALGORITHM: &str = "aes-256-gcm";

pub async fn create_entity_record(
    pool: &PgPool,
    manifest: &BackendAdapterManifest,
    entity: &EntityManifest,
    input: &Map<String, Value>,
) -> Result<Value> {
    let mapping = EntitySqlMapping::new(manifest, entity)?;
    let assignments = mapping.collect_assignments(input, true)?;
    if assignments.is_empty() {
        bail!("Create mutation for entity '{}' does not include any writable fields.", entity.name);
    }

    let mut builder = QueryBuilder::<Postgres>::new("INSERT INTO ");
    builder.push(mapping.quoted_table_name());
    builder.push(" (");
    {
        let mut separated = builder.separated(", ");
        for assignment in &assignments {
            separated.push(mapping.quoted_column_name(&assignment.column.column_name));
        }
    }
    builder.push(") VALUES (");
    {
        let mut separated = builder.separated(", ");
        for assignment in &assignments {
            push_assignment_value(&mut separated, assignment);
        }
    }
    builder.push(") RETURNING ");
    builder.push(mapping.cast_identifier_as_text(&mapping.primary_key.column_name, &mapping.primary_key.sql_type));
    builder.push(" AS id");

    let row = builder
        .build()
        .fetch_one(pool)
        .await
        .with_context(|| format!("Failed to insert entity '{}' into table '{}'.", entity.name, entity.table_name))?;
    let id = row
        .try_get::<String, _>("id")
        .context("Inserted row did not return a primary key.")?;

    mapping.get_by_id(pool, &id).await?.ok_or_else(|| {
        anyhow!(
            "Entity '{}' was inserted but could not be reloaded from '{}'.",
            entity.name,
            entity.table_name
        )
    })
}

pub async fn delete_entity_record(
    pool: &PgPool,
    manifest: &BackendAdapterManifest,
    entity: &EntityManifest,
    id: &str,
) -> Result<bool> {
    let mapping = EntitySqlMapping::new(manifest, entity)?;
    let sql = format!(
        "DELETE FROM {} WHERE {} = CAST($1 AS {})",
        mapping.quoted_table_name(),
        mapping.quoted_column_name(&mapping.primary_key.column_name),
        mapping.primary_key.sql_type
    );

    let result = sqlx::query(&sql)
        .bind(id)
        .execute(pool)
        .await
        .with_context(|| format!("Failed to delete entity '{}' with id '{}'.", entity.name, id))?;

    Ok(result.rows_affected() > 0)
}

pub async fn get_entity_record_by_id(
    pool: &PgPool,
    manifest: &BackendAdapterManifest,
    entity: &EntityManifest,
    id: &str,
) -> Result<Option<Value>> {
    EntitySqlMapping::new(manifest, entity)?.get_by_id(pool, id).await
}

pub async fn list_entity_records(
    pool: &PgPool,
    manifest: &BackendAdapterManifest,
    entity: &EntityManifest,
) -> Result<Vec<Value>> {
    EntitySqlMapping::new(manifest, entity)?.list(pool).await
}

pub async fn update_entity_record(
    pool: &PgPool,
    manifest: &BackendAdapterManifest,
    entity: &EntityManifest,
    id: &str,
    input: &Map<String, Value>,
) -> Result<Option<Value>> {
    let mapping = EntitySqlMapping::new(manifest, entity)?;
    let assignments = mapping.collect_assignments(input, false)?;
    if assignments.is_empty() {
        return mapping.get_by_id(pool, id).await;
    }

    let mut builder = QueryBuilder::<Postgres>::new("UPDATE ");
    builder.push(mapping.quoted_table_name());
    builder.push(" SET ");
    {
        let mut separated = builder.separated(", ");
        for assignment in &assignments {
            separated.push(mapping.quoted_column_name(&assignment.column.column_name));
            separated.push(" = ");
            push_assignment_value(&mut separated, assignment);
        }
    }
    builder.push(" WHERE ");
    builder.push(mapping.quoted_column_name(&mapping.primary_key.column_name));
    builder.push(" = CAST(");
    builder.push_bind(id);
    builder.push(" AS ");
    builder.push(mapping.primary_key.sql_type.as_str());
    builder.push(") RETURNING ");
    builder.push(mapping.cast_identifier_as_text(&mapping.primary_key.column_name, &mapping.primary_key.sql_type));
    builder.push(" AS id");

    let updated_id = builder
        .build()
        .fetch_optional(pool)
        .await
        .with_context(|| format!("Failed to update entity '{}' with id '{}'.", entity.name, id))?
        .map(|row| row.try_get::<String, _>("id"))
        .transpose()
        .context("Updated row did not return a primary key.")?;

    let Some(updated_id) = updated_id else {
        return Ok(None);
    };

    mapping.get_by_id(pool, &updated_id).await
}

struct EntitySqlMapping<'a> {
    entity: &'a EntityManifest,
    fields: Vec<FieldColumnMapping<'a>>,
    primary_key: &'a ExpectedEntityColumnManifest,
    table: &'a ExpectedEntityTableManifest,
}

impl<'a> EntitySqlMapping<'a> {
    fn new(manifest: &'a BackendAdapterManifest, entity: &'a EntityManifest) -> Result<Self> {
        let table = manifest
            .database
            .expected_schema
            .entity_tables
            .iter()
            .find(|candidate| candidate.table_name == entity.table_name)
            .ok_or_else(|| anyhow!("Expected schema table metadata is missing for '{}'.", entity.table_name))?;
        let primary_key = table
            .columns
            .iter()
            .find(|candidate| candidate.column_name == table.primary_key)
            .ok_or_else(|| anyhow!("Primary key column '{}' is missing from '{}'.", table.primary_key, entity.table_name))?;

        let mut fields = Vec::with_capacity(entity.fields.len());
        for field in &entity.fields {
            if field.encrypted {
                let base_name = field_base_column_name(&field.entity_path);
                let ciphertext = table
                    .columns
                    .iter()
                    .find(|candidate| candidate.column_name == format!("{base_name}_ciphertext"))
                    .ok_or_else(|| anyhow!("Encrypted field '{}' is missing its ciphertext column in '{}'.", field.entity_path, entity.table_name))?;
                let nonce = table
                    .columns
                    .iter()
                    .find(|candidate| candidate.column_name == format!("{base_name}_nonce"))
                    .ok_or_else(|| anyhow!("Encrypted field '{}' is missing its nonce column in '{}'.", field.entity_path, entity.table_name))?;

                fields.push(FieldColumnMapping::Encrypted {
                    algorithm: field.strategy_id.as_deref().unwrap_or(LEGACY_ENCRYPTION_ALGORITHM),
                    ciphertext,
                    nonce,
                    remote_path: field.remote_path.as_str(),
                });
                continue;
            }

            let column_name = field_base_column_name(&field.entity_path);
            let column = table
                .columns
                .iter()
                .find(|candidate| candidate.column_name == column_name)
                .ok_or_else(|| anyhow!("Field '{}' could not be mapped to a column in '{}'.", field.entity_path, entity.table_name))?;

            fields.push(FieldColumnMapping::Scalar {
                column,
                remote_path: field.remote_path.as_str(),
            });
        }

        Ok(Self {
            entity,
            fields,
            primary_key,
            table,
        })
    }

    fn cast_identifier_as_text(&self, column_name: &str, sql_type: &str) -> String {
        let identifier = self.quoted_column_name(column_name);
        match sql_kind(sql_type) {
            SqlKind::Text => identifier,
            _ => format!("CAST({identifier} AS TEXT)"),
        }
    }

    fn collect_assignments(
        &self,
        input: &Map<String, Value>,
        include_primary_key: bool,
    ) -> Result<Vec<ColumnAssignment<'a>>> {
        let mut assignments = Vec::new();

        for field in &self.fields {
            match field {
                FieldColumnMapping::Scalar { column, remote_path } => {
                    if !include_primary_key && column.column_name == self.primary_key.column_name {
                        continue;
                    }

                    let Some(value) = get_object_path(input, remote_path) else {
                        continue;
                    };

                    assignments.push(ColumnAssignment {
                        column,
                        value: scalar_column_value(value, column)?,
                    });
                }
                FieldColumnMapping::Encrypted {
                    algorithm,
                    ciphertext,
                    nonce,
                    remote_path,
                } => {
                    let Some(value) = get_object_path(input, remote_path) else {
                        continue;
                    };
                    let envelope = encrypted_column_values(value, algorithm)?;
                    assignments.push(ColumnAssignment {
                        column: ciphertext,
                        value: envelope.ciphertext,
                    });
                    assignments.push(ColumnAssignment {
                        column: nonce,
                        value: envelope.nonce,
                    });
                }
            }
        }

        Ok(assignments)
    }

    async fn get_by_id(&self, pool: &PgPool, id: &str) -> Result<Option<Value>> {
        let sql = format!(
            "SELECT {} FROM {} WHERE {} = CAST($1 AS {})",
            self.select_columns_sql(),
            self.quoted_table_name(),
            self.quoted_column_name(&self.primary_key.column_name),
            self.primary_key.sql_type,
        );

        let row = sqlx::query(&sql)
            .bind(id)
            .fetch_optional(pool)
            .await
            .with_context(|| format!("Failed to load entity '{}' with id '{}'.", self.entity.name, id))?;

        row.map(|value| self.row_to_remote(&value)).transpose()
    }

    async fn list(&self, pool: &PgPool) -> Result<Vec<Value>> {
        let sql = format!(
            "SELECT {} FROM {} ORDER BY {}",
            self.select_columns_sql(),
            self.quoted_table_name(),
            self.quoted_column_name(&self.primary_key.column_name),
        );

        let rows = sqlx::query(&sql)
            .fetch_all(pool)
            .await
            .with_context(|| format!("Failed to list entities for '{}'.", self.entity.name))?;

        rows.iter().map(|row| self.row_to_remote(row)).collect()
    }

    fn quoted_column_name(&self, column_name: &str) -> String {
        quote_identifier(column_name)
    }

    fn quoted_table_name(&self) -> String {
        quote_identifier(&self.table.table_name)
    }

    fn row_to_remote(&self, row: &PgRow) -> Result<Value> {
        let mut remote = Map::new();

        for field in &self.fields {
            match field {
                FieldColumnMapping::Scalar { column, remote_path } => {
                    set_object_path(&mut remote, remote_path, decode_scalar_column(row, column)?);
                }
                FieldColumnMapping::Encrypted {
                    algorithm,
                    ciphertext,
                    nonce,
                    remote_path,
                } => {
                    let ciphertext_base64 = row.try_get::<Option<String>, _>(ciphertext.column_name.as_str())?;
                    let nonce_base64 = row.try_get::<Option<String>, _>(nonce.column_name.as_str())?;

                    let value = match (ciphertext_base64, nonce_base64) {
                        (None, None) => Value::Null,
                        (Some(ciphertext_base64), Some(nonce_base64)) => {
                            let mut envelope = Map::new();
                            envelope.insert(
                                "algorithm".to_owned(),
                                Value::String((*algorithm).to_owned()),
                            );
                            envelope.insert(
                                "ciphertextBase64".to_owned(),
                                Value::String(ciphertext_base64),
                            );
                            envelope.insert(
                                "nonceBase64".to_owned(),
                                Value::String(nonce_base64),
                            );
                            envelope.insert("version".to_owned(), Value::Number(Number::from(1)));
                            Value::Object(envelope)
                        }
                        _ => bail!(
                            "Encrypted field '{}' in '{}' contains incomplete ciphertext/nonce data.",
                            remote_path,
                            self.entity.table_name
                        ),
                    };

                    set_object_path(&mut remote, remote_path, value);
                }
            }
        }

        Ok(Value::Object(remote))
    }

    fn select_columns_sql(&self) -> String {
        let mut seen = std::collections::BTreeSet::new();
        let mut columns = Vec::new();

        for field in &self.fields {
            match field {
                FieldColumnMapping::Scalar { column, .. } => {
                    if seen.insert(column.column_name.as_str()) {
                        columns.push(select_expression(column));
                    }
                }
                FieldColumnMapping::Encrypted { ciphertext, nonce, .. } => {
                    if seen.insert(ciphertext.column_name.as_str()) {
                        columns.push(select_expression(ciphertext));
                    }
                    if seen.insert(nonce.column_name.as_str()) {
                        columns.push(select_expression(nonce));
                    }
                }
            }
        }

        columns.join(", ")
    }
}

enum AssignedColumnValue {
    Bool(bool),
    Bytes(Vec<u8>),
    Float(f64),
    Int(i64),
    Json(Value),
    Null,
    Text(String),
}

struct ColumnAssignment<'a> {
    column: &'a ExpectedEntityColumnManifest,
    value: AssignedColumnValue,
}

enum FieldColumnMapping<'a> {
    Encrypted {
        algorithm: &'a str,
        ciphertext: &'a ExpectedEntityColumnManifest,
        nonce: &'a ExpectedEntityColumnManifest,
        remote_path: &'a str,
    },
    Scalar {
        column: &'a ExpectedEntityColumnManifest,
        remote_path: &'a str,
    },
}

struct EncryptedColumnValues {
    ciphertext: AssignedColumnValue,
    nonce: AssignedColumnValue,
}

fn decode_scalar_column(row: &PgRow, column: &ExpectedEntityColumnManifest) -> Result<Value> {
    Ok(match sql_kind(&column.sql_type) {
        SqlKind::Bool => row
            .try_get::<Option<bool>, _>(column.column_name.as_str())?
            .map(Value::Bool)
            .unwrap_or(Value::Null),
        SqlKind::Float => row
            .try_get::<Option<f64>, _>(column.column_name.as_str())?
            .and_then(Number::from_f64)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        SqlKind::Int => row
            .try_get::<Option<i64>, _>(column.column_name.as_str())?
            .map(|value| Value::Number(Number::from(value)))
            .unwrap_or(Value::Null),
        SqlKind::Json => row
            .try_get::<Option<Json<Value>>, _>(column.column_name.as_str())?
            .map(|Json(value)| value)
            .unwrap_or(Value::Null),
        SqlKind::Text => row
            .try_get::<Option<String>, _>(column.column_name.as_str())?
            .map(Value::String)
            .unwrap_or(Value::Null),
    })
}

fn encrypted_column_values(value: &Value, algorithm: &str) -> Result<EncryptedColumnValues> {
    if value.is_null() {
        return Ok(EncryptedColumnValues {
            ciphertext: AssignedColumnValue::Null,
            nonce: AssignedColumnValue::Null,
        });
    }

    let object = value
        .as_object()
        .ok_or_else(|| anyhow!("Encrypted field payload must be an object or null."))?;
    let payload_algorithm = object
        .get("algorithm")
        .and_then(Value::as_str)
        .unwrap_or(LEGACY_ENCRYPTION_ALGORITHM);
    if payload_algorithm != algorithm {
        bail!(
            "Encrypted field payload uses algorithm '{}' but the runtime only supports '{}'.",
            payload_algorithm,
            algorithm
        );
    }
    if object.contains_key("encapsulatedKeyCiphertextBase64") || object.contains_key("metadata") {
        bail!("Encrypted field payload includes fields that cannot be stored in legacy ciphertext/nonce columns.");
    }

    let ciphertext_base64 = object
        .get("ciphertextBase64")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("Encrypted field payload is missing 'ciphertextBase64'."))?;
    let nonce_base64 = object
        .get("nonceBase64")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("Encrypted field payload is missing 'nonceBase64'."))?;

    Ok(EncryptedColumnValues {
        ciphertext: AssignedColumnValue::Bytes(
            STANDARD
                .decode(ciphertext_base64)
                .context("Encrypted field ciphertextBase64 is not valid base64.")?,
        ),
        nonce: AssignedColumnValue::Bytes(
            STANDARD
                .decode(nonce_base64)
                .context("Encrypted field nonceBase64 is not valid base64.")?,
        ),
    })
}

fn field_base_column_name(path: &str) -> String {
    let mut output = String::new();

    for ch in path.chars() {
        match ch {
            '.' => {
                if !output.ends_with('_') {
                    output.push('_');
                }
            }
            value if value.is_ascii_uppercase() => {
                if !output.is_empty() && !output.ends_with('_') {
                    output.push('_');
                }
                output.push(value.to_ascii_lowercase());
            }
            value if value.is_ascii_alphanumeric() || value == '_' => {
                output.push(value.to_ascii_lowercase());
            }
            _ => {
                if !output.ends_with('_') {
                    output.push('_');
                }
            }
        }
    }

    output.trim_matches('_').to_owned()
}

fn get_object_path<'a>(target: &'a Map<String, Value>, path: &str) -> Option<&'a Value> {
    target_at_path(target, path)
}

fn push_assignment_value(
    separated: &mut sqlx::query_builder::Separated<'_, '_, Postgres, &str>,
    assignment: &ColumnAssignment<'_>,
) {
    match &assignment.value {
        AssignedColumnValue::Null => {
            separated.push("NULL");
        }
        AssignedColumnValue::Bool(value) => {
            separated.push("CAST(");
            separated.push_bind(*value);
            separated.push(" AS ");
            separated.push(assignment.column.sql_type.as_str());
            separated.push(")");
        }
        AssignedColumnValue::Bytes(value) => {
            separated.push_bind(value.clone());
        }
        AssignedColumnValue::Float(value) => {
            separated.push("CAST(");
            separated.push_bind(*value);
            separated.push(" AS ");
            separated.push(assignment.column.sql_type.as_str());
            separated.push(")");
        }
        AssignedColumnValue::Int(value) => {
            separated.push("CAST(");
            separated.push_bind(*value);
            separated.push(" AS ");
            separated.push(assignment.column.sql_type.as_str());
            separated.push(")");
        }
        AssignedColumnValue::Json(value) => {
            separated.push("CAST(");
            separated.push_bind(Json(value.clone()));
            separated.push(" AS ");
            separated.push(assignment.column.sql_type.as_str());
            separated.push(")");
        }
        AssignedColumnValue::Text(value) => {
            separated.push("CAST(");
            separated.push_bind(value.clone());
            separated.push(" AS ");
            separated.push(assignment.column.sql_type.as_str());
            separated.push(")");
        }
    }
}

fn quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn scalar_column_value(value: &Value, column: &ExpectedEntityColumnManifest) -> Result<AssignedColumnValue> {
    if value.is_null() {
        return Ok(AssignedColumnValue::Null);
    }

    Ok(match sql_kind(&column.sql_type) {
        SqlKind::Bool => AssignedColumnValue::Bool(
            value
                .as_bool()
                .ok_or_else(|| anyhow!("Column '{}' expects a boolean value.", column.column_name))?,
        ),
        SqlKind::Float => AssignedColumnValue::Float(
            value
                .as_f64()
                .ok_or_else(|| anyhow!("Column '{}' expects a numeric value.", column.column_name))?,
        ),
        SqlKind::Int => AssignedColumnValue::Int(
            value
                .as_i64()
                .ok_or_else(|| anyhow!("Column '{}' expects an integer value.", column.column_name))?,
        ),
        SqlKind::Json => AssignedColumnValue::Json(value.clone()),
        SqlKind::Text => AssignedColumnValue::Text(
            value
                .as_str()
                .ok_or_else(|| anyhow!("Column '{}' expects a string value.", column.column_name))?
                .to_owned(),
        ),
    })
}

fn select_expression(column: &ExpectedEntityColumnManifest) -> String {
    let identifier = quote_identifier(&column.column_name);
    match sql_kind(&column.sql_type) {
        SqlKind::Bool => format!("{identifier} AS {identifier}"),
        SqlKind::Int => format!("CAST({identifier} AS BIGINT) AS {identifier}"),
        SqlKind::Float => format!("CAST({identifier} AS DOUBLE PRECISION) AS {identifier}"),
        SqlKind::Json => format!("{identifier} AS {identifier}"),
        SqlKind::Text => {
            if column.sql_type.eq_ignore_ascii_case("BYTEA") {
                format!("encode({identifier}, 'base64') AS {identifier}")
            } else {
                format!("CAST({identifier} AS TEXT) AS {identifier}")
            }
        }
    }
}

enum SqlKind {
    Bool,
    Float,
    Int,
    Json,
    Text,
}

fn sql_kind(sql_type: &str) -> SqlKind {
    match sql_type.to_ascii_uppercase().as_str() {
        "BOOL" | "BOOLEAN" => SqlKind::Bool,
        "FLOAT4" | "FLOAT8" | "NUMERIC" | "REAL" | "DOUBLE PRECISION" => SqlKind::Float,
        "INT2" | "INT4" | "INT8" | "INTEGER" | "BIGINT" | "SMALLINT" => SqlKind::Int,
        "JSON" | "JSONB" => SqlKind::Json,
        _ => SqlKind::Text,
    }
}

fn set_object_path(target: &mut Map<String, Value>, path: &str, value: Value) {
    let segments = path.split('.').collect::<Vec<_>>();
    let Some(last_segment) = segments.last().copied() else {
        return;
    };

    let mut current = target;
    for segment in &segments[..segments.len().saturating_sub(1)] {
        let next = current
            .entry((*segment).to_owned())
            .or_insert_with(|| Value::Object(Map::new()));
        if !next.is_object() {
            *next = Value::Object(Map::new());
        }
        current = next.as_object_mut().expect("object entry");
    }

    current.insert(last_segment.to_owned(), value);
}

fn target_at_path<'a>(target: &'a Map<String, Value>, path: &str) -> Option<&'a Value> {
    let mut current = target;
    let mut segments = path.split('.').peekable();

    while let Some(segment) = segments.next() {
        let value = current.get(segment)?;
        if segments.peek().is_none() {
            return Some(value);
        }
        current = value.as_object()?;
    }

    None
}

#[cfg(test)]
mod tests {
    use super::{encrypted_column_values, field_base_column_name, set_object_path};
    use serde_json::{json, Map, Value};

    #[test]
    fn encrypted_column_values_convert_legacy_payload() {
        let value = json!({
            "algorithm": "aes-256-gcm",
            "ciphertextBase64": "YQ==",
            "nonceBase64": "Yg==",
            "version": 1,
        });

        let envelope = encrypted_column_values(&value, "aes-256-gcm").expect("payload should map");

        match envelope.ciphertext {
            super::AssignedColumnValue::Bytes(bytes) => assert_eq!(bytes, b"a"),
            _ => panic!("ciphertext should map to bytes"),
        }
        match envelope.nonce {
            super::AssignedColumnValue::Bytes(bytes) => assert_eq!(bytes, b"b"),
            _ => panic!("nonce should map to bytes"),
        }
    }

    #[test]
    fn field_base_column_name_handles_nested_camel_case_paths() {
        assert_eq!(field_base_column_name("configEnvelope"), "config_envelope");
        assert_eq!(field_base_column_name("settings.apiUrl"), "settings_api_url");
    }

    #[test]
    fn set_object_path_creates_nested_objects() {
        let mut target = Map::new();
        set_object_path(&mut target, "configEnvelope.nonceBase64", Value::String("abc".to_owned()));

        assert_eq!(
            Value::Object(target),
            json!({ "configEnvelope": { "nonceBase64": "abc" } })
        );
    }
}