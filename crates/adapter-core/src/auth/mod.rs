use std::time::Duration;

use argon2::password_hash::{PasswordHasher, SaltString};
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::{
    http::{
        header::{COOKIE, SET_COOKIE},
        HeaderMap, HeaderValue, StatusCode,
    },
    response::{IntoResponse, Response},
    Json,
};
use base64::Engine;
use chrono::{DateTime, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::manifest::SessionManifest;

#[derive(Clone, Debug, Serialize)]
pub struct AuthRouteSummary {
    pub get_kdf_salt: String,
    pub login: String,
    pub logout: String,
    pub refresh: String,
    pub register_begin: String,
    pub register_complete: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AuthenticatedUser {
    pub email: String,
    pub id: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AuthPayload {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<AuthenticatedUser>,
}

#[derive(Clone, Debug, Serialize)]
pub struct KdfSaltResponse {
    #[serde(rename = "kdfSaltBase64")]
    pub kdf_salt_base64: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct EmailBody {
    pub email: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct EmailQuery {
    pub email: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AuthKeyBody {
    #[serde(rename = "authKeyMaterialHex")]
    pub auth_key_material_hex: String,
    pub email: String,
}

#[derive(Clone, Debug, Serialize)]
struct ErrorBody {
    error: String,
}

#[derive(Debug)]
pub struct AuthError {
    message: String,
    status: StatusCode,
}

impl AuthError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            status: StatusCode::BAD_REQUEST,
        }
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            status: StatusCode::CONFLICT,
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            status: StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            status: StatusCode::NOT_FOUND,
        }
    }

    pub fn status(&self) -> StatusCode {
        self.status
    }
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorBody {
                error: self.message,
            }),
        )
            .into_response()
    }
}

impl From<sqlx::Error> for AuthError {
    fn from(value: sqlx::Error) -> Self {
        Self::internal(value.to_string())
    }
}

pub struct AuthResponseWithCookies {
    pub cookies: Vec<HeaderValue>,
    pub payload: AuthPayload,
}

#[derive(Clone, Debug, FromRow)]
struct DbUserRecord {
    id: Uuid,
    email: String,
    kdf_salt: Vec<u8>,
    auth_key_hash: Option<String>,
}

#[derive(Clone, Debug, FromRow)]
struct DbSessionRecord {
    id: Uuid,
    user_id: Uuid,
    expires_at: DateTime<Utc>,
    refresh_expires_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
}

pub fn attach_cookies<T>(payload: T, cookies: Vec<HeaderValue>) -> Response
where
    T: Serialize,
{
    let mut response = Json(payload).into_response();
    for cookie in cookies {
        response.headers_mut().append(SET_COOKIE, cookie);
    }
    response
}

pub async fn authenticated_user_from_headers(
    headers: &HeaderMap,
    pool: &PgPool,
    session_manifest: &SessionManifest,
) -> Result<Option<AuthenticatedUser>, AuthError> {
    let Some(token) = read_cookie(headers, &session_manifest.cookie_names.session) else {
        return Ok(None);
    };

    let session = sqlx::query_as::<_, DbSessionRecord>(
        r#"
        SELECT id, user_id, expires_at, refresh_expires_at, revoked_at
        FROM sessions
        WHERE session_token_hash = $1
        "#,
    )
    .bind(hash_token(&token))
    .fetch_optional(pool)
    .await?;

    let Some(session) = session else {
        return Ok(None);
    };

    if session.revoked_at.is_some() || session.expires_at < Utc::now() {
        return Ok(None);
    }

    let user = sqlx::query_as::<_, DbUserRecord>(
        r#"
        SELECT id, email, kdf_salt, auth_key_hash
        FROM users
        WHERE id = $1
        "#,
    )
    .bind(session.user_id)
    .fetch_optional(pool)
    .await?;

    Ok(user.map(Into::into))
}

pub async fn get_kdf_salt(pool: &PgPool, email: &str) -> Result<KdfSaltResponse, AuthError> {
    let email = normalize_email(email)?;
    let user = find_user_by_email(pool, &email).await?;
    let Some(user) = user else {
        return Err(AuthError::not_found("Unknown email"));
    };

    Ok(KdfSaltResponse {
        kdf_salt_base64: base64_encode(&user.kdf_salt),
    })
}

