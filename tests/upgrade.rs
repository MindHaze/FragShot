use std::{fs, path::Path, process::Command};

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fragshot"))
}

fn text_reports(path: &Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                found.extend(text_reports(&path));
            } else if path.extension().is_some_and(|ext| ext == "txt") {
                found.push(path);
            }
        }
    }
    found
}

#[test]
fn yaml_only_demo_hides_noise_and_writes_grouped_reports() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("fragshot.yaml"),
        "run:\n  mode: demo\nconsole:\n  color: never\nresults:\n  directory: results\n",
    )
    .unwrap();
    let output = binary().current_dir(dir.path()).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("SIMULATED"));
    assert!(stdout.contains("@brain"));
    assert!(stdout.contains("@brainer"));
    assert!(stdout.contains("SEMI-OG"));
    assert!(!stdout.contains("unknownsample"));
    assert!(!stdout.contains('\u{1b}'));
    let files = text_reports(&dir.path().join("results"));
    let brain = files
        .iter()
        .find(|p| p.file_name().unwrap() == "brain.txt")
        .expect("brain report");
    assert!(brain.to_string_lossy().contains("English"));
    let report = fs::read_to_string(brain).unwrap();
    assert!(report.contains("https://fragment.com/username/brain"));
    assert!(report.contains("SIMULATED"));
    assert!(!report.contains('\u{1b}'));
    assert!(!files
        .iter()
        .any(|p| p.file_name().unwrap() == "unknownsample.txt"));
}

#[test]
fn yaml_cli_overrides_and_repeated_runs_do_not_overwrite_reports() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("fragshot.yaml"), "run:\n  mode: scan\nbudget:\n  max_ton: '1'\nconsole:\n  color: always\nresults:\n  directory: results\n").unwrap();
    for _ in 0..2 {
        let output = binary()
            .current_dir(dir.path())
            .args(["demo", "--max-ton", "100", "--color", "never"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(stdout.contains("100 TON"));
        assert!(!stdout.contains('\u{1b}'));
    }
    let files = text_reports(&dir.path().join("results"));
    assert_eq!(
        files
            .iter()
            .filter(|p| p.file_name().unwrap() == "brain.txt")
            .count(),
        2
    );
}

#[test]
fn yaml_overrides_can_correct_tiers_and_explicitly_disable_results() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("fragshot.yaml"), "run:\n  mode: demo\nmatching:\n  overrides:\n    brain:\n      tier: non_og\n      language: en\n      categories: [word]\n    brainer:\n      tier: og\n      language: en\n      categories: [word]\n").unwrap();
    let output = binary()
        .current_dir(dir.path())
        .args(["--save-results=false", "--og-only"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("@brainer"));
    assert!(!stdout.contains("@brain "));
    assert!(!dir.path().join("results").exists());
}

#[test]
fn ratings_export_everywhere_and_rank_flags_override_yaml() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("fragshot.yaml"), "run:\n  mode: demo\n  show_unavailable: true\nranking:\n  sort: price_asc\nscoring:\n  og_reference_ton: '100'\n  semi_og_reference_ton: '25'\nconsole:\n  color: never\nresults:\n  json: out.json\n  csv: out.csv\n").unwrap();
    let output = binary()
        .current_dir(dir.path())
        .args(["--rank-by", "interest"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("out.json")).unwrap()).unwrap();
    let rows = json["listings"].as_array().unwrap();
    assert_eq!(stdout.matches("Interest:").count(), rows.len());
    assert!(rows
        .windows(2)
        .all(|pair| pair[0]["evaluation"]["interest_tenths"].as_u64()
            >= pair[1]["evaluation"]["interest_tenths"].as_u64()));
    for row in rows {
        assert!(row["evaluation"]["name_tenths"].is_number());
    }
    let brain = text_reports(&dir.path().join("results"))
        .into_iter()
        .find(|p| p.file_name().unwrap() == "brain.txt")
        .unwrap();
    assert!(brain.to_string_lossy().contains("Body"));
    assert!(fs::read_to_string(brain).unwrap().contains("Interest:"));
    let csv = fs::read_to_string(dir.path().join("out.csv")).unwrap();
    assert!(csv.contains("interest_score") && csv.contains("rating_basis") && csv.contains("/10"));
    assert!(!stdout.contains('\u{1b}'));
}

#[test]
fn scoring_config_rejects_numeric_or_zero_anchors() {
    for value in ["100", "'0'"] {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("fragshot.yaml"),
            format!("run:\n  mode: demo\nscoring:\n  og_reference_ton: {value}\n"),
        )
        .unwrap();
        let output = binary().current_dir(dir.path()).output().unwrap();
        assert!(!output.status.success());
        assert!(!dir.path().join("results").exists());
    }
}

#[test]
fn yaml_rejects_unknown_keys_and_numeric_ton_before_network() {
    for config in [
        "run:\n  mode: demo\nbudget:\n  max_ton: 100\n",
        "run:\n  mode: demo\nscan:\n  typo: true\n",
    ] {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("fragshot.yaml"), config).unwrap();
        let output = binary().current_dir(dir.path()).output().unwrap();
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("quoted TON string") || error.contains("unknown field `typo`"),
            "{error}"
        );
        assert!(!dir.path().join("results").exists());
    }
}
