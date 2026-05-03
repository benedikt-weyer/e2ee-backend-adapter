use anyhow::Result;
use sqlx::{postgres::PgPoolOptions, PgPool};

use super::DatabaseBackend;

const CREATE_USERS_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    kdf_salt BYTEA NOT NULL,
    auth_key_hash TEXT,
    default_dashboard_id UUID,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
)
"#;

const CREATE_SESSIONS_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    session_token_hash BYTEA NOT NULL UNIQUE,
    refresh_token_hash BYTEA NOT NULL UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL,
    refresh_expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ
)
"#;

const CREATE_SESSIONS_USER_ID_INDEX_SQL: &str =
    "CREATE INDEX IF NOT EXISTS sessions_user_id_idx ON sessions(user_id)";

const CREATE_SESSIONS_EXPIRES_AT_INDEX_SQL: &str =
    "CREATE INDEX IF NOT EXISTS sessions_expires_at_idx ON sessions(expires_at)";

#[derive(Clone)]
pub struct PostgresBackend {
    pool: PgPool,
}

impl PostgresBackend {
    pub async fn connect(database_url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;

        ensure_auth_schema(&pool).await?;

        Ok(Self { pool })
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

async fn ensure_auth_schema(pool: &PgPool) -> Result<()> {
    for statement in auth_schema_statements() {
        sqlx::query(statement).execute(pool).await?;
    }

    Ok(())
}

fn auth_schema_statements() -> [&'static str; 4] {
    [
        CREATE_USERS_TABLE_SQL,
        CREATE_SESSIONS_TABLE_SQL,
        CREATE_SESSIONS_USER_ID_INDEX_SQL,
        CREATE_SESSIONS_EXPIRES_AT_INDEX_SQL,
    ]
}

impl DatabaseBackend for PostgresBackend {
    fn engine(&self) -> &'static str {
        "postgres"
    }

    async fn ping(&self) -> Result<()> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::auth_schema_statements;

    #[test]
    fn auth_schema_statements_create_user_and_session_tables() {
        let statements = auth_schema_statements();

        assert!(statements[0].contains("CREATE TABLE IF NOT EXISTS users"));
        assert!(statements[1].contains("CREATE TABLE IF NOT EXISTS sessions"));
    }

    #[test]
    fn auth_schema_statements_include_session_indexes() {
        let statements = auth_schema_statements();

        assert!(statements[2].contains("sessions_user_id_idx"));
        assert!(statements[3].contains("sessions_expires_at_idx"));
    }
}
