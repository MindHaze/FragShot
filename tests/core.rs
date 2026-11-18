use fragshot::{
    classify::Lexicon,
    model::{normalize_username, Listing, Status, Ton},
};

#[test]
fn ton_is_exact_and_round_trips() {
    for (text, nano, display) in [
        ("0", 0, "0"),
        ("12.500000001", 12_500_000_001, "12.500000001"),
        ("100.2500", 100_250_000_000, "100.25"),
        ("0.000000001", 1, "0.000000001"),
    ] {
        let amount = text.parse::<Ton>().unwrap();
        assert_eq!(amount.0, nano);
        assert_eq!(amount.to_string(), display);
        assert_eq!(
            serde_json::to_string(&amount).unwrap(),
            format!("\"{display}\"")
        );
    }
}
#[test]
fn invalid_or_overflow_amounts_are_rejected() {
    for text in [
        "-1",
        "NaN",
        "inf",
        "1e3",
        "",
        "1.0000000001",
        "18446744074",
        "1,234",
        ".",
        "1.",
    ] {
        assert!(text.parse::<Ton>().is_err(), "{text}");
    }
}
#[test]
fn names_are_normalized_without_allowing_paths_or_terminal_escapes() {
    assert_eq!(normalize_username(" @Alice ").unwrap(), "alice");
    assert_eq!(normalize_username("abc").unwrap(), "abc");
    for name in [
        "",
        "ab",
        "../alice",
        "https://fragment.com/username/alice",
        "alice\x1b[31m",
        "élise",
        "9alice",
        "alice-foo",
    ] {
        assert!(normalize_username(name).is_err(), "{name}");
    }
}
#[test]
fn only_actionable_prices_count_towards_budget() {
    let mut item = Listing::unknown("alice");
    item.last_bid = Some(Ton(5_000_000_000));
    assert!(!item.within_budget(Ton(100_000_000_000), Ton(0)));
    item.status = Status::Auction;
    item.minimum_bid = Some(Ton(10_000_000_001));
    assert!(!item.within_budget(Ton(10_000_000_000), Ton(0)));
    assert!(item.within_budget(Ton(11_000_000_001), Ton(1_000_000_000)));
    assert!(!item.within_budget(Ton(11_000_000_000), Ton(1_000_000_000)));
    item.buy_now = Some(Ton(15_000_000_000));
    assert_eq!(item.action_price(), Some(Ton(10_000_000_001)));
    item.status = Status::Sold;
    assert_eq!(item.action_price(), None);
}
#[test]
fn classification_is_exact_multilabel_and_extensible() {
    let mut lexicon = Lexicon::built_in();
    assert!(lexicon
        .classify("alice")
        .contains(&"first_name".to_string()));
    assert!(lexicon.classify("paris").contains(&"city".to_string()));
    assert!(lexicon
        .classify("mario")
        .contains(&"video_game".to_string()));
    assert!(lexicon.classify("nike").contains(&"brand".to_string()));
    assert_eq!(lexicon.classify("alice123"), vec!["unknown"]);
    lexicon
        .extend_tsv("moon\tword,video_game\n# a comment\n")
        .unwrap();
    assert!(lexicon.classify("moon").contains(&"video_game".to_string()));
    assert!(lexicon.extend_tsv("alice\tfake\n").is_err());
    assert!(lexicon.extend_tsv("../bad\tword\n").is_err());
    assert!(lexicon.names().len() > 100);
}
