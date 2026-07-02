// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use super::ReportResult;
use crate::Result;
use duvet_core::path::Path;
use std::io::Write;

#[rustfmt::skip] // it gets really confused with macros that generate macros
macro_rules! writer {
    ($writer:ident) => {
        #[allow(unused_macros)]
        macro_rules! w {
            ($arg: expr) => {
                write!($writer, "{}", $arg)?
            };
        }
    };
}

pub fn report(report: &ReportResult, file: &Path) -> Result {
    let mut out = vec![];
    report_writer(report, &mut out)?;
    duvet_core::vfs::write(file, out)
}

pub fn report_writer<Output: Write>(report: &ReportResult, output: &mut Output) -> Result {
    writer!(output);

    w!("<!DOCTYPE html>\n");
    w!("<html>");
    w!("<head>");
    w!(r#"<meta charset="utf-8">"#);
    w!("<title>");
    w!("Compliance Coverage Report");
    w!("</title>");

    w!(r#"<script type="application/json" id=result>"#);
    super::json::report_writer(report, output)?;
    w!("</script>");
    w!("</head>");
    w!("<body>");
    w!("<div id=root></div>");
    w!(r#"<script>"#);
    w!(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/www/public/script.js"
    )));
    w!(r#"</script>"#);
    w!("</body>");
    w!("</html>");
    Ok(())
}
