use std::process::Command;
fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fragshot"))
}
fn legacy_labels(key: &str) -> Vec<String> {
    let data: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/legacy-ui.json")).unwrap();
    data[key]
        .as_array()
        .expect("legacy label fixture")
        .iter()
        .map(|value| value.as_str().unwrap().to_owned())
        .collect()
}

// Rejected historical labels are fixture data, never application copy.
#[test]
fn demo_output_uses_english_status_labels() {
    let dir = tempfile::tempdir().unwrap();
    let output = binary()
        .current_dir(dir.path())
        .args(["demo", "--color", "never"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Required English status labels
    assert!(
        stdout.contains("BUY NOW")
            || stdout.contains("START AUCTION")
            || stdout.contains("BID")
            || stdout.contains("SOLD")
            || stdout.contains("UNAVAILABLE")
            || stdout.contains("UNKNOWN"),
        "expected English status labels in stdout:\n{stdout}"
    );
    for label in legacy_labels("status_labels")
        .into_iter()
        .chain(legacy_labels("budget_labels"))
    {
        assert!(
            !stdout.contains(&label),
            "Rejected legacy label {label:?} found in stdout"
        );
    }
    // Demo banner must say SIMULATED
    assert!(
        stdout.contains("SIMULATED"),
        "expected SIMULATED in demo banner, got:\n{stdout}"
    );
}
#[test]
fn demo_json_uses_english_category_tokens() {
    let dir = tempfile::tempdir().unwrap();
    let json = dir.path().join("cat.json");
    let output = binary()
        .current_dir(dir.path())
        .args(["demo", "--json"])
        .arg(&json)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data = std::fs::read_to_string(&json).unwrap();
    // English tokens must appear
    assert!(
        data.contains("first_name")
            || data.contains("surname")
            || data.contains("brand")
            || data.contains("video_game")
            || data.contains("word")
            || data.contains("city")
            || data.contains("unknown"),
        "no English category token found in JSON:\n{data}"
    );
    for token in legacy_labels("category_tokens") {
        assert!(
            !data.contains(&token),
            "Rejected legacy category {token:?} found in JSON export"
        );
    }
}
#[test]
fn category_flag_accepts_english_tokens_and_rejects_french() {
    let dir = tempfile::tempdir().unwrap();
    // English token must be accepted
    let ok = binary()
        .current_dir(dir.path())
        .args(["demo", "--category", "first_name"])
        .output()
        .unwrap();
    assert!(
        ok.status.success(),
        "English category token rejected: {}",
        String::from_utf8_lossy(&ok.stderr)
    );
    for token in legacy_labels("category_tokens") {
        let err = binary()
            .current_dir(dir.path())
            .args(["demo", "--category", &token])
            .output()
            .unwrap();
        assert!(
            !err.status.success(),
            "Legacy category {token:?} was accepted but should have been rejected"
        );
    }
}
#[test]
fn error_messages_are_in_english() {
    let dir = tempfile::tempdir().unwrap();
    // Trigger an error (check with no names) and check that stderr is English
    let result = binary()
        .current_dir(dir.path())
        .arg("check")
        .output()
        .unwrap();
    assert!(!result.status.success());
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("Error: Provide usernames, --file names.txt, or --dictionary"));
    for word in legacy_labels("error_words") {
        assert!(
            !stderr.contains(&word),
            "Rejected legacy error word {word:?} found: {stderr}"
        );
    }
}
#[test]
fn demo_exports_simulated_data_without_network_or_ansi() {
    let dir = tempfile::tempdir().unwrap();
    let json = dir.path().join("result.json");
    let csv = dir.path().join("result.csv");
    let output = binary()
        .current_dir(dir.path())
        .args(["demo", "--color", "never", "--max-ton", "20", "--json"])
        .arg(&json)
        .arg("--csv")
        .arg(&csv)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("SIMUL"));
    assert!(!stdout.contains('\u{1b}'));
    let result: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
    assert_eq!(result["demo"], true);
    assert!(!result["listings"].as_array().unwrap().is_empty());
    for item in result["listings"].as_array().unwrap() {
        assert!(item["username"].as_str().is_some());
    }
    let csv_data = std::fs::read_to_string(csv).unwrap();
    assert!(csv_data.contains("username,status"));
    assert!(csv_data.contains("true"));
}
#[test]
fn no_silent_overwrite_or_request_without_names() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("protected.json");
    std::fs::write(&output, "keep me").unwrap();
    let result = binary()
        .current_dir(dir.path())
        .args(["demo", "--json"])
        .arg(&output)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert_eq!(std::fs::read_to_string(&output).unwrap(), "keep me");
    let result = binary()
        .current_dir(dir.path())
        .arg("check")
        .output()
        .unwrap();
    assert!(!result.status.success());
    let result = binary()
        .current_dir(dir.path())
        .args(["check", "../alice"])
        .output()
        .unwrap();
    assert!(!result.status.success());
}
#[test]
fn og_only_excludes_unknown_names_even_when_showing_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    let list = dir.path().join("og.json");
    let output = binary()
        .current_dir(dir.path())
        .args(["demo", "--og-only", "--show-unavailable", "--json"])
        .arg(&list)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(list).unwrap()).unwrap();
    assert!(report["listings"]
        .as_array()
        .unwrap()
        .iter()
        .all(|row| !row["categories"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c == "unknown")));
}
#[test]
fn help_explains_commands_and_invalid_options_fail() {
    let output = binary().arg("--help").output().unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("scan") && text.contains("check") && text.contains("demo"));
    assert!(!binary()
        .args(["demo", "--max-ton", "-1"])
        .output()
        .unwrap()
        .status
        .success());
}
