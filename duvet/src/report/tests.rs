// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod test {
    use super::super::{Report, TargetReport};
    use crate::specification::Specification;
    use crate::Arguments;
    use clap::Parser;
    use std::sync::Arc;

    // Helper function to extract Report from Arguments
    fn extract_report(args: Arguments) -> Report {
        match args {
            Arguments::Report(report) => report,
            _ => panic!("Expected Report variant"),
        }
    }

    #[test]
    fn test_basic_report_cli() {
        // Test basic report command parsing
        let args = vec!["duvet", "report"];
        let parsed = Arguments::try_parse_from(args).unwrap();
        let _report = extract_report(parsed);
        // Report should parse successfully without the old flags
    }

    #[test]
    fn test_report_with_output_options() {
        // Test report command with output options
        let args = vec![
            "duvet", "report", 
            "--json", "output.json",
            "--html", "output.html",
            "--ci"
        ];
        let parsed = Arguments::try_parse_from(args).unwrap();
        let _report = extract_report(parsed);
        // Report should parse successfully with remaining flags
    }

    #[test]
    fn test_empty_target_report() {
        // Test that empty target reports work correctly
        let target_report = TargetReport {
            references: vec![],
            specification: Arc::new(Specification::default()),
            statuses: Default::default(),
        };
        
        // Should be able to create target report with simplified structure
        assert_eq!(target_report.references.len(), 0);
    }
}
