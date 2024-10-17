use duvet_core::{dir, file::SourceFile, glob, vfs, Result};
use std::sync::Arc;

pub mod v1;

#[derive(Clone, Debug)]
pub struct Schema {
    v1: Arc<v1::Schema>,
    file: SourceFile,
}

impl Schema {
    pub async fn parse(file: SourceFile) -> Result<Self> {
        match file.path().extension().and_then(|ext| ext.to_str()) {
            Some("toml") => {
                // TODO add version entry
                let manifest = file.as_toml().await?;
                let m = Self { v1: manifest, file };
                Ok(m)
            }
            ext => unimplemented!("{:?}", ext),
        }
    }

    pub fn file(&self) -> &SourceFile {
        &self.file
    }

    pub async fn manifest_dir(&self) -> Result<dir::Directory> {
        let manifest_dir = self.file().path().parent().unwrap();
        vfs::read_dir(manifest_dir.to_owned()).await
    }

    pub fn compliance_sources(
        &self,
    ) -> impl Iterator<Item = (glob::Glob, &v1::compliance::CommentStyle)> {
        self.v1
            .compliance
            .sources
            .iter()
            .map(|source| (source.patterns.clone(), &source.comment_style))
    }
}
