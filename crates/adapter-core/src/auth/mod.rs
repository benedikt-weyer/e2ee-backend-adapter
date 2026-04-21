use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct AuthRouteSummary {
    pub get_kdf_salt: String,
    pub login: String,
    pub logout: String,
    pub refresh: String,
    pub register_begin: String,
    pub register_complete: String,
}
