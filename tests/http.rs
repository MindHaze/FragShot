use fragshot::{
    client::{catalog_urls, inspect_response, robots_allowed},
    config::{Market, Sort},
};
#[test]
fn catalogue_urls_encode_queries_and_never_follow_user_supplied_hosts() {
    let urls = catalog_urls(Market::All, Sort::PriceAsc, None);
    assert_eq!(urls.len(), 2);
    assert_eq!(
        urls[0].as_str(),
        "https://fragment.com/?sort=price_asc&filter=auction"
    );
    assert_eq!(
        urls[1].as_str(),
        "https://fragment.com/?sort=price_asc&filter=sale"
    );
    let urls = catalog_urls(Market::All, Sort::Recent, Some("a&filter=sold"));
    assert_eq!(urls.len(), 1);
    assert_eq!(
        urls[0].query_pairs().find(|(k, _)| k == "query").unwrap().1,
        "a&filter=sold"
    );
    assert!(!urls[0].query_pairs().any(|(k, _)| k == "filter"));
    assert_eq!(urls[0].host_str(), Some("fragment.com"));
}
#[test]
fn blocks_and_errors_never_become_empty_availability() {
    assert!(inspect_response(403, "forbidden").is_err());
    assert!(inspect_response(429, "slow down").is_err());
    assert!(inspect_response(503, "maintenance").is_err());
    assert!(inspect_response(
        200,
        "<title>Just a moment...</title><script src='/cdn-cgi/challenge-platform/'></script>"
    )
    .is_err());
    assert_eq!(inspect_response(404, "not found").unwrap(), None);
    assert_eq!(
        inspect_response(200, "<html>ok</html>").unwrap().as_deref(),
        Some("<html>ok</html>")
    );
}
#[test]
fn robots_rules_are_respected_for_our_agent() {
    let rules = "User-agent: *\nDisallow: /username/\nAllow: /username/alice\n";
    assert!(!robots_allowed(rules, "https://fragment.com/username/bob"));
    assert!(robots_allowed(rules, "https://fragment.com/username/alice"));
    assert!(robots_allowed("", "https://fragment.com/username/bob"));
}
