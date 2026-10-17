//! `parity/census_exceptions.toml`: what a reader is allowed to miss, and why (R3.4).

use acadsharp_rs_tests::census::{ExceptionError, Exceptions, Finding};

fn row(extra: &str) -> String {
    format!("[[row]]\nreader = \"acadsharp\"\n{extra}\n")
}

#[test]
fn a_row_with_a_reason_and_an_issue_loads() {
    let t = row("kind = \"missing\"\nclass = \"DBPoint\"\nreason = \"wire 2 has no point\"\nissue = \"libviprs/libviprs-dep#83\"");
    let e = Exceptions::parse(&t).unwrap();
    assert_eq!(e.rows.len(), 1);
    let f = Finding::Missing { handle: 1, class: "DBPoint".into() };
    assert_eq!(e.explains("acadsharp", &f), Some(0));
    assert_eq!(e.explains("acadrust", &f), None, "a row is about one reader");
    assert_eq!(
        e.explains("acadsharp", &Finding::Missing { handle: 1, class: "Line".into() }),
        None,
        "and one class"
    );
}

#[test]
fn a_row_with_no_reason_or_no_issue_is_red() {
    let no_reason = row("kind = \"missing\"\nclass = \"DBPoint\"\nreason = \"\"\nissue = \"libviprs/libviprs-dep#83\"");
    assert!(matches!(Exceptions::parse(&no_reason), Err(ExceptionError::NoReason(_))));
    let no_issue = row("kind = \"missing\"\nclass = \"DBPoint\"\nreason = \"x\"\nissue = \"\"");
    assert!(matches!(Exceptions::parse(&no_issue), Err(ExceptionError::NoIssue(_))));
    let bad_issue = row("kind = \"missing\"\nclass = \"DBPoint\"\nreason = \"x\"\nissue = \"#83\"");
    assert!(matches!(Exceptions::parse(&bad_issue), Err(ExceptionError::NoIssue(_))));
}

#[test]
fn missing_takes_a_class_and_unexpected_takes_a_handle() {
    let a = row("kind = \"missing\"\nhandle = \"705\"\nreason = \"x\"\nissue = \"a/b#1\"");
    assert!(matches!(Exceptions::parse(&a), Err(ExceptionError::Malformed(_))));
    let b = row("kind = \"unexpected\"\nclass = \"DBText\"\nreason = \"x\"\nissue = \"a/b#1\"");
    assert!(matches!(Exceptions::parse(&b), Err(ExceptionError::Malformed(_))));
    let c = row("kind = \"unexpected\"\nhandle = \"705\"\nreason = \"x\"\nissue = \"a/b#1\"");
    let e = Exceptions::parse(&c).unwrap();
    assert_eq!(e.explains("acadsharp", &Finding::Unexpected { handle: 0x705 }), Some(0));
    assert_eq!(e.explains("acadsharp", &Finding::Unexpected { handle: 0x706 }), None);
}

#[test]
fn the_committed_list_loads() {
    Exceptions::committed().expect("parity/census_exceptions.toml loads");
}
