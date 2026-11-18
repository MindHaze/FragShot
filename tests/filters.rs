use fragshot::{
    app::select,
    classify::Lexicon,
    config::Settings,
    model::{Listing, Status, Ton},
};

fn sale(name: &str, price: u64) -> Listing {
    let mut item = Listing::unknown(name);
    item.status = Status::Sale;
    item.buy_now = Some(Ton(price * 1_000_000_000));
    item
}
#[test]
fn filters_use_price_plus_reserve_and_exact_categories() {
    let settings = Settings {
        max_ton: Ton(10_000_000_000),
        reserve_ton: Ton(1_000_000_000),
        categories: vec!["first_name".into()],
        ..Settings::default()
    };
    let items = select(
        vec![sale("alice", 9), sale("emma", 10), sale("nike", 1)],
        &settings,
        &Lexicon::built_in(),
    )
    .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].username, "alice");
    assert_eq!(items[0].categories, ["first_name"]);
}
#[test]
fn unknown_is_not_free_and_sold_is_not_actionable() {
    let mut sold = sale("emma", 1);
    sold.status = Status::Sold;
    let data = vec![Listing::unknown("alice"), sold, sale("nike", 5)];
    let mut settings = Settings::default();
    let lexicon = Lexicon::built_in();
    assert_eq!(select(data.clone(), &settings, &lexicon).unwrap().len(), 1);
    settings.show_unavailable = true;
    assert_eq!(select(data, &settings, &lexicon).unwrap().len(), 3);
}
#[test]
fn explicit_checks_show_unknown_sold_and_over_budget_without_enabling_them() {
    let mut sold = sale("emma", 1);
    sold.status = Status::Sold;
    let settings = Settings {
        max_ton: Ton(5_000_000_000),
        ..Settings::default()
    };
    let data = vec![Listing::unknown("alice"), sold, sale("nike", 10)];
    let rows = fragshot::app::select_checked(data, &settings, &Lexicon::built_in()).unwrap();
    assert_eq!(rows.len(), 3);
    assert!(rows
        .iter()
        .all(|row| !row.within_budget(settings.max_ton, settings.reserve_ton)));
    assert_eq!(
        rows.iter().find(|r| r.username == "alice").unwrap().status,
        Status::Unknown
    );
}
#[test]
fn ranked_selection_explains_exclusions_before_truncating() {
    let settings = Settings {
        limit: 1,
        max_ton: "10".parse().unwrap(),
        statuses: vec![Status::Sale, Status::Unknown],
        ..Default::default()
    };
    let mut sold = sale("emma", 1);
    sold.status = Status::Sold;
    let data = vec![
        sale("noise123", 1),
        sold,
        Listing::unknown("alice"),
        sale("nike", 20),
        sale("brain", 2),
        sale("brainer", 1),
    ];
    let result =
        fragshot::app::select_report(data, &settings, &Lexicon::built_in(), false).unwrap();
    assert_eq!(result.listings[0].username, "brain");
    assert_eq!(result.stats.lexical_filtered, 1);
    assert_eq!(result.stats.status_filtered, 1);
    assert_eq!(result.stats.no_price, 1);
    assert_eq!(result.stats.over_budget, 1);
    assert_eq!(result.stats.eligible, 2);
    assert_eq!(result.stats.displayed, 1);
    assert_eq!(result.stats.omitted_by_limit, 1);
}
#[test]
fn zero_and_elapsed_auction_prices_are_excluded_consistently() {
    let mut elapsed = sale("moon", 1);
    elapsed.status = Status::Auction;
    elapsed.minimum_bid = Some("1".parse().unwrap());
    elapsed.ends_at = Some(1);
    let data = vec![sale("brain", 0), elapsed];
    let settings = Settings::default();
    let lexicon = Lexicon::built_in();
    let selected = fragshot::app::select_report(data.clone(), &settings, &lexicon, false).unwrap();
    assert_eq!(selected.stats.no_price, 2);
    assert!(selected.listings.is_empty());
    let checked = fragshot::app::select_checked(data, &settings, &lexicon).unwrap();
    assert_eq!(checked.len(), 2);
    for row in checked {
        assert!(row.action_price().is_none());
        assert!(!row.within_budget(settings.max_ton, settings.reserve_ton));
        assert!(!row.evaluation.unwrap().assessable);
    }
}

#[test]
fn selection_deduplicates_limits_and_sorts_by_action_price() {
    let settings = Settings {
        limit: 2,
        ranking: fragshot::config::RankingSettings {
            sort: fragshot::config::RankingSort::PriceAsc,
        },
        ..Settings::default()
    };
    let data = vec![
        sale("alice", 20),
        sale("nike", 5),
        sale("nike", 5),
        sale("paris", 9),
    ];
    let items = select(data, &settings, &Lexicon::built_in()).unwrap();
    assert_eq!(
        items
            .iter()
            .map(|r| r.username.as_str())
            .collect::<Vec<_>>(),
        ["nike", "paris"]
    );
}
