use fragshot::{
    config::DictionarySettings,
    dictionary::{import_reader, DictionaryStore},
    model::Tier,
};
use std::sync::atomic::AtomicBool;

#[test]
fn imports_languages_forms_categories_and_preserves_collisions() {
    let dir = tempfile::tempdir().unwrap();
    let options = DictionarySettings {
        directory: dir.path().to_path_buf(),
        ..DictionarySettings::default()
    };
    let data = concat!(
        "{\"word\":\"brain\",\"lang_code\":\"en\",\"pos\":\"noun\",\"senses\":[{\"glosses\":[\"organ\"]}]}\n",
        "{\"word\":\"brains\",\"lang_code\":\"en\",\"pos\":\"noun\",\"senses\":[{\"form_of\":[{\"word\":\"brain\"}],\"tags\":[\"plural\"]}]}\n",
        "{\"word\":\"café\",\"lang_code\":\"fr\",\"pos\":\"noun\",\"senses\":[{\"topics\":[\"food\"]}]}\n",
        "{\"word\":\"cafe\",\"lang_code\":\"en\",\"pos\":\"noun\",\"senses\":[{}]}\n",
        "{\"word\":\"яблоко\",\"lang_code\":\"ru\",\"pos\":\"noun\",\"forms\":[{\"form\":\"yabloko\",\"tags\":[\"romanization\"]}],\"senses\":[{\"topics\":[\"food\"]}]}\n",
        "{\"word\":\"hell\",\"lang_code\":\"en\",\"pos\":\"interj\",\"senses\":[{\"tags\":[\"offensive\"]}]}\n"
    );
    let stats = import_reader(
        data.as_bytes(),
        &options,
        &["en".into(), "fr".into(), "ru".into()],
        "test fixture",
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    assert_eq!(stats.unique_names, 5);
    let store = DictionaryStore::open(dir.path()).unwrap().unwrap();
    assert_eq!(store.lookup("brain").unwrap()[0].tier, Tier::Og);
    assert_eq!(
        store.lookup("brains").unwrap()[0].base.as_deref(),
        Some("brain")
    );
    assert_eq!(store.lookup("cafe").unwrap().len(), 2);
    assert!(store.lookup("yabloko").unwrap()[0]
        .normalization
        .contains("romanization"));
    assert!(store.lookup("hell").unwrap()[0]
        .categories
        .contains(&"profanity".to_string()));
}

#[test]
fn size_limits_and_truncated_gzip_preserve_the_previous_index() {
    use std::io::Write;
    let dir = tempfile::tempdir().unwrap();
    let mut options = DictionarySettings {
        directory: dir.path().join("index"),
        ..Default::default()
    };
    let valid = b"{\"word\":\"brain\",\"lang_code\":\"en\",\"pos\":\"noun\",\"senses\":[{}]}\n";
    import_reader(
        &valid[..],
        &options,
        &["en".into()],
        "fixture",
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    let pointer = std::fs::read(options.directory.join("current.json")).unwrap();
    options.max_record_bytes = 10;
    assert!(import_reader(
        &valid[..],
        &options,
        &["en".into()],
        "fixture",
        &AtomicBool::new(false),
        |_, _| {}
    )
    .is_err());
    options.max_record_bytes = 4096;
    options.max_uncompressed_bytes = 10;
    assert!(import_reader(
        &valid[..],
        &options,
        &["en".into()],
        "fixture",
        &AtomicBool::new(false),
        |_, _| {}
    )
    .is_err());
    options.max_uncompressed_bytes = 4096;
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(valid).unwrap();
    let mut gzip = encoder.finish().unwrap();
    gzip.truncate(gzip.len() - 4);
    let path = dir.path().join("truncated.gz");
    std::fs::write(&path, gzip).unwrap();
    assert!(fragshot::dictionary::import_file(
        &path,
        &options,
        &["en".into()],
        &AtomicBool::new(false),
        |_, _| {}
    )
    .is_err());
    assert_eq!(
        std::fs::read(options.directory.join("current.json")).unwrap(),
        pointer
    );
}

#[test]
fn interrupted_or_malformed_import_keeps_previous_index() {
    let dir = tempfile::tempdir().unwrap();
    let options = DictionarySettings {
        directory: dir.path().to_path_buf(),
        ..DictionarySettings::default()
    };
    let valid = b"{\"word\":\"brain\",\"lang_code\":\"en\",\"pos\":\"noun\",\"senses\":[{}]}\n";
    import_reader(
        &valid[..],
        &options,
        &["en".into()],
        "fixture",
        &AtomicBool::new(false),
        |_, _| {},
    )
    .unwrap();
    let pointer = std::fs::read(dir.path().join("current.json")).unwrap();
    assert!(import_reader(
        &b"not JSON\n"[..],
        &options,
        &["en".into()],
        "bad",
        &AtomicBool::new(false),
        |_, _| {}
    )
    .is_err());
    assert!(import_reader(
        &valid[..],
        &options,
        &["en".into()],
        "cancelled",
        &AtomicBool::new(true),
        |_, _| {}
    )
    .is_err());
    assert_eq!(
        std::fs::read(dir.path().join("current.json")).unwrap(),
        pointer
    );
    let store = DictionaryStore::open(dir.path()).unwrap().unwrap();
    assert_eq!(store.lookup("brain").unwrap().len(), 1);
}
