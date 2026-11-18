use fragshot::{
    classify::Lexicon,
    config::Settings,
    model::{Listing, Ton},
    output::Report,
    reports::{plain, save},
};
fn report() -> Report {
    Report {
        source: "https://fragment.com",
        demo: false,
        complete: false,
        error: Some("HTTP 429".into()),
        hidden: 0,
        batches: 1,
        checked_at: "2026-11-18T00:00:00Z".into(),
        max_ton: Ton(1_000_000_000),
        reserve_ton: Ton(0),
        examined: 1,
        requests: 1,
        listings: vec![Listing::unknown("con")],
        selection: Default::default(),
    }
}
#[test]
fn report_names_are_safe_and_partial_runs_are_explicit() {
    let temp = tempfile::tempdir().unwrap();
    let mut settings = Settings::default();
    settings.results.directory = temp.path().join("results");
    settings.results.grouping.clear();
    let report = report();
    let root = save(&report, &settings).unwrap().unwrap();
    let text = std::fs::read_to_string(root.join("_con.txt")).unwrap();
    assert!(text.contains("INCOMPLETE RUN: HTTP 429"));
    assert!(text.contains("never treated as free"));
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("run.json")).unwrap()).unwrap();
    assert_eq!(manifest["complete"], false);
    let second = save(&report, &settings).unwrap().unwrap();
    assert_ne!(root, second);
    let mut invalid = report;
    invalid.listings[0].username = "../escape".into();
    assert!(save(&invalid, &settings).is_err());
    assert!(!temp.path().join("escape.txt").exists());
}
#[test]
fn grouping_writes_one_canonical_file_and_retains_all_labels() {
    let temp = tempfile::tempdir().unwrap();
    let mut settings = Settings::default();
    settings.results.directory = temp.path().join("results");
    let mut report = report();
    report.listings = vec![Listing::unknown("brain")];
    fragshot::app::prepare(&mut report.listings[0], &settings, &Lexicon::built_in()).unwrap();
    let root = save(&report, &settings).unwrap().unwrap();
    let text = std::fs::read_to_string(root.join("English/Body/brain.txt")).unwrap();
    assert!(text.contains("https://fragment.com/username/brain"));
    assert!(text.contains("FragShot curated lexicon"));
}
#[test]
fn failed_report_writes_leave_an_explicit_incomplete_marker() {
    let temp = tempfile::tempdir().unwrap();
    let mut settings = Settings::default();
    settings.results.directory = temp.path().join("results");
    let mut report = report();
    report.listings[0].username = "../invalid".into();
    assert!(save(&report, &settings).is_err());
    let run = std::fs::read_dir(&settings.results.directory)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert!(run.join("INCOMPLETE.txt").is_file());
    assert!(!run.join("run.json").exists());
}

#[test]
fn report_text_removes_terminal_and_bidirectional_controls() {
    let clean = plain("hello\u{1b}[31m\u{202e}txt\u{2066}\u{7}");
    assert!(!clean.contains('\u{1b}'));
    assert!(!clean.contains('\u{202e}'));
    assert!(!clean.contains('\u{2066}'));
}
