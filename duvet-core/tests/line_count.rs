// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use duvet_core::{
    diagnostic::{Error, IntoDiagnostic},
    file::{Slice, SourceFile},
    path::Path,
    query, vfs, Query, Result,
};
use std::collections::BTreeMap;

macro_rules! path {
    ($path:expr) => {
        Path::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/", $path))
    };
}

#[query]
fn manifest_path() -> Path {
    path!("line_count/manifest.txt")
}

type ManifestSource = Result<SourceFile>;

#[query(delegate)]
async fn manifest_source() -> ManifestSource {
    vfs::read_string(manifest_path().await)
}

type ManifestList = Result<Vec<Slice<SourceFile>>>;

#[query]
async fn manifest_parse() -> ManifestList {
    let manifest = manifest_source().await?;

    let lines = manifest.lines();

    let mut out = vec![];
    for line in lines {
        out.push(manifest.substr(line).unwrap());
    }

    Ok(out)
}

type ProjectFiles = Result<BTreeMap<Slice<SourceFile>, Query<Result<SourceFile>>>>;

#[query]
async fn project_files() -> ProjectFiles {
    let files = manifest_parse().await?;
    let path = manifest_path().await;
    let dir = path.parent().unwrap();

    let mut out = BTreeMap::new();
    for path in files {
        let source_code = path.clone();
        let read = vfs::read_string(dir.join(&path[..])).map_cloned(|v| async move {
            let s = v.map_err(move |e| source_code.error(e, "tried to open the file here"))?;
            Ok::<_, Error>(s)
        });
        out.insert(path.clone(), read);
    }

    Ok(out)
}

#[query]
async fn project_line_counts() -> Result<BTreeMap<Slice<SourceFile>, Query<Result<usize>>>> {
    let files = project_files();
    let files = files.get().await.as_ref()?;

    #[query]
    async fn line_counts(file: Query<Result<SourceFile>>) -> Result<usize> {
        let contents = file.get().await.as_ref()?;
        let count = contents.lines().count();
        Ok(count)
    }

    let mut out = BTreeMap::new();
    for (path, contents) in files {
        out.insert(path.clone(), line_counts(contents.clone()));
    }

    Ok(out)
}

#[query]
async fn total_counts() -> Result<usize> {
    let project = project_line_counts();
    let project = project.get().await.as_ref()?;

    let mut out = 0;

    for file in project.values() {
        if let Ok(file) = file.get().await {
            out += file;
        }
    }

    Ok(out)
}

#[tokio::test]
async fn line_count() -> Result<()> {
    assert_eq!(total_counts().await?, 11);

    let files = project_line_counts();
    let files = files.get().await.as_ref()?;

    let mut errors = vec![];

    for (file, counts) in files.iter() {
        let expected = match &**file {
            "a.txt" => 4,
            "b.txt" => 5,
            "c.txt" => 2,
            _ => 0,
        };

        match counts.get().await {
            Ok(actual) => {
                assert_eq!(expected, *actual, "in {file}");
            }
            Err(err) => errors.push(err.clone()),
        }
    }

    errors.into_diagnostic()?;

    Ok(())
}
