use fragshot::frequency::{apply_reader, source_language};
use rusqlite::Connection;
use std::sync::atomic::AtomicBool;

#[test]
fn frequencies_rank_existing_words_in_the_matching_language_only() {
    let mut db = Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE entries(name TEXT, language TEXT, frequency INTEGER DEFAULT 0); INSERT INTO entries(name,language) VALUES('brain','en'),('cafe','en'),('cafe','fr');").unwrap();
    let count = apply_reader(
        &b"brain 10\ncafe 20\nnotaword 999\n"[..],
        &mut db,
        "en",
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(count, 2);
    assert_eq!(
        db.query_row(
            "SELECT name FROM entries ORDER BY frequency DESC LIMIT 1",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "cafe"
    );
    assert_eq!(
        db.query_row(
            "SELECT frequency FROM entries WHERE language='fr'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM entries", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        3
    );
    assert!(apply_reader(
        &b"brain invalid\n"[..],
        &mut db,
        "en",
        &AtomicBool::new(false)
    )
    .is_err());
    assert!(apply_reader(&b"brain 100\n"[..], &mut db, "en", &AtomicBool::new(true)).is_err());
}

#[test]
fn ranks_are_per_language_and_duplicate_senses_do_not_inflate_counts() {
    use fragshot::{classify::Lexicon, config::DictionarySettings, dictionary::import_reader};
    let dir = tempfile::tempdir().unwrap();
    let options = DictionarySettings {
        directory: dir.path().into(),
        ..Default::default()
    };
    let data = concat!(
        "{\"word\":\"brain\",\"lang_code\":\"en\",\"pos\":\"noun\",\"senses\":[{}, {\"tags\":[\"slang\"]}]}\n",
        "{\"word\":\"moon\",\"lang_code\":\"en\",\"pos\":\"noun\",\"senses\":[{}]}\n",
        "{\"word\":\"lune\",\"lang_code\":\"fr\",\"pos\":\"noun\",\"senses\":[{}]}\n"
    );
    import_reader(
        data.as_bytes(),
        &options,
        &["en".into(), "fr".into()],
        "fixture",
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("current.json")).unwrap()).unwrap();
    let mut db = Connection::open(dir.path().join(manifest["database"].as_str().unwrap())).unwrap();
    apply_reader(
        &b"brain 10\nmoon 15\n"[..],
        &mut db,
        "en",
        &AtomicBool::new(false),
    )
    .unwrap();
    apply_reader(&b"lune 1\n"[..], &mut db, "fr", &AtomicBool::new(false)).unwrap();
    drop(db);
    let mut lexicon = Lexicon::built_in();
    lexicon.attach(dir.path()).unwrap();
    lexicon.load_frequency(&["en".into(), "fr".into()]).unwrap();
    assert_eq!(lexicon.frequency_rank("moon", "en"), Some(1));
    assert_eq!(lexicon.frequency_rank("brain", "en"), Some(2));
    assert_eq!(lexicon.frequency_rank("lune", "fr"), Some(1));
    assert_eq!(lexicon.frequency_rank("brain", "fr"), None);
}

#[test]
fn frequency_urls_must_be_the_expected_https_dataset_paths() {
    assert_eq!(source_language("https://raw.githubusercontent.com/hermitdave/FrequencyWords/master/content/2018/en/en_50k.txt").unwrap(), "en");
    for url in ["http://raw.githubusercontent.com/hermitdave/FrequencyWords/master/content/2018/en/en_50k.txt", "https://evil.example/en.txt", "https://raw.githubusercontent.com/someone/other/main/en.txt"] {
        assert!(source_language(url).is_err());
    }
}
