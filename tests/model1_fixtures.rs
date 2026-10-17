//! The frozen model-1 ground truths (R5.5, docs/MODEL_1.md), checked offline.
//!
//! `tools/model1.py verify` checks the same hashes without cargo; this holds the
//! files to what the Rust parser accepts and to the facts the document states
//! about them, so a freeze that wrote something the comparator would refuse
//! cannot be committed.

use std::collections::BTreeSet;

use acadsharp_rs_tests::model1::{self, Record, Value, header_type};

const SAVES: [&str; 7] = [
    "acadsharp-ac1014",
    "acadsharp-ac1015",
    "acadsharp-ac1018",
    "acadsharp-ac1021",
    "acadsharp-ac1024",
    "acadsharp-ac1027",
    "acadsharp-ac1032",
];
const TWINS: [&str; 6] = [
    "dxf-sample-ac1015-ascii",
    "dxf-sample-ac1018-ascii",
    "dxf-sample-ac1021-ascii",
    "dxf-sample-ac1024-ascii",
    "dxf-sample-ac1027-ascii",
    "dxf-sample-ac1032-ascii",
];

#[test]
fn every_frozen_stream_is_the_file_the_manifest_froze_and_parses_strictly() {
    let files = model1::frozen_files().unwrap();
    assert_eq!(files.len(), SAVES.len() + TWINS.len());
    for s in SAVES {
        let r = model1::load_frozen("oracle", s).unwrap();
        let f = files
            .iter()
            .find(|f| f.engine == "oracle" && f.id == s)
            .unwrap();
        assert_eq!(r.len(), f.records, "{s}");
    }
    for t in TWINS {
        let r = model1::load_frozen("ezdxf", t).unwrap();
        let f = files
            .iter()
            .find(|f| f.engine == "ezdxf" && f.id == t)
            .unwrap();
        assert_eq!(r.len(), f.records, "{t}");
    }
}

#[test]
fn the_oracle_enumerates_all_of_cadheaders_249_variables_on_every_save() {
    let mut first: Option<BTreeSet<String>> = None;
    for s in SAVES {
        let names: BTreeSet<String> = model1::load_frozen("oracle", s)
            .unwrap()
            .iter()
            .map(|r| r.key().to_string())
            .collect();
        assert_eq!(names.len(), 249, "{s}");
        if let Some(f) = &first {
            assert_eq!(
                f, &names,
                "{s}: the set of names does not depend on the file"
            );
        }
        first = Some(names);
    }
}

#[test]
fn a_value_agrees_with_the_type_its_group_code_gives() {
    // The type table is by group code, and a few codes are checked here by
    // name: they are the ones where the library's own type disagrees (a char, a
    // bool that is really an int, a short that is really a bool).
    let want = [
        ("$ORTHOMODE", "int"),
        ("$DGNFRAME", "int"),
        ("$PSTYLEMODE", "bool"),
        ("$LWDISPLAY", "bool"),
        ("$CAMERADISPLAY", "bool"),
        ("$HANDSEED", "string"),
        ("$LIMMIN", "point2"),
        ("$EXTMIN", "point3"),
        ("$TDCREATE", "double"),
        ("$CECOLOR", "int"),
    ];
    let r = model1::load_frozen("oracle", "acadsharp-ac1032").unwrap();
    for (name, ty) in want {
        let Record::Header(h) = r.iter().find(|r| r.key() == name).unwrap();
        assert_eq!(h.value.type_name(), ty, "{name}");
    }
    assert_eq!(header_type("$ORTHOMODE", &[70]).unwrap(), "int");
}

#[test]
fn the_ezdxf_streams_hold_no_handle_valued_variable() {
    // Handles stay out of the stream. The names that were left out are listed in
    // fixtures/model1/ezdxf/<id>.ezdxf.model1.skipped.
    for t in TWINS {
        let skipped = std::fs::read_to_string(
            acadsharp_rs_tests::fixtures::fixture_root()
                .join("model1/ezdxf")
                .join(format!("{t}.ezdxf.model1.skipped")),
        )
        .unwrap();
        let r = model1::load_frozen("ezdxf", t).unwrap();
        for name in skipped.lines() {
            assert!(
                r.iter().all(|x| x.key() != name),
                "{t}: {name} is both skipped and written"
            );
        }
    }
}

#[test]
fn the_text_of_a_string_variable_has_its_line_endings_normalised() {
    for s in SAVES {
        for r in model1::load_frozen("oracle", s).unwrap() {
            let Record::Header(h) = r;
            if let Value::Str(v) = h.value {
                assert!(!v.contains('\r'), "{s} {}", h.name);
            }
        }
    }
}
