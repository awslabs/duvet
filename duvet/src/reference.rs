// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    annotation::{AnnotationReferenceMap, AnnotationWithId},
    specification::Specification,
    target::{SpecificationMap, Target},
    Result,
};
use duvet_core::{
    diagnostic::{Error, IntoDiagnostic},
    error,
    file::Slice,
};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, PartialOrd, Eq, Ord)]
pub struct Reference {
    pub target: Arc<Target>,
    pub annotation: AnnotationWithId,
    pub text: Slice,
}

impl Reference {
    pub fn line(&self) -> usize {
        self.text.line()
    }

    pub fn start(&self) -> usize {
        self.text.range().start
    }

    pub fn end(&self) -> usize {
        self.text.range().end
    }
}

pub async fn query(
    reference_map: AnnotationReferenceMap,
    specs: SpecificationMap,
) -> Result<Arc<[Reference]>> {
    let mut errors = vec![];

    let mut tasks = tokio::task::JoinSet::new();

    for ((target, section_id), annotations) in reference_map.iter() {
        let spec = specs.get(target).expect("missing spec");
        let task = build_references(
            target.clone(),
            spec.clone(),
            section_id.clone(),
            annotations.clone(),
        );
        tasks.spawn(task);
    }

    let mut references = vec![];
    while let Some(res) = tasks.join_next().await {
        match res.into_diagnostic() {
            Ok((task_references, task_errors)) => {
                references.extend(task_references.iter().cloned());
                errors.extend(task_errors.iter().cloned());
            }
            Err(err) => {
                errors.push(err);
            }
        }
    }

    if !errors.is_empty() {
        Err(errors.into())
    } else {
        Ok(references.into())
    }
}

pub async fn build_references(
    target: Arc<Target>,
    spec: Arc<Specification>,
    section_id: Option<Arc<str>>,
    annotations: Arc<[AnnotationWithId]>,
) -> (Arc<[Reference]>, Arc<[Error]>) {
    let mut errors = vec![];
    let mut references = vec![];

    let Some(section_id) = section_id else {
        // TODO return a warning that referring to a spec without a section doesn't make sense
        return (references.into(), errors.into());
    };

    let Some(section) = spec.section(&section_id) else {
        for annotation in annotations.iter() {
            let mut error = error!("missing section {section_id:?} in {}", target.path);
            error = error.with_source_slice(annotation.original_target.clone(), "referenced here");
            // TODO try to make a suggestion if there's something that's close
            errors.push(error);
        }
        return (references.into(), errors.into());
    };

    let contents = section.view();

    for annotation in annotations.iter() {
        if annotation.quote.is_empty() {
            // empty quotes don't count towards coverage but are still
            // references
            let text = section.full_title.clone();
            references.push(Reference {
                target: target.clone(),
                text,
                annotation: annotation.clone(),
            });
            continue;
        }

        if let Some((range, kind)) = annotation.quote_range(&contents) {
            if kind.is_fuzzy() {
                // TODO
                //warnings.push(warn!(""));
            }

            for text in contents.ranges(range) {
                references.push(Reference {
                    target: target.clone(),
                    text,
                    annotation: annotation.clone(),
                });
            }
        } else {
            let mut error = error!(
                "could not find text in section {section_id:?} of {}",
                target.path
            );
            if let Some(original_text) = section.original_text() {
                error = error.with_source_slice(annotation.original_quote.clone(), "text here");

                // TODO print out the section text in the `help` with line numbers
                let _ = original_text;
            } else {
                error =
                    error.with_source_slice(annotation.original_quote.clone(), "section is empty");
            }
            errors.push(error);
        }
    }

    // TODO upgrade levels whenever they overlap

    (references.into(), errors.into())
}
