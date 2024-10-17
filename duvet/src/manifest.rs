use duvet_core::{path::Path, vfs, Result};

pub mod schema;
pub use schema::{v1, Schema};

pub async fn load(path: Path) -> Result<schema::Schema> {
    let file = vfs::read_string(path).await?;
    Schema::parse(file).await
}
