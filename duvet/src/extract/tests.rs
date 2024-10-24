// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use super::*;

macro_rules! snapshot_test {
    ($name:ident) => {
        snapshot_test!($name, ".txt");
    };
    ($name:ident, $ext:expr) => {
        #[test]
        fn $name() {
            let contents = include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../specs/",
                stringify!($name),
                $ext,
            ));
            let path = concat!(stringify!($name), $ext);
            let contents = duvet_core::file::SourceFile::new(path, contents).unwrap();

            let spec = Format::Auto.parse(&contents).unwrap();
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
snapshot_test!(esdk_client, ".md");
snapshot_test!(esdk_decrypt, ".md");
snapshot_test!(esdk_encrypt, ".md");
snapshot_test!(esdk_streaming, ".md");
