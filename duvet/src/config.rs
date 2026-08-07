// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{annotation::AnnotationType, extract::Extraction, Result};
use duvet_core::{path::Path, vfs};
use std::{collections::BTreeSet, sync::Arc};

pub mod schema;

/// A set of claim forms admitted to coexist, e.g. `{exception, test}`.
pub type FormSet = BTreeSet<AnnotationType>;

/// A family of allowed form sets. `None` means the shipped default family for
/// its axis; `Some(vec![])` forbids all mixing.
pub type FormFamily = Option<Vec<FormSet>>;

/// The canonical user-facing name of a claim form.
///
/// //= design/duplicates/spec.md#annotation-model
/// //# Every user-facing surface this
/// //# specification defines — configuration keys, report sections,
/// //# failure messages — MUST emit the name `implementation` and MUST
/// //# NOT emit the name `citation`.
pub fn form_name(form: AnnotationType) -> &'static str {
    match form {
        AnnotationType::Spec => "spec",
        AnnotationType::Test => "test",
        AnnotationType::Citation => "implementation",
        AnnotationType::Exception => "exception",
        AnnotationType::Todo => "todo",
        AnnotationType::Implication => "implication",
    }
}

/// The free claim forms: no checkable obligation, and a claim of one exempts
/// its requirement from the test obligation (spec §1.5).
pub fn is_free_form(form: AnnotationType) -> bool {
    matches!(
        form,
        AnnotationType::Exception | AnnotationType::Implication | AnnotationType::Todo
    )
}

/// Render a form set as `a+b+c` in the canonical vocabulary.
pub fn form_set_name_of(forms: &FormSet) -> String {
    forms
        .iter()
        .map(|form| form_name(*form))
        .collect::<Vec<_>>()
        .join("+")
}

/// Policy for the duplicates check
/// (design/duplicates/spec.md §4). All policy is checked-in configuration;
/// the command line never changes a verdict.
#[derive(Clone, Debug, Default)]
pub struct DuplicatesPolicy {
    pub claims: ClaimsPolicy,
    pub targets: TargetsPolicy,
}

impl DuplicatesPolicy {
    /// Whether any target-axis gate is configured (fan-in bounds or an
    /// explicit form family). When false, the target axis gates only on the
    /// shipped form-family default.
    pub fn targets_gates_configured(&self) -> bool {
        self.targets.count.is_some()
            || self.targets.sections.is_some()
            || self.targets.types.is_some()
    }

    /// The effective policy, one line per setting, with values equal to the
    /// shipped default labeled `(default)`. This is the discovery loop of
    /// design/duplicates/decisions.md Decision 9: see every value, write the
    /// numbers you want into config, and the run goes silent about everything
    /// you have accepted.
    pub fn describe(&self) -> String {
        fn family(types: &FormFamily, default_meaning: &str) -> (String, &'static str) {
            match types {
                None => (default_meaning.to_string(), " (default)"),
                Some(sets) => {
                    let sets = sets
                        .iter()
                        .map(|set| format!("{:?}", form_set_name_of(set)))
                        .collect::<Vec<_>>()
                        .join(", ");
                    (format!("[{sets}]"), "")
                }
            }
        }
        fn bound(value: Option<u64>) -> (String, &'static str) {
            match value {
                None => ("unlimited".to_string(), " (default)"),
                Some(value) => (value.to_string(), ""),
            }
        }

        let mut out = String::new();
        let mut line = |key: &str, value: String, label: &str| {
            out.push_str(&format!("  {key} = {value}{label}\n"));
        };

        let default_mark = |cap: u32| if cap == 1 { " (default)" } else { "" };
        line(
            "claims.test",
            self.claims.test.to_string(),
            default_mark(self.claims.test),
        );
        line(
            "claims.implementation",
            self.claims.implementation.to_string(),
            default_mark(self.claims.implementation),
        );
        let (value, label) = family(&self.claims.types, "no free form shares a claim");
        line("claims.types", value, label);
        let (value, label) = bound(self.targets.count);
        line("targets.count", value, label);
        let (value, label) = bound(self.targets.sections);
        line("targets.sections", value, label);
        let (value, label) = family(
            &self.targets.types,
            "any combination except test+implementation",
        );
        line("targets.types", value, label);
        out
    }
}

/// Predicates over claim classes (spec §4.2.1).
#[derive(Clone, Debug)]
pub struct ClaimsPolicy {
    /// Cap for `test` duplicate sets. Default 1.
    pub test: u32,
    /// Cap for `implementation` duplicate sets. Default 1.
    pub implementation: u32,
    /// The claim-form family (spec §2.4). `None` = default family: every form
    /// set containing no free form.
    pub types: FormFamily,
}

impl Default for ClaimsPolicy {
    fn default() -> Self {
        Self {
            test: 1,
            implementation: 1,
            types: None,
        }
    }
}

