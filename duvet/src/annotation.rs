// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    source::SourceFile,
    specification::Format,
    target::{SpecificationMap, Target, TargetSet},
    Error, Result,
};
use core::{
    fmt,
    ops::{self, Range},
    str::FromStr,
};
use duvet_core::{diagnostic::IntoDiagnostic, error, file::Slice, path::Path, query};
use serde::Serialize;
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

pub type AnnotationSet = Arc<BTreeSet<Arc<Annotation>>>;

pub type AnnotationReferenceMapKey = (Arc<Target>, Option<Arc<str>>);
pub type AnnotationReferenceMapValue = Arc<[AnnotationWithId]>;
pub type AnnotationReferenceMap =
    Arc<HashMap<AnnotationReferenceMapKey, AnnotationReferenceMapValue>>;

pub async fn specifications(
    annotations: AnnotationSet,
    spec_path: Option<Path>,
) -> Result<SpecificationMap> {
    let mut targets = TargetSet::new();
    for anno in annotations.iter() {
        targets.insert(anno.target()?);
    }

    let specs = crate::target::query(&targets, spec_path).await?;

    Ok(specs)
}

#[query]
pub async fn query(sources: Arc<HashSet<SourceFile>>) -> Result<AnnotationSet> {
    let mut errors = vec![];

    let mut tasks = tokio::task::JoinSet::new();

    for source in sources.iter() {
        let source = source.clone();
        tasks.spawn(async move { source.annotations().await });
    }

    let mut annotations = BTreeSet::default();
    while let Some(res) = tasks.join_next().await {
        match res.into_diagnostic() {
            Ok((local_annotations, local_errors)) => {
                annotations.extend(local_annotations.iter().cloned());
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

pub async fn reference_map(set: AnnotationSet) -> Result<AnnotationReferenceMap> {
    let mut map = HashMap::new();
    for (id, anno) in set.iter().enumerate() {
        let target = anno.target()?;
        let section = anno.target_section();
        let entry: &mut Vec<_> = map.entry((target, section)).or_default();
        entry.push(AnnotationWithId {
            id,
            annotation: anno.clone(),
        });
    }
    let map = map
        .into_iter()
        .map(|(key, value)| (key, value.into()))
        .collect();
    Ok(Arc::new(map))
}

#[derive(Clone, Debug, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct AnnotationWithId {
    pub id: usize,
    pub annotation: Arc<Annotation>,
}

impl ops::Deref for AnnotationWithId {
    type Target = Arc<Annotation>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.annotation
    }
}

#[derive(Debug, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct Annotation {
    pub source: Path,
    pub anno_line: usize,
    pub original_target: Slice,
    pub original_text: Slice,
    pub original_quote: Slice,
    pub anno: AnnotationType,
    pub target: String,
    pub quote: String,
    pub comment: String,
    pub manifest_dir: Path,
    pub level: AnnotationLevel,
    pub format: Format,
    pub tracking_issue: String,
    pub feature: String,
    pub tags: BTreeSet<String>,
}

impl Annotation {
    pub fn target(&self) -> Result<Arc<Target>> {
        Target::from_annotation(self).map(Arc::new)
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
                self.resolve_file(std::path::Path::new(target_path))
                    .unwrap()
                    .to_str()
                    .unwrap(),
            ),
        }
    }

    pub fn target_section(&self) -> Option<Arc<str>> {
        self.target_parts().1.map(Arc::from)
    }

    fn target_parts(&self) -> (&str, Option<&str>) {
        self.target
            .split_once('#')
            .map_or((&self.target, None), |(path, section)| {
                (path, Some(section))
            })
    }

    pub fn resolve_file(&self, file: &std::path::Path) -> Result<PathBuf> {
        // If we have the right path, just return it
        if file.is_file() {
            return Ok(file.to_path_buf());
        }

        let mut manifest_dir = self.manifest_dir.clone();
        loop {
            if manifest_dir.join(file).is_file() {
                return Ok(manifest_dir.join(file).into());
            }

            if !manifest_dir.pop() {
                break;
            }
        }

        Err(error!("Could not resolve file {:?}", file))
    }

    pub fn quote_range(&self, contents: &str) -> Option<(Range<usize>, crate::text::find::Kind)> {
        crate::text::find(&self.quote, contents)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub enum AnnotationType {
    Spec,
    Test,
    Citation,
    Exception,
    Todo,
    Implication,
}

impl Default for AnnotationType {
    fn default() -> Self {
        Self::Citation
    }
}

impl fmt::Display for AnnotationType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Self::Spec => "SPEC",
            Self::Test => "TEST",
            Self::Citation => "CITATION",
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
            "IMPLEMENTATION" | "implementation" | "CITATION" | "citation" => Ok(Self::Citation),
            "EXCEPTION" | "exception" => Ok(Self::Exception),
            "TODO" | "todo" => Ok(Self::Todo),
            "IMPLICATION" | "implication" => Ok(Self::Implication),
            _ => Err(error!(
                "Invalid annotation type {:?}, expected one of {:?}",
                v,
                [
                    "spec",
                    "test",
                    "implementation",
                    "exception",
                    "todo",
                    "implication"
                ]
            )),
        }
    }
}

// The order is in terms of priority from least to greatest
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Eq, Ord, Hash, Serialize)]
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
            _ => Err(error!(
                "Invalid annotation level {:?}, expected one of {:?}",
                v,
                ["AUTO", "MUST", "SHOULD", "MAY"]
            )),
        }
    }
}