pub async fn login(
    pool: &PgPool,
    body: AuthKeyBody,
    session_manifest: &SessionManifest,
    secure_cookies: bool,
) -> Result<AuthResponseWithCookies, AuthError> {
    let email = normalize_email(&body.email)?;
    let material = decode_hex(&body.auth_key_material_hex)?;
    let user = find_user_by_email(pool, &email).await?;

    let Some(user) = user else {
        return Ok(AuthResponseWithCookies {
            cookies: Vec::new(),
            payload: invalid_credentials_payload(),
        });
    };

    let Some(stored_hash) = user.auth_key_hash.as_deref() else {
        return Ok(AuthResponseWithCookies {
            cookies: Vec::new(),
            payload: AuthPayload {
                ok: false,
                message: Some("Registration not completed".to_owned()),
                user: None,
            },
        });
    };

    if !verify_auth_key_material(&material, stored_hash) {
        return Ok(AuthResponseWithCookies {
            cookies: Vec::new(),
            payload: invalid_credentials_payload(),
        });
    }

    issue_session(pool, &user, session_manifest, secure_cookies).await
}

pub async fn logout(
    headers: &HeaderMap,
    pool: &PgPool,
    session_manifest: &SessionManifest,
    secure_cookies: bool,
) -> Result<Vec<HeaderValue>, AuthError> {
    if let Some(token) = read_cookie(headers, &session_manifest.cookie_names.session) {
        sqlx::query(
            r#"
            UPDATE sessions
            SET revoked_at = $2
            WHERE session_token_hash = $1
            "#,
        )
        .bind(hash_token(&token))
        .bind(Utc::now())
        .execute(pool)
        .await?;
    }

    clear_auth_cookies(session_manifest, secure_cookies)
}

pub async fn refresh(
    headers: &HeaderMap,
    pool: &PgPool,
    session_manifest: &SessionManifest,
    secure_cookies: bool,
) -> Result<AuthResponseWithCookies, AuthError> {
    let Some(token) = read_cookie(headers, &session_manifest.cookie_names.refresh) else {
        return Ok(AuthResponseWithCookies {
            cookies: Vec::new(),
            payload: AuthPayload {
                ok: false,
                message: Some("Missing refresh token".to_owned()),
                user: None,
            },
        });
    };

    let session = sqlx::query_as::<_, DbSessionRecord>(
        r#"
        SELECT id, user_id, expires_at, refresh_expires_at, revoked_at
        FROM sessions
        WHERE refresh_token_hash = $1
        "#,
    )
    .bind(hash_token(&token))
    .fetch_optional(pool)
    .await?;

    let Some(session) = session else {
        return Ok(AuthResponseWithCookies {
            cookies: Vec::new(),
            payload: AuthPayload {
                ok: false,
                message: Some("Invalid refresh token".to_owned()),
                user: None,
            },
        });
    };

    if session.revoked_at.is_some() {
        return Ok(AuthResponseWithCookies {
            cookies: Vec::new(),
            payload: AuthPayload {
                ok: false,
                message: Some("Session revoked".to_owned()),
                user: None,
            },
        });
    }

    if session.refresh_expires_at < Utc::now() {
        return Ok(AuthResponseWithCookies {
            cookies: Vec::new(),
            payload: AuthPayload {
                ok: false,
                message: Some("Refresh expired".to_owned()),
                user: None,
            },
        });
    }

    let user = sqlx::query_as::<_, DbUserRecord>(
        r#"
        SELECT id, email, kdf_salt, auth_key_hash
        FROM users
        WHERE id = $1
        "#,
    )
    .bind(session.user_id)
    .fetch_optional(pool)
    .await?;

    let Some(user) = user else {
        return Ok(AuthResponseWithCookies {
            cookies: Vec::new(),
            payload: AuthPayload {
                ok: false,
                message: Some("User missing".to_owned()),
                user: None,
            },
        });
    };

    let next_session_token = new_session_token();
    let next_refresh_token = new_refresh_token();
    let now = Utc::now();
    let expires_at = now + chrono::Duration::seconds(session_manifest.session_duration_seconds as i64);
    let refresh_expires_at =
        now + chrono::Duration::seconds(session_manifest.refresh_duration_seconds as i64);

    sqlx::query(
        r#"
        UPDATE sessions
        SET session_token_hash = $2,
            refresh_token_hash = $3,
            expires_at = $4,
            refresh_expires_at = $5,
            revoked_at = NULL
        WHERE id = $1
        "#,
    )
    .bind(session.id)
    .bind(hash_token(&next_session_token))
    .bind(hash_token(&next_refresh_token))
    .bind(expires_at)
    .bind(refresh_expires_at)
    .execute(pool)
    .await?;

    Ok(AuthResponseWithCookies {
        cookies: session_cookie_headers(
            &next_session_token,
            &next_refresh_token,
            session_manifest,
            secure_cookies,
        )?,
        payload: AuthPayload {
            ok: true,
            message: None,
            user: Some(user.into()),
        },
    })
}

