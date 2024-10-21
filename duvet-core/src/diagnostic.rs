// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use crate::file::{Slice, SourceFile};
pub use miette::Report;
use miette::{Diagnostic, LabeledSpan};
use std::{error::Error as StdError, fmt, sync::Arc};

#[derive(Clone)]
pub struct Error(Arc<Report>);

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.0.source()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

pub trait IntoDiagnostic<T, E> {
    fn into_diagnostic(self) -> Result<T, Error>;
}

impl<T, E: 'static + std::error::Error + Send + Sync> IntoDiagnostic<T, E> for Result<T, E> {
    fn into_diagnostic(self) -> Result<T, Error> {
        miette::IntoDiagnostic::into_diagnostic(self).map_err(Error::from)
    }
}

impl IntoDiagnostic<(), ()> for Vec<Error> {
    fn into_diagnostic(self) -> Result<(), Error> {
        if self.is_empty() {
            Ok(())
        } else {
            Err(self.into())
        }
    }
}

impl Error {
    pub fn with_source_slice(
        &self,
        source_code: Slice<SourceFile>,
        label: impl AsRef<str>,
    ) -> Self {
        Report::new(WithSourceCode {
            error: self.clone(),
            source_code,
            label: label.as_ref().to_string(),
        })
        .into()
    }
}

impl Diagnostic for Error {
    fn code<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        self.0.code()
    }

    fn severity(&self) -> Option<miette::Severity> {
        self.0.severity()
    }

    fn help<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        self.0.help()
    }

    fn url<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        self.0.url()
    }

    fn labels<'a>(&'a self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + 'a>> {
        self.0.labels()
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        self.0.source_code()
    }

    fn related<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a dyn Diagnostic> + 'a>> {
        self.0.related()
    }

    fn diagnostic_source(&self) -> Option<&dyn Diagnostic> {
        self.0.diagnostic_source()
    }
}

impl From<Report> for Error {
    fn from(err: Report) -> Self {
        Self(Arc::new(err))
    }
}

impl From<&Error> for Error {
    fn from(error: &Error) -> Self {
        error.clone()
    }
}

impl From<Vec<Error>> for Error {
    fn from(err: Vec<Error>) -> Self {
        Set::from(err).into()
    }
}

impl From<Set> for Error {
    fn from(err: Set) -> Self {
        miette::IntoDiagnostic::into_diagnostic(Err::<(), _>(err))
            .unwrap_err()
            .into()
    }
}

struct WithSourceCode {
    error: Error,

    source_code: Slice<SourceFile>,

    label: String,
}

impl fmt::Display for WithSourceCode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.error.fmt(f)
    }
}

impl fmt::Debug for WithSourceCode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.error.fmt(f)
    }
}

impl StdError for WithSourceCode {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.error.source()
    }
}

impl Diagnostic for WithSourceCode {
    fn code<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        self.error.code()
    }

    fn severity(&self) -> Option<miette::Severity> {
        self.error.severity()
    }

    fn help<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        self.error.help()
    }

    fn url<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        self.error.url()
    }

    fn labels<'a>(&'a self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + 'a>> {
        let iter = core::iter::once(LabeledSpan::new_with_span(
            Some(self.label.clone()),
            self.source_code.range(),
        ));

        Some(if let Some(prev) = self.error.labels() {
            Box::new(prev.chain(iter))
        } else {
            Box::new(iter)
        })
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        Some(self.source_code.file())
    }

    fn related<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a dyn Diagnostic> + 'a>> {
        self.error.related()
    }

    fn diagnostic_source(&self) -> Option<&dyn Diagnostic> {
        self.error.diagnostic_source()
    }
}

#[derive(Diagnostic)]
pub struct Set {
    #[diagnostic_source]
    main: Error,
    #[related]
    errors: Arc<[Error]>,
}

impl From<Vec<Error>> for Set {
    fn from(errors: Vec<Error>) -> Self {
        let main = miette::miette!("encountered {} errors", errors.len()).into();
        let errors = Arc::from(errors);
        Self { main, errors }
    }
}

impl fmt::Display for Set {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for error in self.errors.iter() {
            writeln!(f, "{}", error)?;
        }
        Ok(())
    }
}

impl fmt::Debug for Set {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_list().entries(self.errors.iter()).finish()
    }
}

impl StdError for Set {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(&self.main)
    }
}

/*
impl Diagnostic for Set {
    fn related<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a dyn Diagnostic> + 'a>> {
        let iter = self.errors.iter().map(|e| e as &dyn Diagnostic);
        Some(Box::new(iter))
    }
}
*/