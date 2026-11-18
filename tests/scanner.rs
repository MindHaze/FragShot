use anyhow::{bail, Result};
use fragshot::{
    classify::Lexicon,
    config::{Market, Settings, Sort},
    model::{Listing, Status, Ton},
    scanner::{batch_urls, scan, Transport},
};
use reqwest::Url;
use std::collections::{HashSet, VecDeque};

struct Fake {
    batches: VecDeque<Result<Vec<Listing>>>,
    calls: usize,
    details: Vec<String>,
    fail_detail: bool,
}
impl Transport for Fake {
    fn catalog(&mut self, _: &Url) -> Result<Vec<Listing>> {
        self.calls += 1;
        self.batches.pop_front().unwrap()
    }
    fn detail(&mut self, name: &str) -> Result<Listing> {
        self.calls += 1;
        self.details.push(name.into());
        if self.fail_detail {
            bail!("HTTP 429");
        }
        Ok(Listing::unknown(name))
    }
    fn requests(&self) -> usize {
        self.calls
    }
}
fn sale(name: &str) -> Listing {
    let mut row = Listing::unknown(name);
    row.status = Status::Sale;
    row.buy_now = Some(Ton(1_000_000_000));
    row
}

#[test]
fn query_queue_diversifies_seeds_before_other_sorts() {
    let mut settings = Settings::default();
    settings.scan.markets = vec![Market::All];
    settings.scan.queries = vec!["brain".into(), "moon".into(), "flower".into()];
    settings.scan.max_catalog_requests = 3;
    let urls = batch_urls(&settings, &[]);
    let names: Vec<_> = urls
        .iter()
        .map(|u| {
            u.query_pairs()
                .find(|(k, _)| k == "query")
                .unwrap()
                .1
                .to_string()
        })
        .collect();
    assert_eq!(names, ["brain", "moon", "flower"]);
}
#[test]
fn high_interest_details_are_checked_before_cheap_weak_names() {
    let mut settings = Settings {
        details: 1,
        ..Default::default()
    };
    settings.scan.max_catalog_requests = 1;
    let urls = batch_urls(&settings, &[]);
    let mut expensive = sale("brain");
    expensive.buy_now = Some("2".parse().unwrap());
    let mut client = Fake {
        batches: VecDeque::from([Ok(vec![sale("brainer"), expensive])]),
        calls: 0,
        details: vec![],
        fail_detail: false,
    };
    let result = scan(&mut client, &settings, &Lexicon::built_in(), &urls, |_| {});
    assert!(result.error.is_none());
    assert_eq!(client.details, ["brain"]);
}
#[test]
fn query_seeds_are_bounded_and_obey_filters() {
    let mut settings = Settings {
        categories: vec!["space".into()],
        ..Default::default()
    };
    settings.matching.languages = vec!["en".into()];
    let lexicon = Lexicon::built_in();
    let names = lexicon.query_seeds(&settings, 3).unwrap();
    assert!(!names.is_empty() && names.len() <= 3);
    for name in names {
        let mut row = Listing::unknown(&name);
        lexicon.annotate(&mut row).unwrap();
        assert!(fragshot::app::matches_name(&row, &settings));
    }
}
#[test]
fn queue_is_distinct_bounded_and_never_invents_page_parameters() {
    let mut settings = Settings::default();
    settings.scan.markets = vec![Market::Auction, Market::Auction, Market::Sale];
    settings.scan.sorts = vec![Sort::PriceAsc, Sort::Recent];
    settings.scan.queries = vec!["brain".into(), "brain".into(), "alice".into()];
    settings.scan.max_catalog_requests = 5;
    let urls = batch_urls(&settings, &[]);
    assert_eq!(urls.len(), 5);
    assert_eq!(urls.iter().collect::<HashSet<_>>().len(), 5);
    for url in urls {
        assert_eq!(url.host_str(), Some("fragment.com"));
        assert!(!url
            .query_pairs()
            .any(|(k, _)| k == "page" || k == "offset_id"));
    }
}
#[test]
fn detail_observation_replaces_duplicates_and_hidden_names_never_leak() {
    let mut settings = Settings {
        details: 1,
        ..Default::default()
    };
    settings.scan.max_catalog_requests = 2;
    let urls = batch_urls(&settings, &[]);
    let mut client = Fake {
        batches: VecDeque::from([
            Ok(vec![sale("brain"), sale("noisename123")]),
            Ok(vec![sale("brain"), sale("alice")]),
        ]),
        calls: 0,
        details: vec![],
        fail_detail: false,
    };
    let mut logs = String::new();
    let result = scan(&mut client, &settings, &Lexicon::built_in(), &urls, |s| {
        logs.push_str(s)
    });
    assert!(result.error.is_none());
    assert_eq!(result.items.len(), 3);
    assert_eq!(result.requests, 3);
    assert_eq!(result.hidden, 1);
    assert_eq!(
        result
            .items
            .iter()
            .find(|r| r.username == "alice")
            .unwrap()
            .status,
        Status::Unknown
    );
    assert!(!logs.contains("noisename123"));
}
#[test]
fn empty_batches_continue_and_details_skip_status_and_budget_exclusions() {
    let mut settings = Settings {
        details: 3,
        statuses: vec![Status::Sale],
        ..Default::default()
    };
    settings.scan.max_catalog_requests = 2;
    let mut sold = sale("moon");
    sold.status = Status::Sold;
    let mut expensive = sale("nike");
    expensive.buy_now = Some("1000".parse().unwrap());
    let mut client = Fake {
        batches: VecDeque::from([Ok(vec![]), Ok(vec![sold, expensive, sale("brain")])]),
        calls: 0,
        details: vec![],
        fail_detail: false,
    };
    let result = scan(
        &mut client,
        &settings,
        &Lexicon::built_in(),
        &batch_urls(&settings, &[]),
        |_| {},
    );
    assert!(result.error.is_none());
    assert_eq!(result.batches, 2);
    assert_eq!(client.details, ["brain"]);
    let updated = result.items.iter().find(|r| r.username == "brain").unwrap();
    assert_eq!(updated.status, Status::Unknown);
    let rating = updated.evaluation.as_ref().unwrap();
    assert!(!rating.assessable);
    assert_eq!(rating.interest_tenths, 0);
}

#[test]
fn transport_failure_stops_immediately_and_keeps_partial_observations() {
    let settings = Settings::default();
    let urls = batch_urls(&settings, &[]);
    for failure in ["HTTP 403", "HTTP 429", "Challenge page"] {
        let mut client = Fake {
            batches: VecDeque::from([Ok(vec![sale("brain")]), Err(anyhow::anyhow!(failure))]),
            calls: 0,
            details: vec![],
            fail_detail: false,
        };
        let result = scan(&mut client, &settings, &Lexicon::built_in(), &urls, |_| {});
        assert!(result.error.unwrap().contains(failure));
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.requests, 2);
        assert!(client.details.is_empty());
    }
    let mut client = Fake {
        batches: VecDeque::from([Ok(vec![sale("brain"), sale("alice")])]),
        calls: 0,
        details: vec![],
        fail_detail: true,
    };
    let result = scan(
        &mut client,
        &settings,
        &Lexicon::built_in(),
        &urls[..1],
        |_| {},
    );
    assert!(result.error.unwrap().contains("429"));
    assert_eq!(result.requests, 2);
    assert_eq!(client.details, ["alice"]);
}