impl ClaimsPolicy {
    /// The multiplicity cap for a claim form.
    ///
    /// //= design/duplicates/spec.md#caps
    /// //# The caps of
    /// //# `spec`, `todo`, `exception`, and `implication` are fixed at 1 and
    /// //# MUST NOT be configurable.
    pub fn cap(&self, form: AnnotationType) -> u32 {
        match form {
            AnnotationType::Test => self.test,
            AnnotationType::Citation => self.implementation,
            _ => 1,
        }
    }

    /// Whether the claim-form family admits this set of coexisting forms.
    ///
    /// //= design/duplicates/spec.md#exclusivity
    /// //# The default claim-form family admits exactly the form sets
    /// //# containing no free form
    pub fn admits(&self, forms: &FormSet) -> bool {
        match &self.types {
            Some(family) => family.iter().any(|allowed| forms.is_subset(allowed)),
            None => !forms.iter().any(|form| is_free_form(*form)),
        }
    }
}

/// Predicates over target classes (spec §4.2.2). Bounds are opt-in:
/// unlimited when unset.
#[derive(Clone, Debug, Default)]
pub struct TargetsPolicy {
    /// Fan-in bound: maximum annotations in one target class.
    pub count: Option<u64>,
    /// Maximum distinct sections claimed by one target class.
    pub sections: Option<u64>,
    /// The target-form family (spec §3.3). `None` = default family: every
    /// form set that does not contain both `test` and `implementation`.
    pub types: FormFamily,
}

impl TargetsPolicy {
    /// Whether the target-form family admits this set of coexisting forms.
    ///
    /// //= design/duplicates/spec.md#type-combinations
    /// //# The default family admits every form set that does not contain
    /// //# both `test` and `implementation`
    pub fn admits(&self, forms: &FormSet) -> bool {
        match &self.types {
            Some(family) => family.iter().any(|allowed| forms.is_subset(allowed)),
            None => {
                !(forms.contains(&AnnotationType::Test)
                    && forms.contains(&AnnotationType::Citation))
            }
        }
    }
}

/// Parse a claim form name as configuration input. `citation` is accepted as
/// an alias for `implementation`; the canonical name is what we emit.
pub fn parse_form_name(name: &str) -> Result<AnnotationType> {
    match name.trim() {
        "spec" => Ok(AnnotationType::Spec),
        "test" => Ok(AnnotationType::Test),
        "implementation" | "citation" => Ok(AnnotationType::Citation),
        "exception" => Ok(AnnotationType::Exception),
        "todo" => Ok(AnnotationType::Todo),
        "implication" => Ok(AnnotationType::Implication),
        other => Err(duvet_core::error!(
            "unknown claim form {other:?} in duplicates configuration; \
             expected one of: spec, test, implementation, exception, todo, implication"
        )),
    }
}

/// Parse a `+`-joined form-set family, e.g. `["exception+test"]`.
pub fn parse_form_family(entries: &[String]) -> Result<Vec<FormSet>> {
    let mut family = Vec::with_capacity(entries.len());
    for entry in entries {
        let mut set = FormSet::new();
        for name in entry.split('+') {
            set.insert(parse_form_name(name)?);
        }
        if set.is_empty() {
            return Err(duvet_core::error!(
                "empty form set in duplicates configuration"
            ));
        }
        family.push(set);
    }
    Ok(family)
}

#[derive(Clone, Debug)]
pub struct Config {
    pub sources: Vec<Source>,
    pub requirements: Vec<Requirement>,
    pub specifications: Vec<Specification>,
    pub report: Report,
    pub duplicates: DuplicatesPolicy,
    pub requirements_path: Path,
    pub download_path: Path,
}

impl Config {
    pub async fn load_specifications(&self) -> Result<usize> {
        let download_path = &self.download_path;
        let requirements_path = &self.requirements_path;

        for spec in &self.specifications {
            Extraction {
                download_path,
                base_path: Some(download_path),
                target: spec.target.clone(),
                out: requirements_path,
                extension: "toml",
                // don't log to reduce noise
                log: false,
            }
            .exec()
            .await?;
        }

        Ok(self.specifications.len())
    }
}

#[derive(Clone, Debug)]
pub struct Source {
    pub pattern: String,
    pub root: Path,
    pub comment_style: crate::comment::Pattern,
    pub default_type: crate::annotation::AnnotationType,
    pub blob_link: Option<Arc<str>>,
}

#[derive(Clone, Debug)]
pub struct Requirement {
    pub pattern: String,
    pub root: Path,
}

#[derive(Clone, Debug)]
pub struct Report {
    pub html: HtmlReport,
    pub json: JsonReport,
    pub snapshot: SnapshotReport,
}

#[derive(Clone, Debug)]
pub struct HtmlReport {
    pub enabled: bool,
    pub path: Path,
    pub blob_link: Option<Arc<str>>,
    pub issue_link: Option<Arc<str>>,
}

