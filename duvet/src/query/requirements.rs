// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    annotation::{Annotation,},
};
use std::{
    sync::Arc,
};

#[derive(Debug, Clone)]
pub enum RequirementMode {
    Global,
    Targeted(Vec<TargetedRequirement>),     // All values are spec paths
    Filtered(Vec<String>),                  // Quote text filters only (no section constraint)
    TargetedFiltered {                      // Both section and quote filters
        targets: Vec<TargetedRequirement>,
        quote_filters: Vec<String>,
    },
}

#[derive(Debug, Clone)]
pub struct TargetedRequirement {
    pub path: String,
    pub section: Option<String>, // Some("section") for "path#section", None for "path"
}

impl RequirementMode {
    /// Build a RequirementMode from optional section and quote filter values
    pub fn from_options(sections: &[String], quotes: &[String]) -> RequirementMode {
        let has_sections = !sections.is_empty();
        let has_quotes = !quotes.is_empty();

        match (has_sections, has_quotes) {
            (false, false) => RequirementMode::Global,
            (true, false) => {
                let targets = sections.iter().map(|s| Self::parse_targeted_requirement(s)).collect();
                RequirementMode::Targeted(targets)
            }
            (false, true) => {
                let quote_filters = quotes.iter().map(|q| q.to_lowercase()).collect();
                RequirementMode::Filtered(quote_filters)
            }
            (true, true) => {
                let targets = sections.iter().map(|s| Self::parse_targeted_requirement(s)).collect();
                let quote_filters = quotes.iter().map(|q| q.to_lowercase()).collect();
                RequirementMode::TargetedFiltered { targets, quote_filters }
            }
        }
    }

    pub fn in_scope(&self, annotation: &Arc<Annotation>,) -> bool {
        match self {
            RequirementMode::Global => true,
            RequirementMode::Targeted(targeted) => {
                Self::matches_target(targeted, annotation)
            },
            RequirementMode::Filtered(quote_filters) => {
                Self::matches_quote(quote_filters, annotation)
            },
            RequirementMode::TargetedFiltered { targets, quote_filters } => {
                Self::matches_target(targets, annotation) && Self::matches_quote(quote_filters, annotation)
            },
        }
    }

    fn matches_target(targeted: &[TargetedRequirement], annotation: &Arc<Annotation>) -> bool {
        targeted.iter().any(|target| {
            match &target.section {
                Some(section) => annotation.target == format!("{}#{}", target.path, section),
                None => annotation.target.starts_with(&target.path),
            }
        })
    }

    fn matches_quote(quote_filters: &[String], annotation: &Arc<Annotation>) -> bool {
        let quote_lower = annotation.quote.to_lowercase();
        quote_filters.iter().any(|filter| quote_lower.contains(filter))
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
