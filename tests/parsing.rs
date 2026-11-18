// Synthetic fixtures based on public selectors observed 2026-11-18.
// No amounts in this file represent real offers.
use fragshot::{
    fragment::{parse_catalog, parse_detail, parse_html_ton},
    model::{Status, Ton},
};
const NOW: i64 = 1_700_000_000;
fn detail(status: &str, body: &str) -> String {
    format!(
        r#"<section class="tm-auction-section"><h2><span class="subdomain">alice</span><span class="tm-section-header-status">{status}</span></h2>{body}</section>"#
    )
}
#[test]
fn auction_reads_next_bid_not_the_highest_bid_or_history() {
    let html = detail(
        "On auction",
        r#"
        <div class="tm-section-bid-info"><table><thead><tr><th>Highest Bid</th><th>Bid Step</th><th>Minimum Bid</th></tr></thead><tbody><tr>
        <td><div class="icon-ton">100</div></td><td><div class="icon-ton">5</div></td><td><div class="icon-ton">105</div></td></tr></tbody></table></div>
        <time class="tm-countdown-timer" datetime="2099-01-01T00:00:00+00:00"></time>
        <button class="js-place-bid-btn" data-bid-amount="105">Place bid</button>
        <button class="js-buy-now-btn" data-bid-amount="250">Buy now</button>"#,
    );
    let row = parse_detail(
        &format!("{html}<div class='icon-ton'>1</div>"),
        "alice",
        NOW,
    )
    .unwrap();
    assert_eq!(row.status, Status::Auction);
    assert_eq!(row.last_bid, Some(Ton(100_000_000_000)));
    assert_eq!(row.minimum_bid, Some(Ton(105_000_000_000)));
    assert_eq!(row.buy_now, Some(Ton(250_000_000_000)));
    assert!(row.ends_at.unwrap() > NOW);
}
#[test]
fn available_requires_an_explicit_start_action() {
    let html = detail(
        "Available",
        r#"<button class="js-place-bid-btn" data-bid-amount="10.5">Place bid and start auction</button>"#,
    );
    let row = parse_detail(&html, "alice", NOW).unwrap();
    assert_eq!(row.status, Status::StartAuction);
    assert_eq!(row.start_bid, Some(Ton(10_500_000_000)));
    assert_eq!(
        parse_detail(&detail("Available", ""), "alice", NOW)
            .unwrap()
            .status,
        Status::Unknown
    );
}
#[test]
fn fixed_sale_is_not_confused_with_historical_sale() {
    let body = r#"<button class="js-buy-now-btn" data-bid-amount="12.000000001">Buy now</button>"#;
    let sale = parse_detail(&detail("For sale", body), "alice", NOW).unwrap();
    assert_eq!(sale.status, Status::Sale);
    assert_eq!(sale.buy_now, Some(Ton(12_000_000_001)));
    let sold = parse_detail(&detail("Sold", body), "alice", NOW).unwrap();
    assert_eq!(sold.status, Status::Sold);
    assert_eq!(sold.action_price(), None);
}
#[test]
fn unknown_pages_disabled_buttons_and_expired_auctions_are_not_offers() {
    assert_eq!(
        parse_detail("<h1>Search usernames</h1>", "alice", NOW)
            .unwrap()
            .status,
        Status::Unknown
    );
    let hidden = "<div class='popup hide'><button class='js-buy-now-btn' data-bid-amount='1'>Buy</button></div>";
    let row = parse_detail(
        &format!("{}{hidden}", detail("Unavailable", "")),
        "alice",
        NOW,
    )
    .unwrap();
    assert_eq!(row.status, Status::Unavailable);
    assert_eq!(row.buy_now, None);
    let expired = detail(
        "On auction",
        r#"<time class="tm-countdown-timer" datetime="2020-01-01T00:00:00Z"></time><button class="js-place-bid-btn" data-bid-amount="5">Place bid</button>"#,
    );
    assert_eq!(
        parse_detail(&expired, "alice", NOW).unwrap().action_price(),
        None
    );
    let disabled = detail(
        "For sale",
        r#"<button disabled class="js-buy-now-btn" data-bid-amount="5">Buy</button>"#,
    );
    assert_eq!(
        parse_detail(&disabled, "alice", NOW)
            .unwrap()
            .action_price(),
        None
    );
    assert!(parse_detail(&detail("Sold", ""), "bob", NOW).is_err());
}
#[test]
fn hidden_ancestors_cannot_supply_action_prices() {
    for attribute in [
        "class='popup hide'",
        "hidden",
        "aria-hidden='true'",
        "class='hidden'",
        "style='display: none'",
        "disabled",
    ] {
        let button = "<button class='js-buy-now-btn' data-bid-amount='1'>Buy</button>";
        let html = detail(
            "For sale",
            &format!("<div {attribute}><div>{button}</div></div>"),
        );
        assert_eq!(
            parse_detail(&html, "alice", NOW).unwrap().action_price(),
            None,
            "{attribute}"
        );
        let visible = "<button class='js-buy-now-btn' data-bid-amount='12'>Buy</button>";
        let html = detail(
            "For sale",
            &format!("<div {attribute}>{button}</div>{visible}"),
        );
        assert_eq!(
            parse_detail(&html, "alice", NOW).unwrap().buy_now,
            Some(Ton(12_000_000_000))
        );
    }
    let html = detail("Available", "<div hidden><button class='js-place-bid-btn' data-bid-amount='1'>Place bid and start auction</button></div>");
    assert_eq!(
        parse_detail(&html, "alice", NOW).unwrap().status,
        Status::Unknown
    );
}
#[test]
fn untrusted_status_text_cannot_inject_terminal_escapes() {
    let row = parse_detail(&detail("Unexpected\u{1b}[31m", ""), "alice", NOW).unwrap();
    assert!(!row.evidence.contains('\u{1b}'));
}
#[test]
fn html_amounts_accept_grouping_but_reject_ambiguous_numbers() {
    assert_eq!(
        parse_html_ton("1,234.000000001").unwrap(),
        Ton(1_234_000_000_001)
    );
    assert_eq!(
        parse_html_ton("1\u{a0}234").unwrap(),
        Ton(1_234_000_000_000)
    );
    assert!(parse_html_ton("12,5").is_err());
    assert!(parse_html_ton("~ $25").is_err());
}
#[test]
fn explicit_empty_search_is_not_a_block_or_an_availability_claim() {
    // Public empty-search markup observed on 2026-11-18.
    let html = r#"<section class="tm-section clearfix js-search-results">
        <div class="tm-section-header"><h2 class="tm-section-header-text">Search Results</h2></div>
        <div class="tm-table-empty">Auctions not found.</div></section>"#;
    assert!(parse_catalog(html, NOW).unwrap().is_empty());
    for invalid in [
        "<html>Auctions not found.</html>",
        "<section class='js-search-results'>Please verify access</section>",
        "<section class='js-search-results'><div class='tm-table-empty'>Access denied</div></section>",
    ] {
        assert!(parse_catalog(invalid, NOW).is_err());
    }
}

