// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    annotation::AnnotationType,
    manifest::{Requirement, Source},
};
use duvet_core::{glob::Glob, path::Path};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Schema {
    #[serde(default)]
    pub compliance: Compliance,
}

impl Schema {
    pub fn load_sources(&self, sources: &mut Vec<Source>, file: &Path) {
        let root: Path = file.parent().unwrap().into();

        for source in self.compliance.sources.iter() {
            sources.push(Source {
                pattern: source.pattern.clone(),
                comment_style: (&source.comment_style).into(),
                default_type: source.default_type.into(),
                root: root.clone(),
            });
        }
    }

    pub fn load_requirements(&self, requirements: &mut Vec<Requirement>, file: &Path) {
        let root: Path = file.parent().unwrap().into();

        for requirement in self.compliance.requirements.iter() {
            requirements.push(Requirement {
                pattern: requirement.pattern.clone(),
                root: root.clone(),
            });
        }
    }
}

pub use compliance::Compliance;

pub mod compliance {
    use super::*;

    #[derive(Clone, Debug, Default, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Compliance {
        #[serde(default, rename = "source")]
        pub sources: Arc<[Source]>,

        #[serde(default, rename = "requirement")]
        pub requirements: Arc<[Requirement]>,
    }

    #[derive(Clone, Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Source {
        pub pattern: Glob,
        #[serde(default, rename = "comment-style")]
        pub comment_style: CommentStyle,
        #[serde(rename = "type", default)]
        pub default_type: DefaultType,
    }

    #[derive(Clone, Copy, Debug, Default, Deserialize)]
    #[serde(rename = "lowercase")]
    pub enum DefaultType {
        #[default]
        Implementation,
        Spec,
        Test,
        Exception,
        Todo,
        Implication,
    }

    impl From<DefaultType> for AnnotationType {
        fn from(value: DefaultType) -> Self {
            match value {
                DefaultType::Implementation => Self::Implementation,
                DefaultType::Spec => Self::Spec,
                DefaultType::Test => Self::Test,
                DefaultType::Todo => Self::Todo,
                DefaultType::Exception => Self::Exception,
                DefaultType::Implication => Self::Implication,
            }
        }
    }

    #[derive(Clone, Debug, Deserialize, Hash)]
    #[serde(deny_unknown_fields)]
    pub struct CommentStyle {
        pub meta: Arc<str>,
        pub content: Arc<str>,
    }

    impl Default for CommentStyle {
        fn default() -> Self {
            Self {
                meta: "//=".into(),
                content: "//#".into(),
            }
        }
    }

    impl From<&CommentStyle> for crate::comment::Pattern {
        fn from(value: &CommentStyle) -> Self {
            Self {
                meta: value.meta.clone(),
                content: value.content.clone(),
            }
        }
    }

    #[derive(Clone, Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Requirement {
        pub pattern: Glob,
    }
}
