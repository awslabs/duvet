use crate::manifest::{v1::compliance::CommentStyle, Schema};
use duvet_core::{
    diagnostic::Set,
    file::{Slice, SourceFile},
    glob::Glob,
    path::Path,
    vfs, Result,
};
use futures::StreamExt;
use std::sync::Arc;

pub mod parser;
pub mod tokenizer;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Comment {
    pub meta: Vec<Meta>,
    pub contents: Vec<Slice<SourceFile>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Meta {
    pub key: Option<Slice<SourceFile>>,
    pub value: Slice<SourceFile>,
}

pub type List = Arc<[Comment]>;
pub type Group = Arc<[List]>;

pub async fn query_group(manifest: Schema) -> (Group, Set) {
    let dir = manifest
        .manifest_dir()
        .await
        .expect("could not load manifest dir");

    // TODO load from the `.gitignore`
    let ignore = Glob::try_from_iter(["**/target", "**/.git", "**/node_modules"]).unwrap();

    let mut annotations = vec![];
    let mut errors = vec![];

    for (pattern, comments) in manifest.compliance_sources() {
        let mut files = Box::pin(dir.glob(pattern, ignore.clone()));

        while let Some(path) = files.next().await {
            match query_file(path, comments.clone()).await {
                Ok(for_file) => {
                    if let Some(for_file) = for_file {
                        annotations.push(for_file);
                    }
                }
                Err(error) => {
                    errors.push(error);
                }
            }
        }
    }

    (annotations.into(), errors.into())
}

#[duvet_core::query(spawn)]
pub async fn query_file(path: Path, comments: CommentStyle) -> Result<Option<List>> {
    let file = vfs::read_string(&path).await?;
    let tokens = tokenizer::tokens(&file, &comments);
    let list: List = parser::parse(tokens).collect();

    if list.is_empty() {
        return Ok(None);
    }

    Ok(Some(list))
}
