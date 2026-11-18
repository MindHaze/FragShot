use fragshot::{
    classify::Lexicon,
    config::DictionarySettings,
    dictionary::{import_reader, DictionaryStore},
};
use std::sync::atomic::AtomicBool;

#[test]
fn ordinary_curated_words_have_specific_semantic_categories() {
    let lexicon = Lexicon::built_in();
    for (name, category) in [
        ("brain", "body"),
        ("moon", "space"),
        ("flower", "nature"),
        ("winter", "time"),
    ] {
        assert!(
            lexicon
                .analyze(name)
                .unwrap()
                .iter()
                .any(|a| a.language == "en" && a.categories.iter().any(|c| c == category)),
            "{name} should have {category} evidence"
        );
    }
    assert!(lexicon.analyze("xbrainx").unwrap().is_empty());
}

#[test]
fn known_language_evidence_refines_only_the_same_curated_meaning() {
    use fragshot::{app::prepare, config::Settings, model::Listing};
    let lexicon = Lexicon::built_in();
    let mut item = Listing::unknown("alice");
    lexicon.annotate(&mut item).unwrap();
    let curated = item.analyses[0].clone();
    let mut english = curated.clone();
    english.language = "en".into();
    english.source = "dictionary fixture".into();
    let mut french = english.clone();
    french.language = "fr".into();
    french.categories = vec!["word".into()];
    item.analyses.extend([french, english]);
    let mut settings = Settings::default();
    settings.matching.languages = vec!["fr".into(), "en".into()];
    prepare(&mut item, &settings, &lexicon).unwrap();
    let primary = item.primary.as_ref().unwrap();
    assert_eq!(primary.language, "en");
    assert_eq!(primary.category, "first_name");
    assert_eq!(item.analyses[primary.analysis_index].language, "en");
    assert_eq!(item.analyses.len(), 3);
    settings.matching.languages = vec!["fr".into()];
    prepare(&mut item, &settings, &lexicon).unwrap();
    let primary = item.primary.unwrap();
    assert_eq!(primary.language, "und");
    assert_eq!(primary.category, "first_name");
}

#[test]
fn semantic_overlay_preserves_language_specific_transliteration() {
    use fragshot::{app::prepare, config::Settings, model::Listing};
    let dir = tempfile::tempdir().unwrap();
    let options = DictionarySettings {
        directory: dir.path().into(),
        ..Default::default()
    };
    let fixture = "{\"word\":\"луна\",\"lang_code\":\"ru\",\"pos\":\"noun\",\"senses\":[{}]}\n";
    import_reader(
        fixture.as_bytes(),
        &options,
        &["ru".into()],
        "fixture",
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    let mut lexicon = Lexicon::built_in();
    lexicon.attach(dir.path()).unwrap();
    let mut settings = Settings::default();
    settings.matching.languages = vec!["ru".into()];
    let mut row = Listing::unknown("luna");
    prepare(&mut row, &settings, &lexicon).unwrap();
    let primary = row.primary.as_ref().unwrap();
    assert_eq!(primary.language, "ru");
    assert_eq!(primary.category, "space");
    let analysis = &row.analyses[primary.analysis_index];
    assert_eq!(analysis.original, "луна");
    assert!(analysis.normalization.starts_with("transliteration:"));
    assert_eq!(row.evaluation.unwrap().name_tenths, 65);
}

#[test]
fn import_understands_namespaces_objects_and_category_boundaries() {
    let dir = tempfile::tempdir().unwrap();
    let options = DictionarySettings {
        directory: dir.path().to_path_buf(),
        ..Default::default()
    };
    let fixture = concat!(
        "{\"word\":\"dessert\",\"lang_code\":\"en\",\"pos\":\"noun\",\"senses\":[{\"categories\":[\"en:Foods\"]}]}\n",
        "{\"word\":\"celine\",\"lang_code\":\"fr\",\"pos\":\"name\",\"senses\":[{\"categories\":[{\"name\":\"French given names\"}]}]}\n",
        "{\"word\":\"foofood\",\"lang_code\":\"en\",\"pos\":\"noun\",\"senses\":[{\"categories\":[\"en:Foodstuffishness\"]}]}\n"
    );
    import_reader(
        fixture.as_bytes(),
        &options,
        &["en".into(), "fr".into()],
        "fixture",
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    let store = DictionaryStore::open(dir.path()).unwrap().unwrap();
    assert!(store.lookup("dessert").unwrap()[0]
        .categories
        .contains(&"food".into()));
    assert!(store.lookup("celine").unwrap()[0]
        .categories
        .contains(&"first_name".into()));
    assert!(!store.lookup("foofood").unwrap()[0]
        .categories
        .contains(&"food".into()));
}
