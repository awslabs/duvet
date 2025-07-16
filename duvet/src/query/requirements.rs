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
}

#[derive(Debug, Clone)]
pub struct TargetedRequirement {
    pub path: String,
    pub section: Option<String>, // Some("section") for "path#section", None for "path"
}

impl RequirementMode {
    /// Parse requirement strings from CLI or config into RequirementMode
    pub fn parse_requirements(values: &[String]) -> RequirementMode {
        if values.is_empty() {
            return RequirementMode::Global;
        }

        // All should be spec paths
        let mut targeted = Vec::new();
        for value in values {
            targeted.push(Self::parse_targeted_requirement(value));
        }
        RequirementMode::Targeted(targeted)
    }

    pub fn in_scope(&self, annotation: &Arc<Annotation>,) -> bool {
        match self {
            RequirementMode::Global => true,
            RequirementMode::Targeted(targeted) => {
                targeted.iter().any(|target| {
                    match &target.section {
                        Some(section) => annotation.target == format!("{}#{}", target.path, section),
                        None => annotation.target.starts_with(&target.path),
                    }
                })
            },
            
        }
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
