use clap::Parser;
use fragshot::{
    config::{Cli, Settings},
    model::Ton,
};
use std::io::Write;

#[test]
fn cli_overrides_toml_without_resetting_other_values() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    write!(
        file,
        "max_ton = \"15.25\"\nreserve_ton = \"1\"\ncategories = [\"city\"]\ndelay_ms = 2000\n"
    )
    .unwrap();
    let cli = Cli::try_parse_from([
        "fragshot",
        "--config",
        file.path().to_str().unwrap(),
        "scan",
        "--max-ton",
        "20",
        "--category",
        "first_name,word",
    ])
    .unwrap();
    let settings = Settings::load(&cli).unwrap();
    assert_eq!(settings.max_ton, Ton(20_000_000_000));
    assert_eq!(settings.reserve_ton, Ton(1_000_000_000));
    assert_eq!(settings.categories, ["first_name", "word"]);
    assert_eq!(settings.delay_ms, 2000);
}
#[test]
fn explicit_query_disables_unrelated_automatic_seeds() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(
        &path,
        "scan:\n  queries: [moon]\n  dictionary_queries: 24\n",
    )
    .unwrap();
    let cli = Cli::try_parse_from([
        "fragshot",
        "--config",
        path.to_str().unwrap(),
        "scan",
        "--query",
        "brain",
    ])
    .unwrap();
    let settings = Settings::load(&cli).unwrap();
    assert_eq!(settings.scan.queries, ["brain"]);
    assert_eq!(settings.scan.dictionary_queries, 0);
    let urls = fragshot::scanner::batch_urls(&settings, &["flower".into()]);
    assert!(!urls.is_empty());
    assert!(urls.iter().all(|url| url
        .query_pairs()
        .any(|(key, value)| key == "query" && value == "brain")));
}

#[test]
fn invalid_configuration_fails_before_network() {
    for args in [
        vec!["--delay-ms", "0"],
        vec!["--limit", "0"],
        vec!["--limit", "1001"],
        vec!["--min-length", "12", "--max-length", "5"],
        vec!["--category", "typo"],
        vec!["--max-ton", "1", "--reserve-ton", "2"],
        vec!["--timeout-seconds", "0"],
    ] {
        let mut argv = vec!["fragshot", "demo"];
        argv.extend(args);
        let cli = Cli::try_parse_from(argv).unwrap();
        assert!(Settings::load(&cli).is_err());
    }
}
#[test]
fn distributed_profiles_apply_the_documented_budget_and_bounded_scan() {
    for text in [
        include_str!("../fragshot.yaml"),
        include_str!("../fragshot.example.yaml"),
        "{}",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fragshot.yaml");
        std::fs::write(&path, text).unwrap();
        let cli = Cli::try_parse_from(["fragshot", "--config", path.to_str().unwrap()]).unwrap();
        let settings = Settings::load(&cli).unwrap();
        let lexicon = fragshot::classify::Lexicon::built_in();
        let mut brain = fragshot::model::Listing::unknown("brain");
        brain.status = fragshot::model::Status::Sale;
        brain.buy_now = Some(Ton(125_000_000_000));
        assert!(
            fragshot::app::select(vec![brain.clone()], &settings, &lexicon)
                .unwrap()
                .is_empty()
        );
        brain.buy_now = Some(Ton(100_000_000_000));
        assert_eq!(
            fragshot::app::select(vec![brain], &settings, &lexicon)
                .unwrap()
                .len(),
            1
        );
        let seeds = lexicon
            .query_seeds(&settings, settings.scan.dictionary_queries)
            .unwrap();
        let urls = fragshot::scanner::batch_urls(&settings, &seeds);
        assert!(urls.len() > 6 && urls.len() <= 32);
        assert_eq!(settings.limit, 100);
        assert_eq!(settings.details, 30);
        assert_eq!(settings.check.max_names, 50);
        assert_eq!(
            settings.matching.languages,
            ["en", "fr", "ru", "de", "es", "it", "pt"]
        );
        assert!(settings.dictionary.frequency_urls.is_empty());
        assert!(settings.results.enabled);
    }
}

#[test]
fn unknown_toml_keys_are_not_silently_ignored() {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    writeln!(file, "max_tno = \"1\"").unwrap();
    let cli = Cli::try_parse_from([
        "fragshot",
        "demo",
        "--config",
        file.path().to_str().unwrap(),
    ])
    .unwrap();
    assert!(Settings::load(&cli).is_err());
}