impl HtmlReport {
    pub fn path(&self) -> Option<&Path> {
        self.enabled.then_some(&self.path)
    }
}

#[derive(Clone, Debug)]
pub struct JsonReport {
    pub enabled: bool,
    pub path: Path,
}

impl JsonReport {
    pub fn path(&self) -> Option<&Path> {
        self.enabled.then_some(&self.path)
    }
}

#[derive(Clone, Debug)]
pub struct SnapshotReport {
    pub enabled: bool,
    pub path: Path,
}

impl SnapshotReport {
    pub fn path(&self) -> Option<&Path> {
        self.enabled.then_some(&self.path)
    }
}

#[derive(Clone, Debug)]
pub struct Specification {
    pub target: Arc<crate::target::Target>,
}

pub async fn load(path: Path, root: Path) -> Result<Arc<Config>> {
    let file = vfs::read_string(path.clone()).await?;
    let schema: Arc<schema::Schema> = file.as_toml().await?;

    let mut sources = vec![];
    let mut requirements = vec![];
    let mut specifications = vec![];

    schema.load_sources(&mut sources, &root)?;
    schema.load_requirements(&mut requirements, &root)?;
    schema.load_specifications(&mut specifications, &root)?;

    let requirements_path = schema.requirements_path(&path, &root);
    let download_path = schema.download_path(&path, &root);
    let report = schema.report(&path, &root);
    let duplicates = schema.duplicates()?;

    Ok(Arc::new(Config {
        sources,
        requirements,
        specifications,
        requirements_path,
        download_path,
        report,
        duplicates,
    }))
}

pub async fn default_path_and_root() -> Option<(Path, Path)> {
    let root = duvet_core::env::current_dir().ok()?;
    let path = root.join(".duvet").join("config.toml");

    // check to see if it exists
    let _ = vfs::read_metadata(&path).await.ok()?;

    Some((path, root))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forms(names: &[&str]) -> FormSet {
        names
            .iter()
            .map(|name| parse_form_name(name).unwrap())
            .collect()
    }

    /// spec §2.4: the default claim-form family admits exactly the form sets
    /// containing no free form.
    #[test]
    fn default_claims_family_excludes_free_forms() {
        let policy = ClaimsPolicy::default();
        assert!(policy.admits(&forms(&["test", "implementation"])));
        assert!(policy.admits(&forms(&["test"])));
        for free in ["exception", "implication", "todo"] {
            assert!(!policy.admits(&forms(&[free, "test"])), "{free}+test");
            assert!(
                !policy.admits(&forms(&[free, "implementation"])),
                "{free}+implementation"
            );
        }
    }

    /// spec §3.3: the default target-form family admits every form set that
    /// does not contain both `test` and `implementation`.
    #[test]
    fn default_targets_family_splits_test_from_implementation() {
        let policy = TargetsPolicy::default();
        assert!(!policy.admits(&forms(&["test", "implementation"])));
        assert!(!policy.admits(&forms(&["test", "implementation", "exception"])));
        assert!(policy.admits(&forms(&["exception", "test"])));
        assert!(policy.admits(&forms(&["implementation", "implication"])));
    }

    /// spec §3.3: a configured family admits exactly subsets of its members;
    /// `types = []` forbids all mixing.
    #[test]
    fn configured_family_is_subset_closed() {
        let family = parse_form_family(&["exception+test".to_string()]).unwrap();
        let policy = TargetsPolicy {
            types: Some(family),
            ..Default::default()
        };
        assert!(policy.admits(&forms(&["exception", "test"])));
        assert!(policy.admits(&forms(&["exception"])), "subset is admitted");
        assert!(!policy.admits(&forms(&["exception", "todo"])));

        let empty = TargetsPolicy {
            types: Some(vec![]),
            ..Default::default()
        };
        assert!(!empty.admits(&forms(&["exception", "test"])));
    }

    /// spec §2.2: free-form caps are fixed at 1 regardless of the priced caps.
    #[test]
    fn free_form_caps_are_fixed() {
        let policy = ClaimsPolicy {
            test: 5,
            implementation: 5,
            types: None,
        };
        assert_eq!(policy.cap(AnnotationType::Test), 5);
        assert_eq!(policy.cap(AnnotationType::Citation), 5);
        for fixed in [
            AnnotationType::Spec,
            AnnotationType::Exception,
            AnnotationType::Implication,
            AnnotationType::Todo,
        ] {
            assert_eq!(policy.cap(fixed), 1, "{fixed:?}");
        }
    }

    /// spec §1.1 / §4.2.3: `citation` accepted on input, `implementation`
    /// emitted, unknown names rejected.
    #[test]
    fn form_name_round_trip() {
        assert_eq!(
            parse_form_name("citation").unwrap(),
            AnnotationType::Citation
        );
        assert_eq!(form_name(AnnotationType::Citation), "implementation");
        assert!(parse_form_name("citations").is_err());
        assert!(parse_form_family(&["".to_string()]).is_err());
    }
}
