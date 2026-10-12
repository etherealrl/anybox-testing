use std::process::Command;

/// the whole flow on the mock box, so a change to the tester can't break a run nobody here can repeat
#[test]
fn a_dry_run_goes_through_and_writes_a_clean_report() {
    let out = std::env::temp_dir().join(format!("anybox-testing-{}", std::process::id()));
    std::fs::create_dir_all(&out).unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_anybox-testing"))
        .args(["--dry-run", "--out"])
        .arg(&out)
        .status()
        .unwrap();
    let report = std::fs::read_dir(&out)
        .unwrap()
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .find(|path| path.to_string_lossy().contains("anybox-dry-run-"))
        .expect("no report written");
    let text = std::fs::read_to_string(&report).unwrap();
    let leftovers = std::fs::read_dir(&out).unwrap().count();
    std::fs::remove_dir_all(&out).unwrap();
    assert_eq!(leftovers, 1, "the trace file should be gone");
    assert!(status.success(), "{text}");
    assert!(text.contains(", 0 failed,"), "{text}");
    for check in [
        "move 100 0 reached the PC: noted",
        "button events, all five buttons: passed",
        "unplug and replug: passed",
        "close: passed",
    ] {
        assert!(text.contains(check), "{check} missing in\n{text}");
    }
}
