use fragshot::{
    app::prepare,
    classify::Lexicon,
    config::Settings,
    model::{Listing, Status, Ton},
    scoring::format_score,
};

fn offer(name: &str, price: &str) -> Listing {
    let mut item = Listing::unknown(name);
    item.status = Status::Sale;
    item.buy_now = Some(price.parse().unwrap());
    item
}
fn evaluate(mut item: Listing, settings: &Settings) -> Listing {
    prepare(&mut item, settings, &Lexicon::built_in()).unwrap();
    item
}
#[test]
fn ratings_are_bounded_monotonic_and_budget_independent() {
    let settings = Settings::default();
    let low = evaluate(offer("brain", "1"), &settings);
    let high = evaluate(offer("brain", "100"), &settings);
    let low_score = low.evaluation.unwrap();
    let high_score = high.evaluation.unwrap();
    assert!(low_score.interest_tenths > high_score.interest_tenths);
    assert!(low_score.name_tenths <= 100 && low_score.price_tenths <= 100);
    assert_eq!(format_score(72), "7.2/10");
    let changed = Settings {
        max_ton: "900".parse().unwrap(),
        ..settings
    };
    assert_eq!(
        low_score,
        evaluate(offer("brain", "1"), &changed).evaluation.unwrap()
    );
}
#[test]
fn bid_and_purchase_are_separate_and_history_is_ignored() {
    let mut item = offer("brain", "100");
    item.status = Status::Auction;
    item.minimum_bid = Some("1".parse().unwrap());
    item.last_bid = Some("0.1".parse().unwrap());
    item.ends_at = Some(chrono::Utc::now().timestamp() + 3600);
    let result = evaluate(item.clone(), &Settings::default());
    let score = result.evaluation.unwrap();
    assert_eq!(score.basis, "buy_now");
    assert_eq!(score.actions.len(), 2);
    assert_eq!(score.price_tenths, score.actions[0].price_tenths);
    assert!(score.actions[1].provisional);
    assert!(score.actions[1].interest_tenths <= 60);
    item.last_bid = Some("1000".parse().unwrap());
    assert_eq!(
        score,
        evaluate(item, &Settings::default()).evaluation.unwrap()
    );
}
#[test]
fn unknown_zero_sold_and_expired_prices_are_not_bargains() {
    for mut item in [
        Listing::unknown("brain"),
        offer("brain", "0"),
        offer("brain", "1"),
        offer("brain", "1"),
    ]
    .into_iter()
    .enumerate()
    {
        if item.0 == 2 {
            item.1.status = Status::Sold;
        }
        if item.0 == 3 {
            item.1.status = Status::Auction;
            item.1.ends_at = Some(1);
        }
        let score = evaluate(item.1, &Settings::default()).evaluation.unwrap();
        assert!(!score.assessable);
        assert_eq!(score.interest_tenths, 0);
        assert_eq!(score.price_tenths, 0);
        assert!(score.name_tenths > 0);
    }
}
#[test]
fn extreme_amounts_and_unrecognized_names_cannot_overflow_or_win() {
    let mut settings = Settings::default();
    settings.scoring.og_reference_ton = Ton(u64::MAX);
    let mut item = offer("brain", "1");
    item.buy_now = Some(Ton(u64::MAX));
    let score = evaluate(item, &settings).evaluation.unwrap();
    assert!(score.price_tenths <= 100);
    settings.matching.show_non_og = true;
    let noise = evaluate(offer("xxxxxx", "0.000000001"), &settings)
        .evaluation
        .unwrap();
    assert!(noise.name_tenths <= 20 && noise.interest_tenths <= 20);
}
#[test]
fn primary_category_ignores_secondary_offensive_meaning_but_retains_it() {
    let lexicon = Lexicon::built_in();
    let mut item = offer("brain", "8");
    lexicon.annotate(&mut item).unwrap();
    let mut secondary = item.analyses[0].clone();
    secondary.categories = vec!["profanity".into()];
    secondary.source = "dictionary fixture".into();
    item.analyses.insert(0, secondary);
    item.categories.push("profanity".into());
    prepare(&mut item, &Settings::default(), &lexicon).unwrap();
    assert_eq!(item.primary.as_ref().unwrap().category, "body");
    assert_eq!(item.primary.as_ref().unwrap().language, "en");
    assert!(item
        .analyses
        .iter()
        .any(|a| a.categories.contains(&"profanity".into())));
    let filtered = Settings {
        categories: vec!["profanity".into()],
        ..Default::default()
    };
    prepare(&mut item, &filtered, &lexicon).unwrap();
    assert_eq!(item.primary.unwrap().category, "profanity");
}
#[test]
fn evidence_does_not_label_catalogue_or_missing_pages_as_detail_observations() {
    for (evidence, expected) in [
        (
            "Catalogue Available; start-auction must be confirmed on the detail page",
            "catalogue-only",
        ),
        ("HTTP 404; availability not determined", "unconfirmed"),
        (
            "No listing recognised; absence does not imply availability",
            "unconfirmed",
        ),
        ("Fragment listing: For sale", "detail-observed"),
    ] {
        let mut item = Listing::unknown("brain");
        item.evidence = evidence.into();
        let score = evaluate(item, &Settings::default()).evaluation.unwrap();
        assert_eq!(score.evidence_level, expected);
    }
}

#[test]
fn legacy_listings_without_scores_remain_loadable() {
    let mut old = serde_json::to_value(offer("brain", "8")).unwrap();
    old.as_object_mut().unwrap().remove("primary");
    old.as_object_mut().unwrap().remove("evaluation");
    let mut row: Listing = serde_json::from_value(old).unwrap();
    assert!(row.primary.is_none() && row.evaluation.is_none());
    prepare(&mut row, &Settings::default(), &Lexicon::built_in()).unwrap();
    assert!(row.evaluation.unwrap().assessable);
}

#[test]
fn missing_commonness_and_brand_risk_are_disclosed() {
    let score = evaluate(offer("nike", "7"), &Settings::default())
        .evaluation
        .unwrap();
    assert!(score.warnings.iter().any(|w| w.contains("rights")));
    assert!(score
        .warnings
        .iter()
        .any(|w| w.contains("commonness unknown")));
}
