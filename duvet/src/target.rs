// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{annotation::Annotation, specification::Format, Error, Result};
use core::{fmt, str::FromStr};
use duvet_core::{diagnostic::IntoDiagnostic, file::SourceFile};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};
use url::Url;

pub type TargetSet = HashSet<Target>;

#[derive(Clone, Debug, PartialEq, PartialOrd, Ord, Eq, Hash)]
pub struct Target {
    pub path: TargetPath,
    pub format: Format,
}

impl Target {
    pub fn from_annotation(anno: &Annotation) -> Result<Self> {
        let path = TargetPath::from_annotation(anno)?;
        Ok(Self {
            path,
            format: anno.format,
        })
    }
}

impl FromStr for Target {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self {
            path: s.parse()?,
            format: Format::default(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, PartialOrd, Ord, Eq, Hash)]
pub enum TargetPath {
    Url(Url),
    Path(PathBuf),
}

impl fmt::Display for TargetPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Url(url) => url.fmt(f),
            Self::Path(path) => path.display().fmt(f),
        }
    }
}

impl TargetPath {
    pub fn from_annotation(anno: &Annotation) -> Result<Self> {
        let path = anno.target_path();

        // Absolute path
        if path.starts_with('/') {
            return Ok(Self::Path(path.into()));
        }

        // URL style path
        if path.contains("://") {
            let url = Url::parse(path).into_diagnostic()?;
            return Ok(Self::Url(url));
        }

        let path = anno.resolve_file(Path::new(&path))?;
        Ok(Self::Path(path))
    }

    pub async fn load(&self, spec_download_path: Option<&str>) -> Result<SourceFile> {
        match self {
            Self::Url(url) => {
                let path = self.local(spec_download_path);
                let url = Self::canonical_url(url.as_str());
                duvet_core::http::get_cached_string(url, path).await
            }
            Self::Path(path) => duvet_core::vfs::read_string(path).await,
        }
    }

    pub fn local(&self, spec_download_path: Option<&str>) -> PathBuf {
        match self {
            Self::Url(url) => {
                let mut path = if let Some(path_to_spec) = spec_download_path {
                    PathBuf::from_str(path_to_spec).unwrap()
                } else {
                    std::env::current_dir().unwrap()
                };
                path.push("specs");
                path.push(url.host_str().expect("url should have host"));
                path.extend(url.path_segments().expect("url should have path"));
                path.set_extension("txt");
                path
            }
            Self::Path(path) => path.clone(),
        }
    }

    fn canonical_url(url: &str) -> String {
        // rewrite some of the IETF links for convenience
        if let Some(rfc) = url.strip_prefix("https://tools.ietf.org/rfc/") {
            let rfc = rfc.trim_end_matches(".txt").trim_end_matches(".html");
            return format!("https://www.rfc-editor.org/rfc/{}.txt", rfc);
        }

        if url.starts_with("https://www.rfc-editor.org/rfc/") {
            let rfc = url.trim_end_matches(".txt").trim_end_matches(".html");
            return format!("{}.txt", rfc);
        }

        url.to_owned()
    }
}

impl FromStr for TargetPath {
    type Err = Error;

    fn from_str(path: &str) -> Result<Self, Self::Err> {
        // URL style path
        if path.contains("://") {
            let url = Url::parse(path).into_diagnostic()?;
            return Ok(Self::Url(url));
        }

        let path = PathBuf::from(path);
        Ok(Self::Path(path))
    }
}
