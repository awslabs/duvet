// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{annotation::Annotation, specification::Format, Error};
use core::{fmt, str::FromStr};
use duvet_core::{path::Path, vfs};
use std::collections::HashSet;
use url::Url;

pub type TargetSet = HashSet<Target>;

#[derive(Clone, Debug, PartialEq, PartialOrd, Ord, Eq, Hash)]
pub struct Target {
    pub path: TargetPath,
    pub format: Format,
}

impl Target {
    pub fn from_annotation(anno: &Annotation) -> Result<Self, Error> {
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
    Path(Path),
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
    pub fn from_annotation(anno: &Annotation) -> Result<Self, Error> {
        let path = anno.target_path();

        // Absolute path
        if path.starts_with('/') {
            return Ok(Self::Path(path.into()));
        }

        // URL style path
        if path.contains("://") {
            let url = Url::parse(path)?;
            return Ok(Self::Url(url));
        }

        let path = anno.resolve_file(path.into())?;
        Ok(Self::Path(path))
    }

    pub async fn load(&self, spec_download_path: Option<&str>) -> Result<String, Error> {
        let contents = match self {
            Self::Url(url) => {
                let path = self.local(spec_download_path);
                tokio::fs::create_dir_all(path.parent().unwrap()).await?;
                let url = Self::canonical_url(url.as_str());
                duvet_core::http::get_cached_string(url, path).await?
            }
            Self::Path(path) => vfs::read_string(path).await?,
        };

        let mut contents = contents.to_string();

        // make sure the file has a newline
        if !contents.ends_with('\n') {
            contents.push('\n');
        }

        if contents.trim_start().starts_with("<!DOCTYPE html>") {
            return Err(anyhow::anyhow!(
                "target {self} returned HTML instead of plaintext"
            ));
        }

        Ok(contents)
    }

    pub fn local(&self, spec_download_path: Option<&str>) -> Path {
        match self {
            Self::Url(url) => {
                let mut path = if let Some(path_to_spec) = spec_download_path {
                    path_to_spec.into()
                } else {
                    std::env::current_dir().unwrap()
                };
                path.push("specs");
                let url = Self::canonical_url(url.as_str());
                let url = Url::parse(&url).unwrap();
                path.push(url.host_str().expect("url should have host"));
                path.extend(url.path_segments().expect("url should have path"));
                path.set_extension("txt");
                path.into()
            }
            Self::Path(path) => path.clone(),
        }
    }

    fn canonical_url(url: &str) -> String {
        // rewrite some of the IETF links for convenience
        if let Some(rfc) = url.strip_prefix("https://tools.ietf.org/rfc/") {
            let rfc = rfc.trim_end_matches(".txt").trim_end_matches(".html");
            return format!("https://www.rfc-editor.org/rfc/{rfc}.txt");
        }

        if let Some(rfc) = url.strip_prefix("https://datatracker.ietf.org/doc/html/rfc") {
            let rfc = rfc
                .trim_end_matches(".txt")
                .trim_end_matches(".html")
                .trim_end_matches('/');
            return format!("https://www.rfc-editor.org/rfc/rfc{rfc}.txt");
        }

        if url.starts_with("https://www.rfc-editor.org/rfc/") {
            let rfc = url.trim_end_matches(".txt").trim_end_matches(".html");
            return format!("{rfc}.txt");
        }

        url.to_owned()
    }
}

impl FromStr for TargetPath {
    type Err = Error;

    fn from_str(path: &str) -> Result<Self, Self::Err> {
        // URL style path
        if path.contains("://") {
            let url = Url::parse(path)?;
            return Ok(Self::Url(url));
        }

        Ok(Self::Path(path.into()))
    }
}
