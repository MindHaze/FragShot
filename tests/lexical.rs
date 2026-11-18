use fragshot::{
    app::matches_name,
    classify::Lexicon,
    config::{DictionarySettings, Settings},
    dictionary::import_reader,
    model::{LexicalAnalysis, Listing, Tier},
};
use std::sync::atomic::AtomicBool;

#[test]
fn tiers_and_categories_must_match_the_same_allowed_language_analysis() {
    let mut row = Listing::unknown("cafe");
    row.analyses = vec![
        LexicalAnalysis {
            tier: Tier::Og,
            original: "cafe".into(),
            language: "en".into(),
            categories: vec!["food".into()],
            base: None,
            rule: "fixture".into(),
            source: "fixture".into(),
            normalization: "exact".into(),
        },
        LexicalAnalysis {
            tier: Tier::SemiOg,
            original: "cafe".into(),
            language: "fr".into(),
            categories: vec!["word".into()],
            base: Some("cafeter".into()),
            rule: "fixture".into(),
            source: "fixture".into(),
            normalization: "exact".into(),
        },
    ];
    row.categories = vec!["food".into(), "word".into()];
    let mut settings = Settings::default();
    settings.matching.languages = vec!["fr".into()];
    settings.matching.tiers = vec!["og".into()];
    assert!(
        !matches_name(&row, &settings),
        "English OG evidence must not satisfy a French OG filter"
    );
    settings.matching.tiers = vec!["semi_og".into()];
    settings.categories = vec!["food".into()];
    assert!(
        !matches_name(&row, &settings),
        "English food evidence must not satisfy a French category filter"
    );
    settings.categories.clear();
    assert!(matches_name(&row, &settings));
}

#[test]
fn independent_lemmas_override_suffixes_but_random_decorations_do_not_match() {
    let lexicon = Lexicon::built_in();
    assert_eq!(lexicon.analyze("brain").unwrap()[0].tier, Tier::Og);
    assert_eq!(lexicon.analyze("flower").unwrap()[0].tier, Tier::Og);
    let semi = lexicon.analyze("brainer").unwrap();
    assert_eq!(semi[0].tier, Tier::SemiOg);
    assert_eq!(semi[0].base.as_deref(), Some("brain"));
    assert_eq!(lexicon.analyze("brains").unwrap()[0].tier, Tier::SemiOg);
    for noise in ["xbrainx", "brain123", "brain_", "brainss"] {
        assert!(lexicon.analyze(noise).unwrap().is_empty(), "{noise}");
    }
}

#[test]
fn local_root_overrides_apply_to_suffixes_and_candidate_names() {
    use fragshot::config::LocalOverride;
    let mut lexicon = Lexicon::built_in();
    lexicon
        .set_overrides(
            [
                (
                    "brain".into(),
                    LocalOverride {
                        tier: Tier::NonOg,
                        base: None,
                        language: "en".into(),
                        categories: vec!["word".into()],
                    },
                ),
                (
                    "zzcustomroot".into(),
                    LocalOverride {
                        tier: Tier::Og,
                        base: None,
                        language: "en".into(),
                        categories: vec!["word".into()],
                    },
                ),
            ]
            .into(),
        )
        .unwrap();
    assert!(lexicon.analyze("brains").unwrap().is_empty());
    assert_eq!(
        lexicon.analyze("zzcustomroots").unwrap()[0].tier,
        Tier::SemiOg
    );
    assert!(lexicon
        .bounded_names(1000)
        .unwrap()
        .contains(&"zzcustomroot".into()));
}

#[test]
fn incomplete_and_orphan_form_records_are_not_og_evidence() {
    let dir = tempfile::tempdir().unwrap();
    let options = DictionarySettings {
        directory: dir.path().into(),
        ..Default::default()
    };
    let data = concat!(
        "{\"word\":\"brain\",\"lang_code\":\"en\",\"pos\":\"noun\",\"senses\":[{\"glosses\":[\"organ\"]}]}\n",
        "{\"word\":\"garbage\",\"lang_code\":\"en\"}\n",
        "{\"word\":\"orphans\",\"lang_code\":\"en\",\"pos\":\"noun\",\"senses\":[{\"form_of\":[{\"word\":\"missingroot\"}]}]}\n",
        "{\"word\":\"tagonly\",\"lang_code\":\"en\",\"pos\":\"noun\",\"senses\":[{\"tags\":[\"form-of\"]}]}\n"
    );
    import_reader(
        data.as_bytes(),
        &options,
        &["en".into()],
        "fixture",
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    let mut lexicon = Lexicon::built_in();
    lexicon.attach(dir.path()).unwrap();
    for name in ["garbage", "orphans", "tagonly"] {
        assert!(
            lexicon.analyze(name).unwrap().is_empty(),
            "{name} must lack usable evidence"
        );
    }
}
