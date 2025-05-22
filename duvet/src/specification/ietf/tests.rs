// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use super::tokenizer::{self, tokens, Token};

const HIGHEST_KNOWN_ID: usize = 9663;

macro_rules! tests {
    ($(($name:ident, $id:expr)),* $(,)?) => {
        $(
            #[tokio::test]
            async fn $name() {
                test_range($id..$id + 100).await
            }
        )*
    }
}

tests!(
    (rfc_30xx, 3000),
    (rfc_31xx, 3100),
    (rfc_32xx, 3200),
    (rfc_33xx, 3300),
    (rfc_34xx, 3400),
    (rfc_35xx, 3500),
    (rfc_36xx, 3600),
    (rfc_37xx, 3700),
    (rfc_38xx, 3800),
    (rfc_39xx, 3900),
);

tests!(
    (rfc_40xx, 4000),
    (rfc_41xx, 4100),
    (rfc_42xx, 4200),
    (rfc_43xx, 4300),
    (rfc_44xx, 4400),
    (rfc_45xx, 4500),
    (rfc_46xx, 4600),
    (rfc_47xx, 4700),
    (rfc_48xx, 4800),
    (rfc_49xx, 4900),
);

tests!(
    (rfc_50xx, 5000),
    (rfc_51xx, 5100),
    (rfc_52xx, 5200),
    (rfc_53xx, 5300),
    (rfc_54xx, 5400),
    (rfc_55xx, 5500),
    (rfc_56xx, 5600),
    (rfc_57xx, 5700),
    (rfc_58xx, 5800),
    (rfc_59xx, 5900),
);

tests!(
    (rfc_60xx, 6000),
    (rfc_61xx, 6100),
    (rfc_62xx, 6200),
    (rfc_63xx, 6300),
    (rfc_64xx, 6400),
    (rfc_65xx, 6500),
    (rfc_66xx, 6600),
    (rfc_67xx, 6700),
    (rfc_68xx, 6800),
    (rfc_69xx, 6900),
);

tests!(
    (rfc_70xx, 7000),
    (rfc_71xx, 7100),
    (rfc_72xx, 7200),
    (rfc_73xx, 7300),
    (rfc_74xx, 7400),
    (rfc_75xx, 7500),
    (rfc_76xx, 7600),
    (rfc_77xx, 7700),
    (rfc_78xx, 7800),
    (rfc_79xx, 7900),
);

tests!(
    (rfc_80xx, 8000),
    (rfc_81xx, 8100),
    (rfc_82xx, 8200),
    (rfc_83xx, 8300),
    (rfc_84xx, 8400),
    (rfc_85xx, 8500),
    (rfc_86xx, 8600),
    (rfc_87xx, 8700),
    (rfc_88xx, 8800),
    (rfc_89xx, 8900),
);

tests!(
    (rfc_90xx, 9000),
    (rfc_91xx, 9100),
    (rfc_92xx, 9200),
    (rfc_93xx, 9300),
    (rfc_94xx, 9400),
    (rfc_95xx, 9500),
    (rfc_96xx, 9600),
    (rfc_97xx, 9700),
    (rfc_98xx, 9800),
    (rfc_99xx, 9900),
);

async fn test_range(range: core::ops::Range<usize>) {
    let mut saw_any = false;
    for rfc in range {
        saw_any |= test_rfc(rfc).await;
    }

    assert!(saw_any, "missing RFC download - run `cargo xtask test`");
}

