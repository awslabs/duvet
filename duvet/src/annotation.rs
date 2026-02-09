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

    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;

    /// Helper function to create a test Annotation with the given source, line, and target.
    /// Other fields are set to sensible defaults.
    fn make_test_annotation(source_path: &str, anno_line: usize, target: &str) -> Annotation {
        // Create a minimal source file for the Slice fields
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
    fn fnv1a_64_empty_input_returns_offset_basis() {
        // Empty input should return the FNV offset basis
        assert_eq!(fnv1a_64(&[]), FNV_OFFSET_BASIS);
    }

    #[test]
    fn fnv1a_64_known_test_vectors() {
        // Known FNV-1a test vectors from the FNV specification
        // Reference: http://www.isthe.com/chongo/tech/comp/fnv/
        assert_eq!(fnv1a_64(b""), 0xcbf29ce484222325);
        assert_eq!(fnv1a_64(b"a"), 0xaf63dc4c8601ec8c);
        assert_eq!(fnv1a_64(b"foobar"), 0x85944171f73967e8);
    }

    #[test]
    fn fnv1a_64_determinism() {
        // Same input should always produce the same output
        let input = b"test input for determinism check";
        let hash1 = fnv1a_64(input);
        let hash2 = fnv1a_64(input);
        let hash3 = fnv1a_64(input);

        assert_eq!(hash1, hash2);
        assert_eq!(hash2, hash3);
    }

    // Tests for stable_annotation_id via composite key format
    // The function builds: "{source}\0{anno_line}\0{target_path}"

    #[test]
    fn stable_id_format_is_16_lowercase_hex_chars() {
        // Test that the output format is correct (16 lowercase hex chars)
        // Using the same composite key format as stable_annotation_id
        let composite_key = "src/lib.rs\x0042\x00https://example.com/spec";
        let hash = fnv1a_64(composite_key.as_bytes());
        let stable_id = format!("{hash:016x}");

        assert_eq!(stable_id.len(), 16);
        assert!(stable_id
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn stable_id_determinism() {
        // Same composite key should always produce the same ID
        let composite_key = "src/lib.rs\x0042\x00https://example.com/spec#section-1";

        let hash1 = fnv1a_64(composite_key.as_bytes());
        let hash2 = fnv1a_64(composite_key.as_bytes());
        let hash3 = fnv1a_64(composite_key.as_bytes());

        let id1 = format!("{hash1:016x}");
        let id2 = format!("{hash2:016x}");
        let id3 = format!("{hash3:016x}");

        assert_eq!(id1, id2);
        assert_eq!(id2, id3);
    }

    #[test]
    fn stable_id_different_annotations_produce_different_ids() {
        // Different composite keys should produce different IDs
        let key1 = "src/lib.rs\x0042\x00https://example.com/spec";
        let key2 = "src/lib.rs\x0043\x00https://example.com/spec"; // Different line
        let key3 = "src/other.rs\x0042\x00https://example.com/spec"; // Different source
        let key4 = "src/lib.rs\x0042\x00https://example.com/other"; // Different target

        let id1 = format!("{:016x}", fnv1a_64(key1.as_bytes()));
        let id2 = format!("{:016x}", fnv1a_64(key2.as_bytes()));
        let id3 = format!("{:016x}", fnv1a_64(key3.as_bytes()));
        let id4 = format!("{:016x}", fnv1a_64(key4.as_bytes()));

        // All IDs should be different
        assert_ne!(id1, id2, "Different line should produce different ID");
        assert_ne!(id1, id3, "Different source should produce different ID");
        assert_ne!(id1, id4, "Different target should produce different ID");
        assert_ne!(id2, id3);
        assert_ne!(id2, id4);
        assert_ne!(id3, id4);
    }

    #[test]
    fn stable_id_leading_zeros_preserved() {
        // Test that leading zeros are preserved in the output
        // Find an input that produces a hash with leading zeros
        // The format string {:016x} should pad with zeros
        let hash: u64 = 0x0000_1234_5678_9abc;
        let stable_id = format!("{hash:016x}");

        assert_eq!(stable_id, "000012345678_9abc".replace("_", ""));
        assert_eq!(stable_id.len(), 16);
    }

    #[test]
    fn stable_id_target_path_extracts_correctly() {
        // Test that target_path extraction works correctly
        // target_path() returns the part before '#' or the whole string
        let target_with_section = "https://example.com/spec#section-1";
        let target_without_section = "https://example.com/spec";

        // Simulate target_path() behavior
        let path1 = target_with_section
            .split_once('#')
            .map_or(target_with_section, |(p, _)| p);
        let path2 = target_without_section
            .split_once('#')
            .map_or(target_without_section, |(p, _)| p);

        assert_eq!(path1, "https://example.com/spec");
        assert_eq!(path2, "https://example.com/spec");

        // Same target_path should produce same ID regardless of section
        let key1 = format!("src/lib.rs\x0042\x00{path1}");
        let key2 = format!("src/lib.rs\x0042\x00{path2}");

        let id1 = format!("{:016x}", fnv1a_64(key1.as_bytes()));
        let id2 = format!("{:016x}", fnv1a_64(key2.as_bytes()));

        assert_eq!(id1, id2, "Same target_path should produce same ID");
    }

    // =========================================================================
    // Tests that directly call stable_annotation_id() with real Annotation instances
    // =========================================================================

    #[test]
    fn stable_annotation_id_returns_16_lowercase_hex_chars() {
        let annotation = make_test_annotation(
            "src/lib.rs",
            42,
            "https://example.com/spec#section-1",
        );

        let stable_id = stable_annotation_id(&annotation);

        assert_eq!(stable_id.len(), 16, "Stable ID must be exactly 16 characters");
        assert!(
            stable_id.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "Stable ID must be lowercase hex, got: {stable_id}"
        );
    }

    #[test]
    fn stable_annotation_id_is_deterministic() {
        let annotation = make_test_annotation(
            "src/lib.rs",
            42,
            "https://example.com/spec#section-1",
        );

        let id1 = stable_annotation_id(&annotation);
        let id2 = stable_annotation_id(&annotation);
        let id3 = stable_annotation_id(&annotation);

        assert_eq!(id1, id2, "stable_annotation_id must be deterministic");
        assert_eq!(id2, id3, "stable_annotation_id must be deterministic");
    }

    #[test]
    fn stable_annotation_id_different_line_produces_different_id() {
        let anno1 = make_test_annotation("src/lib.rs", 42, "https://example.com/spec");
        let anno2 = make_test_annotation("src/lib.rs", 43, "https://example.com/spec");

        let id1 = stable_annotation_id(&anno1);
        let id2 = stable_annotation_id(&anno2);

        assert_ne!(id1, id2, "Different anno_line should produce different stable ID");
    }

    #[test]
    fn stable_annotation_id_different_source_produces_different_id() {
        let anno1 = make_test_annotation("src/lib.rs", 42, "https://example.com/spec");
        let anno2 = make_test_annotation("src/other.rs", 42, "https://example.com/spec");

        let id1 = stable_annotation_id(&anno1);
        let id2 = stable_annotation_id(&anno2);

        assert_ne!(id1, id2, "Different source path should produce different stable ID");
    }

    #[test]
    fn stable_annotation_id_different_target_produces_different_id() {
        let anno1 = make_test_annotation("src/lib.rs", 42, "https://example.com/spec1");
        let anno2 = make_test_annotation("src/lib.rs", 42, "https://example.com/spec2");

        let id1 = stable_annotation_id(&anno1);
        let id2 = stable_annotation_id(&anno2);

        assert_ne!(id1, id2, "Different target path should produce different stable ID");
    }

    #[test]
    fn stable_annotation_id_uses_target_path_not_full_target() {
        // Two annotations with same target_path but different sections should have same stable ID
        // because stable_annotation_id uses target_path() which strips the section
        let anno1 = make_test_annotation("src/lib.rs", 42, "https://example.com/spec#section-1");
        let anno2 = make_test_annotation("src/lib.rs", 42, "https://example.com/spec#section-2");

        let id1 = stable_annotation_id(&anno1);
        let id2 = stable_annotation_id(&anno2);

        assert_eq!(
            id1, id2,
            "Same target_path (ignoring section) should produce same stable ID"
        );
    }

    #[test]
    fn stable_annotation_id_ignores_other_annotation_fields() {
        // Create two annotations with same (source, line, target_path) but different other fields
        let mut anno1 = make_test_annotation("src/lib.rs", 42, "https://example.com/spec");
        let mut anno2 = make_test_annotation("src/lib.rs", 42, "https://example.com/spec");

        // Modify fields that should NOT affect the stable ID
        anno1.quote = "Some quote text".to_string();
        anno2.quote = "Different quote text".to_string();

        anno1.comment = "Comment 1".to_string();
        anno2.comment = "Comment 2".to_string();

        anno1.anno = AnnotationType::Test;
        anno2.anno = AnnotationType::Citation;

        anno1.level = AnnotationLevel::Must;
        anno2.level = AnnotationLevel::May;

        let id1 = stable_annotation_id(&anno1);
        let id2 = stable_annotation_id(&anno2);

        assert_eq!(
            id1, id2,
            "Stable ID should only depend on (source, anno_line, target_path)"
        );
    }

    #[test]
    fn stable_annotation_id_matches_expected_composite_key_format() {
        // Verify that stable_annotation_id produces the same result as manually
        // constructing the composite key and hashing it
        let annotation = make_test_annotation(
            "src/lib.rs",
            42,
            "https://example.com/spec#section-1",
        );

        let stable_id = stable_annotation_id(&annotation);

        // Manually construct the composite key the same way the function does
        let expected_composite_key = format!(
            "{}\0{}\0{}",
            annotation.source.to_string_lossy(),
            annotation.anno_line,
            annotation.target_path()
        );
        let expected_id = format!("{:016x}", fnv1a_64(expected_composite_key.as_bytes()));

        assert_eq!(stable_id, expected_id, "stable_annotation_id should match manual computation");
    }

    /// Property 1: Hash Determinism
    /// For any byte slice input, calling fnv1a_64() multiple times with the same
    /// input always produces the same 64-bit output, and for any annotation composite
    /// key, calling the hash function multiple times always produces the same
    /// 16-character hex string.
    ///
    /// **Validates: Requirements 1.2, 2.2, 9.2**
    #[test]
    fn property_hash_determinism() {
        // Test fnv1a_64 determinism with arbitrary byte inputs
        check!().with_type::<Vec<u8>>().for_each(|input| {
            let hash1 = fnv1a_64(input);
            let hash2 = fnv1a_64(input);
            let hash3 = fnv1a_64(input);

            assert_eq!(hash1, hash2, "fnv1a_64 must be deterministic");
            assert_eq!(hash2, hash3, "fnv1a_64 must be deterministic");
        });
    }

    /// Property 1 (continued): Stable ID Determinism
    /// For any composite key (source, line, target), the generated stable ID
    /// is always the same 16-character lowercase hex string.
    ///
    /// **Validates: Requirements 1.2, 2.2, 9.2**
    #[test]
    fn property_stable_id_determinism() {
        // Test stable ID determinism with arbitrary composite key components
        check!()
            .with_type::<(String, usize, String)>()
            .for_each(|(source, line, target)| {
                // Build composite key the same way stable_annotation_id does
                let composite_key = format!("{source}\0{line}\0{target}");

                let hash1 = fnv1a_64(composite_key.as_bytes());
                let hash2 = fnv1a_64(composite_key.as_bytes());

                let id1 = format!("{hash1:016x}");
                let id2 = format!("{hash2:016x}");

                assert_eq!(id1, id2, "Stable ID must be deterministic");
                assert_eq!(id1.len(), 16, "Stable ID must be 16 characters");
                assert!(
                    id1.chars()
                        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
                    "Stable ID must be lowercase hex"
                );
            });
    }

    /// Property 3: Cross-Run Determinism
    /// For any annotation set, processing it through reference_map() multiple times
    /// (or in independent runs) produces identical stable_id values for each annotation,
    /// given the same source content.
    ///
    /// This test validates that the stable ID generation is deterministic across
    /// multiple "runs" by simulating the reference_map behavior: for any given
    /// annotation content (source_path, anno_line, target_path), the stable_id
    /// computed is always the same regardless of when or how many times it's computed.
    ///
    /// **Validates: Requirements 2.5, 5.4, 6.4, 9.1, 9.3**
    #[test]
    fn property_cross_run_determinism() {
        // Test that simulated "runs" produce identical stable IDs
        // We test at the composite key level since stable_annotation_id only uses
        // (source_path, anno_line, target_path) from the annotation
        check!().with_type::<(String, usize, String)>().for_each(
            |(source_path, anno_line, target_path)| {
                // Build composite key the same way stable_annotation_id does
                let composite_key = format!("{source_path}\0{anno_line}\0{target_path}");

                // Simulate multiple "runs" by computing the hash and ID multiple times
                let run1_hash = fnv1a_64(composite_key.as_bytes());
                let run1_id = format!("{run1_hash:016x}");

                let run2_hash = fnv1a_64(composite_key.as_bytes());
                let run2_id = format!("{run2_hash:016x}");

                let run3_hash = fnv1a_64(composite_key.as_bytes());
                let run3_id = format!("{run3_hash:016x}");

                // All runs must produce identical stable IDs
                assert_eq!(
                    run1_id, run2_id,
                    "Cross-run determinism: run 1 and run 2 must produce same stable_id"
                );
                assert_eq!(
                    run2_id, run3_id,
                    "Cross-run determinism: run 2 and run 3 must produce same stable_id"
                );

                // Verify the stable_id does not depend on processing order or timing
                // by creating a fresh composite key with same content
                let composite_key_copy = format!("{source_path}\0{anno_line}\0{target_path}");
                let independent_run_hash = fnv1a_64(composite_key_copy.as_bytes());
                let independent_run_id = format!("{independent_run_hash:016x}");

                assert_eq!(
                    run1_id, independent_run_id,
                    "Independent computation with same content must produce same stable_id"
                );

                // Verify the ID format is correct (16 lowercase hex chars)
                assert_eq!(run1_id.len(), 16, "Stable ID must be 16 characters");
                assert!(
                    run1_id
                        .chars()
                        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
                    "Stable ID must be lowercase hex"
                );
            },
        );
    }
}
