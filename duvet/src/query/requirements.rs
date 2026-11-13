// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::annotation::Annotation;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum RequirementMode {
    Global,
    Targeted(Vec<TargetedRequirement>), // All values are spec paths
    Filtered(Vec<String>),              // Quote text filters only (no section constraint)
    TargetedFiltered {
        // Both section and quote filters
        targets: Vec<TargetedRequirement>,
        quote_filters: Vec<String>,
    },
}

#[derive(Debug, Clone)]
pub struct TargetedRequirement {
    pub path: String,
    pub section: Option<String>, // Some("section") for "path#section", None for "path"
}

impl TargetedRequirement {
    /// Whether an annotation `target` (of the form `path` or `path#section`)
    /// matches this filter.
    ///
    /// The path component is compared *exactly*: the target is split on its
    /// first `#` and the path part must equal `self.path`. This avoids the raw
    /// `starts_with` over-match where `-s spec.md` would also match
    /// `spec.md.bak` (or `-s rfc2` match both `rfc2324` and `rfc200`). A filter
    /// that also carries a section (`path#section`) additionally requires the
    /// annotation's section to match exactly; a path-only filter matches any
    /// section under that path.
    fn matches(&self, target: &str) -> bool {
        let (path, section) = target
            .split_once('#')
            .map_or((target, None), |(p, s)| (p, Some(s)));

        if path != self.path {
            return false;
        }

        match &self.section {
            Some(want) => section == Some(want.as_str()),
            None => true,
        }
    }
}

impl RequirementMode {
    /// Build a RequirementMode from optional section and quote filter values
    pub fn from_options(sections: &[String], quotes: &[String]) -> RequirementMode {
        let has_sections = !sections.is_empty();
        let has_quotes = !quotes.is_empty();

        match (has_sections, has_quotes) {
            (false, false) => RequirementMode::Global,
            (true, false) => {
                let targets = sections
                    .iter()
                    .map(|s| Self::parse_targeted_requirement(s))
                    .collect();
                RequirementMode::Targeted(targets)
            }
            (false, true) => {
                let quote_filters = quotes.iter().map(|q| q.to_lowercase()).collect();
                RequirementMode::Filtered(quote_filters)
            }
            (true, true) => {
                let targets = sections
                    .iter()
                    .map(|s| Self::parse_targeted_requirement(s))
                    .collect();
                let quote_filters = quotes.iter().map(|q| q.to_lowercase()).collect();
                RequirementMode::TargetedFiltered {
                    targets,
                    quote_filters,
                }
            }
        }
    }

    pub fn in_scope(&self, annotation: &Arc<Annotation>) -> bool {
        match self {
            RequirementMode::Global => true,
            RequirementMode::Targeted(targeted) => Self::matches_target(targeted, annotation),
            RequirementMode::Filtered(quote_filters) => {
                Self::matches_quote(quote_filters, annotation)
            }
            RequirementMode::TargetedFiltered {
                targets,
                quote_filters,
            } => {
                Self::matches_target(targets, annotation)
                    && Self::matches_quote(quote_filters, annotation)
            }
        }
    }

    fn matches_target(targeted: &[TargetedRequirement], annotation: &Arc<Annotation>) -> bool {
        targeted
            .iter()
            .any(|target| target.matches(&annotation.target))
    }

    fn matches_quote(quote_filters: &[String], annotation: &Arc<Annotation>) -> bool {
        let quote_lower = annotation.quote.to_lowercase();
        quote_filters
            .iter()
            .any(|filter| quote_lower.contains(filter))
    }

    fn parse_targeted_requirement(value: &str) -> TargetedRequirement {
        if let Some((path, section)) = value.split_once('#') {
            TargetedRequirement {
                path: path.to_string(),
                section: Some(section.to_string()),
            }
        } else {
            TargetedRequirement {
                path: value.to_string(),
                section: None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(value: &str) -> TargetedRequirement {
        RequirementMode::parse_targeted_requirement(value)
    }

    #[test]
    fn path_only_filter_matches_exact_path_any_section() {
        let t = target("spec.md");
        assert!(t.matches("spec.md"));
        assert!(t.matches("spec.md#section-1"));
        assert!(t.matches("spec.md#other"));
    }

    #[test]
    fn path_only_filter_does_not_over_match_prefix() {
        // `-s spec.md` must not match a different file that merely shares
        // the prefix as a byte string.
        let t = target("spec.md");
        assert!(!t.matches("spec.md.bak"));
        assert!(!t.matches("spec.markdown"));
        assert!(!t.matches("spec.md.bak#section-1"));

        // numeric RFC ids are a real instance of the same hazard.
        let rfc = target("rfc2");
        assert!(!rfc.matches("rfc2324"));
        assert!(!rfc.matches("rfc200"));
        assert!(rfc.matches("rfc2"));
        assert!(rfc.matches("rfc2#s1"));
    }

    #[test]
    fn sectioned_filter_requires_exact_path_and_section() {
        let t = target("spec.md#section-1");
        assert!(t.matches("spec.md#section-1"));
        // Same path, wrong section.
        assert!(!t.matches("spec.md#section-2"));
        // Same path, no section.
        assert!(!t.matches("spec.md"));
        // Prefix-matching path must not slip through.
        assert!(!t.matches("spec.md.bak#section-1"));
    }
}