async fn test_rfc(rfc: usize) -> bool {
    let etc = std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../target/www.rfc-editor.org"
    ));

    // these RFCs don't have any sections
    let empty = [
        3005, 3099, 3129, 3199, 3232, 3268, 3299, 3364, 3442, 3494, 3499, 3599, 3818,
    ];

    // these RFCs have empty section titles
    let empty_titles = [
        (3002, "4.1.1"),
        (3002, "4.1.2"),
        (3002, "4.1.3"),
        (3002, "4.2.1"),
        (3002, "4.2.2"),
        (3002, "4.3.1"),
        (3002, "4.3.2"),
        (3002, "4.3.3"),
        (3002, "4.3.4"),
        (3002, "4.4.1"),
        (3002, "4.4.2"),
        (3002, "4.4.3"),
        (3002, "4.5.1"),
        (3002, "4.5.2"),
        (3002, "4.5.3"),
        (3002, "4.5.4"),
        (3002, "4.5.5"),
        (3002, "4.5.6"),
        (3002, "4.6.1"),
        (3002, "4.6.2"),
        (3002, "4.7.1"),
        (3002, "4.7.2"),
        (3172, "A"),
        (3258, "A"),
        (3304, "2.1.1"),
        (3304, "2.1.2"),
        (3304, "2.1.3"),
        (3304, "2.1.4"),
        (3304, "2.1.5"),
        (3304, "2.1.6"),
        (3304, "2.1.7"),
        (3304, "2.1.8"),
        (3304, "2.1.9"),
        (3304, "2.1.10"),
        (3304, "2.1.11"),
        (3304, "2.1.12"),
        (3304, "2.2.1"),
        (3304, "2.2.2"),
        (3304, "2.2.3"),
        (3304, "2.2.4"),
        (3304, "2.2.5"),
        (3304, "2.2.6"),
        (3304, "2.2.7"),
        (3304, "2.2.8"),
        (3304, "2.2.9"),
        (3304, "2.2.10"),
        (3304, "2.2.11"),
        (3304, "2.3.1"),
        (3304, "2.3.2"),
        (3304, "2.3.3"),
        (3304, "2.3.4"),
        (3332, "A"),
        (3411, "A"),
        (3552, "A"),
        (4009, "B.1"),
        (4009, "B.2"),
        (4009, "B.3"),
        (4009, "B.4"),
        (4233, "A"),
        (4269, "B.1"),
        (4269, "B.2"),
        (4269, "B.3"),
        (4269, "B.4"),
        (4523, "A"),
        (4666, "A"),
        (4951, "A"),
        (4951, "B"),
        (4951, "C"),
    ];

    // RFCs that use numbers for appendix IDs
    let number_appendix_ids = [
        (3175, "1"),
        (3946, "1"),
        (3549, "1"),
        (4258, "1"),
        (4606, "1"),
    ];

    // RFCs that use roman numerals
    let roman_appendix_ids = [(5357, "I")];

    // RFCs that have indented sections
    let indented_sections = [(3003, "4")];

    // these RFCs skip/reorder sections
    let skips = [
        (1050, "11.1"),
        (1125, "11"),
        (3090, "10"),
        (3132, "4.1.2.4"),
        (3134, "1.2.31"),
        (3162, "2.3"),
        (3186, "2.3.5"),
        (3204, "3"),
        (3208, "9.7.3"),
        (3212, "10"),
        (3234, "1.4"),
        (3257, "8"),
        (3258, "7"),
        (3261, "F1"),
        (3261, "25"),
        (3284, "5.6"),
        (3296, "5.6"),
        (3326, "8"),
        (3326, "7"),
        (3326, "9"),
        (3331, "11.0"),
        (3348, "5"),
        (3383, "10"),
        (3428, "16"),
        (3475, "9"),
        (3509, "10"),
        (3568, "8"),
        (3608, "F1"),
        (3608, "6.4.2"),
        (3608, "7"),
        (3671, "3.13"),
        (3701, "5"),
        (3810, "5.1.7"),
        (3825, "6"),
        (3868, "7.3.4"),
        (3877, "3.3.5"),
        (3929, "10"),
        (4037, "16"),
        (4160, "4.6"),
        (4469, "9"),
        (4540, "3.5.16"),
        (4540, "5.3.17"),
        (4604, "8"),
        (4715, "10"),
        (4842, "18"),
        (4853, "6"),
        (5013, "10"),
        (5322, "7"),
        (5570, "5.1.5"),
        (5805, "4.4"),
        (5849, "6"),
        (5850, "5"),
        (5858, "8"),
        (5892, "8"),
        (6219, "11"),
        (6484, "1.5.4"),
        (6484, "5.4.8"),
        (6484, "5.6"),
        (6485, "9"),
        (6722, "5"),
        (6730, "12"),
    ];

    // these RFCs have duplicate sections
    let duplicate = [
        (3063, "6.2.1"),
        (3063, "A.5.2"),
        (3093, "3.2"),
        (3119, "11"),
        (3131, "10"),
        (3250, "3"),
        (3284, "5.4"),
        (3302, "6"),
        (3414, "12.1"),
        (3418, "6.1"),
        (3476, "8"),
        (3562, "3"),
        (3640, "A"),
        (3745, "6"),
        (3785, "6.1"),
        (3946, "1"), // uses both Appendix and Annex
        (4511, "C.2.1"),
        (4520, "A.8"),
        (4606, "1"), // uses both Appendix and Annex
        (4949, "7"),
        (5570, "2.4.2"),
        (5755, "10.2"),
    ];

    // _really_ messed up RFCs
    let janky_sections = [
        (3015, "A"),
        (3113, "8"),
        (3113, "9"),
        (3122, "A"),
        (3133, "1"),
        (3134, "1"),
        (3411, "A"),
        (3525, "A.1"),
        (3525, "I"),
        (3730, "1"), // Appendices repeat section counters
        (3730, "B"), // Appendices repeat section counters
        (5038, "B"), // Appendices repeat B and C
    ];

    println!("rfc{rfc}");

    // ignore any that we haven't snapshotted
    if HIGHEST_KNOWN_ID < rfc {
        return true;
    }

    let Ok(file) = duvet_core::vfs::read_string(etc.join(format!("rfc{rfc}.txt"))).await else {
        println!("  NOT FOUND");
        return false;
    };

    let tokens = tokens(&file).collect::<Vec<_>>();

    insta::assert_debug_snapshot!(format!("rfc{rfc}_tokens"), tokens);

    // don't do any checks right now
    if ERRORS.iter().any(|e| e.contains(&rfc)) {
        return true;
    }

    let mut sections = vec![];

    let mut prev_section = None;

    let mut check_section = |id: &str, title: &str, is_section: bool| {
        assert!(!id.is_empty());

        let prev = prev_section.replace(id.to_string());

        if janky_sections.contains(&(rfc, id)) {
            return;
        }

        assert_eq!(empty_titles.contains(&(rfc, id)), title.is_empty());

        let Some(prev) = prev else {
            if is_section {
                assert!(["1", "1.0"].contains(&id));
            }
            return;
        };

        if *prev == *id {
            assert!(duplicate.contains(&(rfc, id)), "duplicate section: {id:?}");
            return;
        }

        let is_ok = tokenizer::section_id_monotonic(&prev, id);

        let key = &(rfc, id);
        let expected = !(skips.contains(key)
            || indented_sections.contains(key)
            || number_appendix_ids.contains(key)
            || roman_appendix_ids.contains(key));

        assert_eq!(
            is_ok, expected,
            "unexpected section number: prev={prev:?} current={id:?}"
        );
    };

    let mut line = 1;
    for token in tokens {
        // make sure we don't drop any lines
        assert_eq!(line, token.line());
        line = token.line() + 1;

        match &token {
            Token::Section { id, title, .. } => {
                println!("  SECTION(id={id:?} title={title:?})");

                check_section(id, title, true);

                sections.push(token);
            }
            Token::Appendix { id, title, .. } => {
                println!(" APPENDIX(id={id:?} title={title:?})");

                check_section(id, title, false);

                sections.push(token);
            }
            Token::NamedSection { title, .. } => {
                println!("  SECTION(title={title:?})");
                // TODO
            }
            Token::Break { .. } => {
                // TODO
            }
            Token::Content { .. } => {
                // TODO
            }
            Token::Header { .. } => {
                // TODO
            }
        }
    }

    assert_eq!(
        sections.is_empty(),
        empty.contains(&rfc),
        "RFC sections is empty"
    );

    true
}

