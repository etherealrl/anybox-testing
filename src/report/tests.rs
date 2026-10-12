use super::{Outcome, Report};

#[test]
fn the_summary_counts_every_outcome_and_the_trace_comes_last() {
    let mut report = Report::default();
    report.section("box");
    report.check("moves", Outcome::Passed, "");
    report.check("locks", Outcome::Failed, "left still worked\nright too");
    report.check("keys", Outcome::NotAvailable, "no keyboard");
    let text = report.render("header", "1.0 COM3 open\n");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "header");
    assert_eq!(
        lines[1],
        "1 passed, 1 failed, 0 skipped, 1 not available, 0 noted"
    );
    assert!(text.contains("locks: failed, left still worked\n    right too\n"));
    assert!(text.ends_with("## trace\n\n1.0 COM3 open\n"));
    assert_eq!(report.failed(), 1);
}

#[test]
fn an_empty_trace_says_so() {
    let text = Report::default().render("header", "");
    assert!(text.ends_with("nothing went over a traced port\n"));
}
