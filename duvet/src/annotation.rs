// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    specification::Format,
    target::{Target, TargetSet},
    Error,
};
use anyhow::anyhow;
use core::{fmt, ops::Range, str::FromStr};
use duvet_core::{diagnostic::IntoDiagnostic, path::Path, query, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

pub type AnnotationSet = BTreeSet<Annotation>;

pub type AnnotationReferenceMap<'a> =
    HashMap<(Target, Option<&'a str>), Vec<(usize, &'a Annotation)>>;

pub trait AnnotationSetExt {
    fn targets(&self) -> Result<TargetSet, Error>;
    fn reference_map(&self) -> Result<AnnotationReferenceMap, Error>;
}

impl AnnotationSetExt for AnnotationSet {
    fn targets(&self) -> Result<TargetSet, Error> {
        let mut set = TargetSet::new();
        for anno in self.iter() {
            set.insert(anno.target()?);
        }
        Ok(set)
    }

    fn reference_map(&self) -> Result<AnnotationReferenceMap, Error> {
        let mut map = AnnotationReferenceMap::new();
        for (id, anno) in self.iter().enumerate() {
            let target = anno.target()?;
            let section = anno.target_section();
            map.entry((target, section)).or_default().push((id, anno));
        }
        Ok(map)
    }
}

#[query]
pub async fn query() -> Result<Arc<AnnotationSet>> {
    let mut errors = vec![];

    // TODO use try_join after fixing `duvet_core::Query` concurrency issues
    // let (sources, requirements) =
    //     try_join!(crate::manifest::sources(), crate::manifest::requirements())?;
    let sources = crate::manifest::sources().await?;
    let requirements = crate::manifest::requirements().await?;

    let mut tasks = tokio::task::JoinSet::new();

    for source in sources.iter().chain(requirements.iter()) {
        let source = source.clone();
        tasks.spawn(async move { source.annotations().await });
    }

    let mut annotations = AnnotationSet::default();
    while let Some(res) = tasks.join_next().await {
        match res.into_diagnostic() {
            Ok((local_annotations, local_errors)) => {
                annotations.extend(local_annotations);
                errors.extend(local_errors.iter().cloned());
            }
            Err(err) => {
                errors.push(err);
            }
        }
    }

    if !errors.is_empty() {
        Err(errors.into())
    } else {
        Ok(Arc::new(annotations))
    }
}

#[derive(Debug, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct Annotation {
    pub source: Path,
    pub anno_line: u32,
    pub anno_column: u32,
    pub anno: AnnotationType,
    pub target: String,
    pub quote: String,
    pub comment: String,
    pub level: AnnotationLevel,
    pub format: Format,
    pub tracking_issue: String,
    pub feature: String,
    pub tags: BTreeSet<String>,
}

impl Annotation {
    pub fn relative_source(&self) -> &std::path::Path {
        let cwd = duvet_core::env::current_dir().unwrap();
        self.source.strip_prefix(&cwd).unwrap_or(&self.source)
    }

    pub fn target(&self) -> Result<Target, Error> {
        Target::from_annotation(self)
    }

    pub fn target_path(&self) -> &str {
        self.target_parts().0
    }

    // The JSON file needs to index the specification
    // to the same path that the annotation targets will have
    pub fn resolve_target_path(&self) -> String {
        let target_path = self.target_path();
        match target_path.contains("://") {
            // A URL should not be changed.
            true => target_path.into(),
            // A file path needs to match
            false => String::from(
                self.resolve_file(target_path.into())
                    .unwrap()
                    .to_str()
                    .unwrap(),
            ),
        }
    }

    pub fn target_section(&self) -> Option<&str> {
        self.target_parts().1
    }

    fn target_parts(&self) -> (&str, Option<&str>) {
        self.target
            .split_once('#')
            .map_or((&self.target, None), |(path, section)| {
                (path, Some(section))
            })
    }

    pub fn resolve_file(&self, file: Path) -> Result<Path, Error> {
        // If we have the right path, just return it
        if file.is_file() {
            return Ok(file);
        }

        let mut manifest_dir = self.source.as_ref();
        while let Some(dir) = manifest_dir.parent() {
            manifest_dir = dir;

            if manifest_dir.join(&file).is_file() {
                return Ok(manifest_dir.join(file).into());
            }
        }

        Err(anyhow!(format!("Could not resolve file {:?}", file)))
    }

    pub fn quote_range(&self, contents: &str) -> Option<Range<usize>> {
        crate::text::find(&self.quote, contents)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub enum AnnotationType {
    Implementation,
    Spec,
    Test,
    Exception,
    Todo,
    Implication,
}

impl Default for AnnotationType {
    fn default() -> Self {
        Self::Implementation
    }
}

impl fmt::Display for AnnotationType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Self::Implementation => "IMPLEMENTATION",
            Self::Spec => "SPEC",
            Self::Test => "TEST",
            Self::Exception => "EXCEPTION",
            Self::Todo => "TODO",
            Self::Implication => "IMPLICATION",
        })
    }
}

impl FromStr for AnnotationType {
    type Err = Error;

    fn from_str(v: &str) -> Result<Self, Self::Err> {
        match v {
            "SPEC" | "spec" => Ok(Self::Spec),
            "TEST" | "test" => Ok(Self::Test),
            "CITATION" | "citation" | "IMPLEMENTATION" | "implementation" => {
                Ok(Self::Implementation)
            }
            "EXCEPTION" | "exception" => Ok(Self::Exception),
            "TODO" | "todo" => Ok(Self::Todo),
            "IMPLICATION" | "implication" => Ok(Self::Implication),
            _ => Err(anyhow!(format!("Invalid annotation type {:?}", v))),
        }
    }
}

// The order is in terms of priority from least to greatest
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Eq, Ord, Hash, Deserialize, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum AnnotationLevel {
    Auto,
    May,
    Should,
    Must,
}

impl AnnotationLevel {
    pub const LEVELS: [Self; 4] = [Self::Auto, Self::May, Self::Should, Self::Must];
}

impl Default for AnnotationLevel {
    fn default() -> Self {
        Self::Auto
    }
}

impl fmt::Display for AnnotationLevel {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Self::Auto => "AUTO",
            Self::May => "MAY",
            Self::Should => "SHOULD",
            Self::Must => "MUST",
        })
    }
}

impl FromStr for AnnotationLevel {
    type Err = Error;

    fn from_str(v: &str) -> Result<Self, Self::Err> {
        match v {
            "AUTO" => Ok(Self::Auto),
            "MUST" => Ok(Self::Must),
            "SHOULD" => Ok(Self::Should),
            "MAY" => Ok(Self::May),
            _ => Err(anyhow!(format!("Invalid annotation level {:?}", v))),
        }
    }
}
