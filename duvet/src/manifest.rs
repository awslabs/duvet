// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{annotation::AnnotationType, comment, source::SourceFile};
use duvet_core::{glob::Glob, path::Path, query, vfs, Result};
use futures::StreamExt;
use std::{collections::HashSet, sync::Arc};

#[derive(Debug)]
pub struct Manifest {
    pub compliance: Compliance,
}

#[derive(Debug)]
pub struct Compliance {
    pub sources: Arc<[Source]>,
    pub requirements: Arc<[Requirement]>,
}

#[derive(Debug)]
pub struct Source {
    pub pattern: Glob,
    pub root: Path,
    pub comment_style: comment::Pattern,
    pub default_type: AnnotationType,
}

#[derive(Debug)]
pub struct Requirement {
    pub pattern: Glob,
    pub root: Path,
}

#[query]
pub async fn load() -> Result<Arc<Manifest>> {
    let mut sources = vec![];
    let mut requirements = vec![];

    let arguments = crate::arguments::get().await;

    arguments.load_sources(&mut sources);
    arguments.load_requirements(&mut requirements);

    if let Some(config) = crate::config::load().await {
        let config = config?;
        config.load_sources(&mut sources);
        config.load_requirements(&mut requirements);
    }

    let manifest = Manifest {
        compliance: Compliance {
            sources: sources.into(),
            requirements: requirements.into(),
        },
    };

    Ok(Arc::new(manifest))
}

// TODO pull this from `.gitignore`s
fn ignores() -> Glob {
    Glob::try_from_iter(["**/.git", "**/node_modules", "**/target", "**/build"]).unwrap()
}

#[query]
pub async fn sources() -> Result<Arc<HashSet<SourceFile>>> {
    let manifest = load().await?;

    let mut sources = HashSet::new();

    let ignores = ignores();

    for source in manifest.compliance.sources.iter() {
        let root = vfs::read_dir(&source.root).await?;
        let glob = root.glob(source.pattern.clone(), ignores.clone());
        tokio::pin!(glob);

        while let Some(entry) = glob.next().await {
            sources.insert(SourceFile::Text(
                source.comment_style.clone(),
                entry,
                source.default_type,
            ));
        }
    }

    Ok(Arc::new(sources))
}

#[query]
pub async fn requirements() -> Result<Arc<HashSet<SourceFile>>> {
    let manifest = load().await?;

    let mut sources = HashSet::new();

    let ignores = ignores();

    for requirement in manifest.compliance.requirements.iter() {
        let root = vfs::read_dir(&requirement.root).await?;
        let glob = root.glob(requirement.pattern.clone(), ignores.clone());
        tokio::pin!(glob);

        while let Some(entry) = glob.next().await {
            sources.insert(SourceFile::Spec(entry));
        }
    }

    Ok(Arc::new(sources))
}
