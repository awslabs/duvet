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
    spec_path: Path,
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
        let stable_id = stable_annotation_id(anno);
        let entry: &mut Vec<_> = map.entry((target, section)).or_default();
        entry.push(AnnotationWithId {
            id,
            stable_id,
            annotation: anno.clone(),
        });
    }
    let map = map
        .into_iter()
        .map(|(key, value)| (key, value.into()))
        .collect();
    Ok(Arc::new(map))
}
/// FNV-1a 64-bit hash function.
///
/// Deterministic, no dependencies, sufficient for expected annotation counts.
/// Uses the standard FNV-1a constants for 64-bit hashing.
fn fnv1a_64(data: &[u8]) -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET_BASIS;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Generates a stable, deterministic ID for an annotation based on its content.
///
/// The ID is derived from a composite key of (source_path, anno_line, target_path)
/// using FNV-1a 64-bit hashing, formatted as a 16-character hex string.
///
/// # Arguments
/// * `annotation` - The annotation to generate an ID for
///
/// # Returns
/// A 16-character lowercase hex string that uniquely identifies the annotation
pub fn stable_annotation_id(annotation: &Annotation) -> String {
    use std::io::Write;
    let mut buf = Vec::new();
    let _ = write!(
        buf,
        "{}\0{}\0{}",
        annotation.source.to_string_lossy(),
        annotation.anno_line,
        annotation.target_path(),
    );
    format!("{:016x}", fnv1a_64(&buf))
}

#[derive(Clone, Debug, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct AnnotationWithId {
    pub id: usize,
    pub stable_id: String,
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
    pub blob_link: Option<Arc<str>>,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub enum AnnotationType {
    Spec,
    Test,
    #[default]
    Citation,
    Exception,
    Todo,
    Implication,
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
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd, Eq, Ord, Hash, Serialize)]
#[cfg_attr(test, derive(bolero::TypeGenerator))]
pub enum AnnotationLevel {
    #[default]
    Auto,
    May,
    Should,
    Must,
}

impl AnnotationLevel {
    pub const LEVELS: [Self; 4] = [Self::Auto, Self::May, Self::Should, Self::Must];
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

#[cfg(test)]
mod tests {
    use super::{fnv1a_64, stable_annotation_id, Annotation, AnnotationLevel, AnnotationType};
    use crate::specification::Format;
    use bolero::check;
    use duvet_core::{file::SourceFile, path::Path};
    use std::collections::BTreeSet;

    fn make_test_annotation(source_path: &str, anno_line: usize, target: &str) -> Annotation {
        let source_file = SourceFile::new(Path::from(source_path), "test content").unwrap();
        let slice = source_file.substr_range(0..4).unwrap();

        Annotation {
            source: Path::from(source_path),
            anno_line,
            original_target: slice.clone(),
            original_text: slice.clone(),
            original_quote: slice,
            anno: AnnotationType::Citation,
            target: target.to_string(),
            quote: String::new(),
            comment: String::new(),
            manifest_dir: Path::from("."),
            level: AnnotationLevel::Auto,
            format: Format::Auto,
            tracking_issue: String::new(),
            feature: String::new(),
            tags: BTreeSet::new(),
            blob_link: None,
        }
    }

    #[test]
    fn fnv1a_64_known_test_vectors() {
        // Reference: http://www.isthe.com/chongo/tech/comp/fnv/
        assert_eq!(fnv1a_64(b""), 0xcbf29ce484222325);
        assert_eq!(fnv1a_64(b"a"), 0xaf63dc4c8601ec8c);
        assert_eq!(fnv1a_64(b"foobar"), 0x85944171f73967e8);
    }

    #[test]
    fn stable_annotation_id_returns_16_lowercase_hex_chars() {
        let id = stable_annotation_id(&make_test_annotation(
            "src/lib.rs",
            42,
            "https://example.com/spec#section-1",
        ));

        assert_eq!(id.len(), 16);
        assert!(id
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn stable_annotation_id_varies_with_each_key_component() {
        let base = make_test_annotation("src/lib.rs", 42, "https://example.com/spec");
        let diff_line = make_test_annotation("src/lib.rs", 43, "https://example.com/spec");
        let diff_source = make_test_annotation("src/other.rs", 42, "https://example.com/spec");
        let diff_target = make_test_annotation("src/lib.rs", 42, "https://example.com/other");

        let id_base = stable_annotation_id(&base);
        let id_line = stable_annotation_id(&diff_line);
        let id_source = stable_annotation_id(&diff_source);
        let id_target = stable_annotation_id(&diff_target);

        assert_ne!(id_base, id_line, "different line");
        assert_ne!(id_base, id_source, "different source");
        assert_ne!(id_base, id_target, "different target");
    }

    #[test]
    fn stable_annotation_id_uses_target_path_not_section() {
        let anno1 = make_test_annotation("src/lib.rs", 42, "https://example.com/spec#section-1");
        let anno2 = make_test_annotation("src/lib.rs", 42, "https://example.com/spec#section-2");

        assert_eq!(stable_annotation_id(&anno1), stable_annotation_id(&anno2));
    }

    #[test]
    fn stable_annotation_id_ignores_non_key_fields() {
        let mut anno1 = make_test_annotation("src/lib.rs", 42, "https://example.com/spec");
        let mut anno2 = make_test_annotation("src/lib.rs", 42, "https://example.com/spec");

        anno1.quote = "quote A".to_string();
        anno1.comment = "comment A".to_string();
        anno1.anno = AnnotationType::Test;
        anno1.level = AnnotationLevel::Must;

        anno2.quote = "quote B".to_string();
        anno2.comment = "comment B".to_string();
        anno2.anno = AnnotationType::Citation;
        anno2.level = AnnotationLevel::May;

        assert_eq!(stable_annotation_id(&anno1), stable_annotation_id(&anno2));
    }

    /// Property: fnv1a_64 is deterministic for arbitrary inputs.
    #[test]
    fn property_hash_determinism() {
        check!().with_type::<Vec<u8>>().for_each(|input| {
            assert_eq!(fnv1a_64(input), fnv1a_64(input));
        });
    }
}