pub async fn register_begin(pool: &PgPool, body: EmailBody) -> Result<KdfSaltResponse, AuthError> {
    let email = normalize_email(&body.email)?;
    if find_user_by_email(pool, &email).await?.is_some() {
        return Err(AuthError::conflict("Email already registered"));
    }

    let mut salt = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut salt);
    let now = Utc::now();

    sqlx::query(
        r#"
        INSERT INTO users (id, email, kdf_salt, auth_key_hash, default_dashboard_id, created_at, updated_at)
        VALUES ($1, $2, $3, NULL, NULL, $4, $4)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(&email)
    .bind(salt.to_vec())
    .bind(now)
    .execute(pool)
    .await?;

    Ok(KdfSaltResponse {
        kdf_salt_base64: base64_encode(&salt),
    })
}

pub async fn register_complete(
    pool: &PgPool,
    body: AuthKeyBody,
    session_manifest: &SessionManifest,
    secure_cookies: bool,
) -> Result<AuthResponseWithCookies, AuthError> {
    let email = normalize_email(&body.email)?;
    let material = decode_hex(&body.auth_key_material_hex)?;
    let user = sqlx::query_as::<_, DbUserRecord>(
        r#"
        SELECT id, email, kdf_salt, auth_key_hash
        FROM users
        WHERE email = $1 AND auth_key_hash IS NULL
        "#,
    )
    .bind(&email)
    .fetch_optional(pool)
    .await?;

    let Some(user) = user else {
        return Ok(AuthResponseWithCookies {
            cookies: Vec::new(),
            payload: AuthPayload {
                ok: false,
                message: Some("No pending registration for this email".to_owned()),
                user: None,
            },
        });
    };

    let hash = hash_auth_key_material(&material)
        .map_err(|value| AuthError::internal(value.to_string()))?;

    sqlx::query(
        r#"
        UPDATE users
        SET auth_key_hash = $2, updated_at = $3
        WHERE id = $1
        "#,
    )
    .bind(user.id)
    .bind(hash)
    .bind(Utc::now())
    .execute(pool)
    .await?;

    issue_session(pool, &user, session_manifest, secure_cookies).await
}

fn base64_encode(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn build_cookie(
    name: &str,
    value: &str,
    max_age: Duration,
    secure: bool,
) -> Result<HeaderValue, AuthError> {
    let secure_flag = if secure { "; Secure" } else { "" };
    let cookie = format!(
        "{name}={value}; Path=/; HttpOnly; SameSite=Lax{secure_flag}; Max-Age={}",
        max_age.as_secs()
    );

    HeaderValue::from_str(&cookie).map_err(|value| AuthError::internal(value.to_string()))
}

fn clear_auth_cookies(
    session_manifest: &SessionManifest,
    secure: bool,
) -> Result<Vec<HeaderValue>, AuthError> {
    Ok(vec![
        build_cookie(
            &session_manifest.cookie_names.session,
            "",
            Duration::from_secs(0),
            secure,
        )?,
        build_cookie(
            &session_manifest.cookie_names.refresh,
            "",
            Duration::from_secs(0),
            secure,
        )?,
    ])
}

fn decode_hex(value: &str) -> Result<Vec<u8>, AuthError> {
    hex::decode(value.trim()).map_err(|_| AuthError::bad_request("Invalid hex"))
}

async fn find_user_by_email(pool: &PgPool, email: &str) -> Result<Option<DbUserRecord>, AuthError> {
    sqlx::query_as::<_, DbUserRecord>(
        r#"
        SELECT id, email, kdf_salt, auth_key_hash
        FROM users
        WHERE email = $1
        "#,
    )
    .bind(email)
    .fetch_optional(pool)
    .await
    .map_err(Into::into)
}

fn hash_auth_key_material(material: &[u8]) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut rand::thread_rng());
    Argon2::default()
        .hash_password(material, &salt)
        .map(|value| value.to_string())
}

fn hash_token(token: &str) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hasher.finalize().to_vec()
}

async fn issue_session(
    pool: &PgPool,
    user: &DbUserRecord,
    session_manifest: &SessionManifest,
    secure_cookies: bool,
) -> Result<AuthResponseWithCookies, AuthError> {
    let session_token = new_session_token();
    let refresh_token = new_refresh_token();
    let now = Utc::now();
    let expires_at = now + chrono::Duration::seconds(session_manifest.session_duration_seconds as i64);
    let refresh_expires_at =
        now + chrono::Duration::seconds(session_manifest.refresh_duration_seconds as i64);

    sqlx::query(
        r#"
        INSERT INTO sessions (
            id,
            user_id,
            session_token_hash,
            refresh_token_hash,
            expires_at,
            refresh_expires_at,
            created_at,
            revoked_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, NULL)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(user.id)
    .bind(hash_token(&session_token))
    .bind(hash_token(&refresh_token))
    .bind(expires_at)
    .bind(refresh_expires_at)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(AuthResponseWithCookies {
        cookies: session_cookie_headers(
            &session_token,
            &refresh_token,
            session_manifest,
            secure_cookies,
        )?,
        payload: AuthPayload {
            ok: true,
            message: None,
            user: Some(user.clone().into()),
        },
    })
}

fn invalid_credentials_payload() -> AuthPayload {
    AuthPayload {
        ok: false,
        message: Some("Invalid credentials".to_owned()),
        user: None,
    }
}

fn new_refresh_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn new_session_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn normalize_email(email: &str) -> Result<String, AuthError> {
    let normalized = email.trim().to_lowercase();
    if normalized.is_empty() || !normalized.contains('@') {
        return Err(AuthError::bad_request("Invalid email"));
    }
    Ok(normalized)
}

fn read_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    let header = headers.get(COOKIE)?.to_str().ok()?;
    header.split(';').find_map(|segment| {
        let mut parts = segment.trim().splitn(2, '=');
        let key = parts.next()?.trim();
        let value = parts.next()?.trim();
        (key == name).then(|| value.to_owned())
    })
}

