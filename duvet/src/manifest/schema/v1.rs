use duvet_core::{glob::Glob, path::Path};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Schema {
    pub version: String, // TODO make this a hard-coded value to v1
    pub compliance: Compliance,
}

pub use compliance::Compliance;

pub mod compliance {
    use super::*;

    #[derive(Clone, Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Compliance {
        #[serde(rename = "source")]
        pub sources: Arc<[Source]>,

        #[serde(rename = "requirement")]
        pub requirements: Arc<[Requirement]>,

        #[serde(default)]
        pub spec: Spec,
    }

    #[derive(Clone, Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Source {
        pub patterns: Glob,
        #[serde(rename = "comment-style")]
        pub comment_style: CommentStyle,
        #[serde(rename = "type", default = "default_type")]
        pub default_type: Arc<str>,
    }

    fn default_type() -> Arc<str> {
        "implementation".into()
    }

    #[derive(Clone, Debug, Deserialize, Hash)]
    #[serde(deny_unknown_fields)]
    pub struct CommentStyle {
        pub meta: Arc<str>,
        pub content: Arc<str>,
    }

    #[derive(Clone, Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Requirement {
        pub patterns: Glob,
    }

    #[derive(Clone, Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Spec {
        #[serde(default)]
        pub markdown: Arc<[Markdown]>,
        #[serde(default)]
        pub ietf: Arc<[Ietf]>,
        #[serde(default = "default_spec_dir")]
        pub directory: Path,
    }

    impl Default for Spec {
        fn default() -> Self {
            Self {
                markdown: Default::default(),
                ietf: Default::default(),
                directory: default_spec_dir(),
            }
        }
    }

    fn default_spec_dir() -> Path {
        Path::from("specs")
    }

    #[derive(Clone, Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Markdown {
        pub patterns: Glob,
    }

    #[derive(Clone, Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Ietf {
        pub id: Arc<str>,
        pub title: Option<Arc<str>>,
        pub url: Arc<str>,
        #[serde(default)]
        pub aliases: Arc<[Arc<str>]>,
    }
}