#[test]
fn catalogue_uses_status_and_headers_not_history() {
    let html = r#"<table><thead><tr><th>Username</th><th>Minimum bid</th><th>Auction ends in</th></tr></thead><tbody>
    <tr class="tm-row-selectable"><td><a href="/username/alice">@alice</a></td><td><div class="table-cell-value icon-ton">10</div></td><td><time datetime="2099-01-01T00:00:00Z"></time></td></tr>
    <tr class="tm-row-selectable"><td><a href="/username/paris">@paris</a></td><td><div class="table-cell-value icon-ton">20</div></td><td><div class="tm-status-avail">For sale</div></td></tr>
    <tr class="tm-row-selectable"><td><a href="/username/hello">@hello</a></td><td><div class="table-cell-value icon-ton">1</div></td><td><div class="tm-status-unavail">Sold</div><time datetime="2020-01-01T00:00:00Z"></time></td></tr>
    <tr class="tm-row-selectable"><td><a href="/username/winter">@winter</a></td><td><div class="table-cell-value icon-ton">7</div></td><td><div class="tm-status-avail">Available</div></td></tr>
    <tr class="tm-row-selectable"><td><a href="/number/1234">1234</a></td></tr>
    </tbody></table>"#;
    let rows = parse_catalog(html, NOW).unwrap();
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[0].status, Status::Auction);
    assert_eq!(rows[0].minimum_bid, Some(Ton(10_000_000_000)));
    assert_eq!(rows[1].buy_now, Some(Ton(20_000_000_000)));
    assert_eq!(rows[2].status, Status::Sold);
    assert_eq!(rows[2].action_price(), None);
    assert_eq!(rows[3].status, Status::Unknown);
    assert!(parse_catalog("<html>captcha</html>", NOW).is_err());
}
