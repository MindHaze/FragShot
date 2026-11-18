use crate::model::{normalize_username, Listing, Status, Ton};
use anyhow::{ensure, Context, Result};
use scraper::{ElementRef, Html, Selector};

fn selector(css: &str) -> Selector {
    Selector::parse(css).expect("internal CSS selector")
}
fn first<'a>(root: ElementRef<'a>, css: &str) -> Option<ElementRef<'a>> {
    root.select(&selector(css)).next()
}
fn text(root: ElementRef<'_>) -> String {
    root.text()
        .collect::<String>()
        .chars()
        .filter(|c| !c.is_control() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
fn timestamp(root: ElementRef<'_>, css: &str) -> Option<i64> {
    let time = first(root, css)?.value().attr("datetime")?;
    chrono::DateTime::parse_from_rfc3339(time)
        .ok()
        .map(|date| date.timestamp())
}

pub fn parse_html_ton(input: &str) -> Result<Ton> {
    let input = input.trim();
    let (whole, fraction) = input
        .split_once('.')
        .map_or((input, None), |(a, b)| (a, Some(b)));
    let grouped = whole.replace(['\u{a0}', '\u{202f}', ' '], ",");
    if grouped.contains(',') {
        let parts: Vec<_> = grouped.split(',').collect();
        ensure!(
            (1..=3).contains(&parts[0].len()) && parts.iter().skip(1).all(|part| part.len() == 3),
            "Ambiguous TON grouping: {input}"
        );
    }
    let mut normalized = grouped.replace(',', "");
    if let Some(fraction) = fraction {
        normalized.push('.');
        normalized.push_str(fraction);
    }
    normalized.parse()
}

fn action(root: ElementRef<'_>, css: &str) -> Option<(Ton, String)> {
    let button = root.select(&selector(css)).find(|node| {
        std::iter::once(*node)
            .chain(node.ancestors().filter_map(ElementRef::wrap))
            .all(|element| {
                let value = element.value();
                let style: String = value
                    .attr("style")
                    .unwrap_or_default()
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect::<String>()
                    .to_ascii_lowercase();
                value.attr("disabled").is_none()
                    && value.attr("hidden").is_none()
                    && value.attr("aria-disabled") != Some("true")
                    && value.attr("aria-hidden") != Some("true")
                    && !value
                        .classes()
                        .any(|c| matches!(c, "disabled" | "hide" | "hidden"))
                    && !style.contains("display:none")
                    && !style.contains("visibility:hidden")
            })
    })?;
    let amount = button
        .value()
        .attr("data-bid-amount")?
        .parse::<Ton>()
        .ok()?;
    (amount.0 > 0).then(|| (amount, text(button).to_ascii_lowercase()))
}

pub fn parse_detail(html: &str, name: &str, now: i64) -> Result<Listing> {
    let name = normalize_username(name)?;
    let mut item = Listing::unknown(&name);
    let document = Html::parse_document(html);
    let Some(section) = document
        .select(&selector("section.tm-auction-section"))
        .next()
    else {
        item.evidence = "No listing recognised; absence does not imply availability".into();
        return Ok(item);
    };
    let actual = first(section, "h2 .subdomain")
        .map(text)
        .context("Listing identity element missing")?;
    ensure!(
        normalize_username(&actual)? == name,
        "Fragment returned a different listing than @{name}"
    );
    let label = first(section, ".tm-section-header-status")
        .map(text)
        .unwrap_or_default();
    let bid = action(section, "button.js-place-bid-btn");
    let buy = action(section, "button.js-buy-now-btn");
    item.evidence = format!("Fragment listing: {label}");
    match label.to_ascii_lowercase().as_str() {
        "for sale" => {
            item.status = Status::Sale;
            item.buy_now = buy.map(|(amount, _)| amount);
        }
        "on auction" => {
            item.ends_at = timestamp(section, "time.tm-countdown-timer[datetime]");
            if item.ends_at.is_some_and(|end| end > now) {
                item.status = Status::Auction;
                item.minimum_bid = bid.map(|(amount, _)| amount);
                item.buy_now = buy.map(|(amount, _)| amount);
            } else {
                item.evidence
                    .push_str(" ; deadline reached or unknown, actions not confirmed");
            }
            for table in section.select(&selector(".tm-section-bid-info table")) {
                let headers: Vec<_> = table.select(&selector("thead th")).map(text).collect();
                let cells: Vec<_> = table.select(&selector("tbody td")).collect();
                for (header, cell) in headers.iter().zip(cells) {
                    if header.eq_ignore_ascii_case("Highest Bid") {
                        item.last_bid =
                            first(cell, ".icon-ton").and_then(|n| parse_html_ton(&text(n)).ok());
                    }
                }
            }
        }
        "available" => {
            if let Some((amount, label)) = bid.filter(|(_, label)| label.contains("start auction"))
            {
                item.status = Status::StartAuction;
                item.start_bid = Some(amount);
                item.evidence.push_str(&format!(" ; {label}"));
            } else {
                item.evidence
                    .push_str(" ; no confirmed start-auction action");
            }
        }
        "sold" => item.status = Status::Sold,
        "unavailable" | "taken" => item.status = Status::Unavailable,
        _ => item.evidence.push_str(" ; status not recognised"),
    }
    Ok(item)
}

pub fn parse_catalog(html: &str, now: i64) -> Result<Vec<Listing>> {
    let document = Html::parse_document(html);
    let mut items = Vec::new();
    let mut recognized = document
        .select(&selector("section.js-search-results .tm-table-empty"))
        .any(|node| text(node) == "Auctions not found.");
    for table in document.select(&selector("table")) {
        let headings: Vec<_> = table
            .select(&selector("thead th"))
            .map(|node| text(node).to_ascii_lowercase())
            .collect();
        let auction_table = headings.iter().any(|h| h == "minimum bid")
            && headings.iter().any(|h| h.contains("auction ends"));
        recognized |= headings.iter().any(|h| h == "username");
        for row in table.select(&selector("tr.tm-row-selectable")) {
            let Some(link) = first(row, "a[href^='/username/']") else {
                continue;
            };
            let Some(name) = link
                .value()
                .attr("href")
                .and_then(|href| href.strip_prefix("/username/"))
                .and_then(|name| normalize_username(name).ok())
            else {
                continue;
            };
            recognized = true;
            let mut item = Listing::unknown(&name);
            let labels: Vec<_> = row
                .select(&selector(".tm-status-avail, .tm-status-unavail"))
                .map(|n| text(n).to_ascii_lowercase())
                .collect();
            let has = |needle: &str| labels.iter().any(|label| label == needle);
            let price = first(row, "td:nth-child(2) .icon-ton")
                .and_then(|n| parse_html_ton(&text(n)).ok())
                .filter(|p| p.0 > 0);
            item.evidence =
                "Fragment catalogue; price should be re-confirmed on the detail page".into();
            if has("sold") {
                item.status = Status::Sold;
            } else if has("unavailable") || has("taken") {
                item.status = Status::Unavailable;
            } else if has("for sale") {
                item.status = Status::Sale;
                item.buy_now = price;
            } else if has("available") {
                item.evidence =
                    "Catalogue Available; start-auction must be confirmed on the detail page"
                        .into();
            } else if has("on auction") || auction_table {
                item.ends_at = timestamp(row, "time[datetime]");
                if item.ends_at.is_some_and(|end| end > now) {
                    item.status = Status::Auction;
                    item.minimum_bid = price;
                }
            }
            items.push(item);
        }
    }
    ensure!(recognized, "Catalogue not recognised (modified HTML, empty search, or block page); no availability inferred");
    Ok(items)
}