fn session_cookie_headers(
    session_token: &str,
    refresh_token: &str,
    session_manifest: &SessionManifest,
    secure: bool,
) -> Result<Vec<HeaderValue>, AuthError> {
    Ok(vec![
        build_cookie(
            &session_manifest.cookie_names.session,
            session_token,
            Duration::from_secs(session_manifest.session_duration_seconds),
            secure,
        )?,
        build_cookie(
            &session_manifest.cookie_names.refresh,
            refresh_token,
            Duration::from_secs(session_manifest.refresh_duration_seconds),
            secure,
        )?,
    ])
}

fn verify_auth_key_material(material: &[u8], stored: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(stored) else {
        return false;
    };

    Argon2::default()
        .verify_password(material, &parsed)
        .is_ok()
}

impl From<DbUserRecord> for AuthenticatedUser {
    fn from(value: DbUserRecord) -> Self {
        Self {
            email: value.email,
            id: value.id.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        attach_cookies, build_cookie, hash_auth_key_material, normalize_email, read_cookie,
        session_cookie_headers, verify_auth_key_material, AuthPayload,
    };
    use axum::response::IntoResponse;
    use crate::manifest::{SessionCookieNames, SessionManifest};
    use axum::http::{header::SET_COOKIE, HeaderMap};

    fn session_manifest() -> SessionManifest {
        SessionManifest {
            cookie_names: SessionCookieNames {
                refresh: "refresh_token".to_owned(),
                session: "session_token".to_owned(),
            },
            refresh_duration_seconds: 120,
            session_duration_seconds: 60,
        }
    }

    #[test]
    fn normalize_email_trims_and_lowercases() {
        let normalized = normalize_email("  User@Example.COM ").expect("email should normalize");

        assert_eq!(normalized, "user@example.com");
    }

    #[test]
    fn normalize_email_rejects_invalid_values() {
        let error = normalize_email("not-an-email").expect_err("invalid email should fail");

        assert_eq!(error.into_response().status().as_u16(), 400);
    }

    #[test]
    fn password_hash_round_trip_verifies() {
        let material = b"secret-material";
        let hash = hash_auth_key_material(material).expect("hashing should succeed");

        assert!(verify_auth_key_material(material, &hash));
        assert!(!verify_auth_key_material(b"wrong-material", &hash));
    }

    #[test]
    fn read_cookie_extracts_named_cookie() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            "foo=bar; session_token=abc123; refresh_token=def456"
                .parse()
                .expect("cookie header should parse"),
        );

        let cookie = read_cookie(&headers, "session_token");

        assert_eq!(cookie.as_deref(), Some("abc123"));
    }

    #[test]
    fn session_cookie_headers_use_manifest_names_and_secure_flag() {
        let headers = session_cookie_headers("sess", "ref", &session_manifest(), true)
            .expect("cookie headers should build");
        let values = headers
            .iter()
            .map(|value| value.to_str().expect("header should be utf-8"))
            .collect::<Vec<_>>();

        assert_eq!(values.len(), 2);
        assert!(values[0].contains("session_token=sess"));
        assert!(values[0].contains("Secure"));
        assert!(values[1].contains("refresh_token=ref"));
    }

    #[test]
    fn attach_cookies_appends_set_cookie_headers() {
        let cookie = build_cookie("session_token", "sess", std::time::Duration::from_secs(60), false)
            .expect("cookie should build");
        let response = attach_cookies(
            AuthPayload {
                ok: true,
                message: None,
                user: None,
            },
            vec![cookie],
        );

        let cookie_headers = response.headers().get_all(SET_COOKIE);
        let values = cookie_headers.iter().collect::<Vec<_>>();

        assert_eq!(values.len(), 1);
        assert_eq!(response.status().as_u16(), 200);
    }
}
