// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    annotation::Annotation,
    specification::{Format, Specification},
    Error, Result,
};
use core::{fmt, str::FromStr};
use duvet_core::{diagnostic::IntoDiagnostic, file::SourceFile, path::Path, progress, query};
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    sync::Arc,
};
use url::Url;

pub type SpecificationMap = Arc<HashMap<Arc<Target>, Arc<Specification>>>;

pub async fn query(targets: &TargetSet, spec_path: Path) -> Result<SpecificationMap> {
    let mut errors = vec![];

    let mut tasks = tokio::task::JoinSet::new();

    for target in targets.iter() {
        let target = target.clone();
        let task = to_specification(target.clone(), spec_path.clone());
        tasks.spawn(async move {
            let v = task.await?;
            <Result<_>>::Ok((target, v))
        });
    }

    let mut targets = HashMap::default();
    while let Some(res) = tasks.join_next().await {
        match res.into_diagnostic().and_then(|v| v.into_diagnostic()) {
            Ok((target, spec)) => {
                targets.insert(target.clone(), spec);
            }
            Err(err) => {
                errors.push(err);
            }
        }
    }

    if !errors.is_empty() {
        Err(errors.into())
    } else {
        Ok(Arc::new(targets))
    }
}

#[query]
pub async fn to_specification(
    target: Arc<Target>,
    spec_path: duvet_core::path::Path,
) -> Result<Arc<Specification>> {
    let contents = target.path.load(&spec_path).await?;
    let spec = target.format.parse(&contents)?;
    let spec = Arc::new(spec);
    Ok(spec)
}

pub type TargetSet = HashSet<Arc<Target>>;

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
    Path(Path),
}

impl fmt::Display for TargetPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Url(url) => url.fmt(f),
            Self::Path(path) => path.fmt(f),
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

        let path = anno.resolve_file(std::path::Path::new(path))?;
        Ok(Self::Path(path.into()))
    }

    pub async fn load(&self, spec_download_path: &Path) -> Result<SourceFile> {
        match self {
            Self::Url(url) => {
                let canonical_url = Self::canonical_url(url.as_str()).to_string();
                let path = self.local(spec_download_path);

                let progress = if !path.exists() {
                    Some(progress!("Downloading {url}"))
                } else {
                    None
                };

                let out = duvet_core::http::get_cached_string(canonical_url, path).await?;

                if let Some(progress) = progress {
                    progress!(progress, "Downloaded {url}");
                }

                Ok(out)
            }
            Self::Path(path) => duvet_core::vfs::read_string(path).await,
        }
    }

    pub fn local(&self, spec_download_path: &Path) -> Path {
        match self {
            Self::Url(url) => {
                let mut path = spec_download_path.to_path_buf();

                let mut push_url = |url: &Url| {
                    path.push(url.host_str().expect("url should have host"));
                    path.extend(url.path_segments().expect("url should have path"));
                };

                let canonical_url = Self::canonical_url(url.as_str());

                if matches!(canonical_url, Cow::Borrowed(_)) {
                    push_url(url)
                } else {
                    let url = Url::parse(&canonical_url).unwrap();
                    push_url(&url)
                }

                path.set_extension("txt");
                path.into()
            }
            Self::Path(path) => path.clone(),
        }
    }

    fn canonical_url(url: &str) -> Cow<'_, str> {
        fn remove_extension(url: &str) -> &str {
            url.trim_end_matches('/')
                .trim_end_matches(".txt")
                .trim_end_matches(".html")
                .trim_end_matches(".xml")
                .trim_end_matches(".pdf")
        }

        // rewrite some of the IETF links for convenience
        let patterns = [
            "https://www.rfc-editor.org/rfc/rfc",
            "http://www.rfc-editor.org/rfc/rfc",
            "https://tools.ietf.org/rfc/rfc",
            "http://tools.ietf.org/rfc/rfc",
            "https://datatracker.ietf.org/doc/html/rfc",
            "http://datatracker.ietf.org/doc/html/rfc",
        ];

        for pattern in patterns {
            if let Some(rfc) = url.strip_prefix(pattern) {
                let rfc = remove_extension(rfc);
                return format!("{}{rfc}.txt", patterns[0]).into();
            }
        }

        url.into()
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

        let path = Path::from(path);
        Ok(Self::Path(path))
    }
}

#[cfg(test)]
mod tests {
    use super::TargetPath;

    #[test]
    fn canonical_url_test() {
        let urls = [
            "https://www.rfc-editor.org/rfc/rfc9000.txt",
            "https://www.rfc-editor.org/rfc/rfc9000",
            "https://www.rfc-editor.org/rfc/rfc9000.html",
            "https://www.rfc-editor.org/rfc/rfc9000.xml",
            "https://www.rfc-editor.org/rfc/rfc9000.pdf",
            "https://datatracker.ietf.org/doc/html/rfc9000",
            "https://datatracker.ietf.org/doc/html/rfc9000.html",
            "https://datatracker.ietf.org/doc/html/rfc9000.txt",
            "https://tools.ietf.org/rfc/rfc9000",
            "https://tools.ietf.org/rfc/rfc9000.txt",
            "https://tools.ietf.org/rfc/rfc9000.html",
        ];

        let expected = urls[0];
        for url in urls {
            let canonical = TargetPath::canonical_url(url);
            assert_eq!(expected, canonical);
        }
    }
}
