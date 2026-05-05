pub mod entity_store;
pub mod postgres;

use anyhow::Result;

pub use postgres::PostgresBackend;

#[allow(async_fn_in_trait)]
pub trait DatabaseBackend {
    fn engine(&self) -> &'static str;
    async fn ping(&self) -> Result<()>;
}
