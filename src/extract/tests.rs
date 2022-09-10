// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use super::*;

macro_rules! snapshot_test {
    ($name:ident) => {
        #[test]
        fn $name() {
            let contents = include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/specs/",
                stringify!($name),
                ".txt"
            ));

            let spec = Format::Ietf.parse(contents).unwrap();
            let sections = extract_sections(&spec);

            let results: Vec<_> = sections
                .iter()
                .flat_map(|(section, features)| {
                    let id = &*section.id;
                    features.iter().map(move |feature| (id, feature))
                })
                .collect();

            insta::assert_debug_snapshot!(stringify!($name), results);
        }
    };
}

snapshot_test!(rfc9000);
snapshot_test!(rfc9001);
snapshot_test!(rfc9114);