// these currently have parsing errors
static ERRORS: &[&[usize]] = &[
    &[
        19, 70, 77, 98, 107, 155, 172, 194, 199, 230, 240, 254, 271, 293, 329, 330, 331, 332, 333,
        354,
        // TODO gap
    ],
    &[
        768, 778, 782, 783, 787, 789, 799, 800, 802, 803, 810, 869, 876, 887, 891, 892, 896, 899,
        904, 911, 914, 994, 995, 999, 1001, 1002, 1005, 1014, 1035, 1038, 1045, 1076, 1099, 1123,
        1138, 1142, 1148, 1163, 1180, 1190, 1195, 1199, 1244, 1245, 1246,
        // TODO gap
    ],
    &[
        3064, // The first sections is `1.0.Introduction`
        3502, // This starts on 6.3.11
        3877, // The sections embed sequence diagrams
    ],
    &[
        5054, // this has a section with a title with lots of spaces
        5165, // this section has poorly formatted sections
    ],
    &[
        6503, // this embeds messages into the section
        6504, // this embeds messages into the section
        6917, // this embeds messages into the section
    ],
    &[
        7058, // This RFC embeds sequence diagrams in the sections
    ],
    &[
        9592, // This RFC embeds another RFC in the appendix, which fails the monotonic check
    ],
];
