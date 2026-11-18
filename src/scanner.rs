use crate::{
    app::matches_name,
    classify::Lexicon,
    client::{catalog_urls, FragmentClient},
    config::Settings,
    model::Listing,
};
use anyhow::Result;
use reqwest::Url;
use std::collections::{HashMap, HashSet};

pub fn batch_urls(settings: &Settings, dictionary_queries: &[String]) -> Vec<Url> {
    let mut queries: Vec<Option<&str>> = if settings.scan.queries.is_empty() {
        vec![None]
    } else {
        settings
            .scan
            .queries
            .iter()
            .map(|q| Some(q.as_str()))
            .collect()
    };
    queries.extend(
        dictionary_queries
            .iter()
            .take(settings.scan.dictionary_queries)
            .map(|q| Some(q.as_str())),
    );
    let mut seen = HashSet::new();
    let mut urls = Vec::new();
    let mut push = |query, sort, market| {
        for url in catalog_urls(market, sort, query) {
            if urls.len() >= settings.scan.max_catalog_requests {
                return;
            }
            if seen.insert(url.to_string()) {
                urls.push(url);
            }
        }
    };
    if settings.scan.queries.is_empty() {
        for sort in &settings.scan.sorts {
            for market in &settings.scan.markets {
                push(None, *sort, *market);
            }
        }
    }
    for sort in &settings.scan.sorts {
        for market in &settings.scan.markets {
            for query in queries.iter().flatten() {
                push(Some(*query), *sort, *market);
            }
        }
    }
    urls
}
pub trait Transport {
    fn catalog(&mut self, url: &Url) -> Result<Vec<Listing>>;
    fn detail(&mut self, name: &str) -> Result<Listing>;
    fn requests(&self) -> usize;
}
impl Transport for FragmentClient {
    fn catalog(&mut self, url: &Url) -> Result<Vec<Listing>> {
        self.catalog(url)
    }
    fn detail(&mut self, name: &str) -> Result<Listing> {
        self.detail(name)
    }
    fn requests(&self) -> usize {
        self.requests
    }
}
#[derive(Default)]
pub struct ScanOutcome {
    pub items: Vec<Listing>,
    pub requests: usize,
    pub batches: usize,
    pub hidden: usize,
    pub error: Option<String>,
}

pub fn scan(
    client: &mut impl Transport,
    settings: &Settings,
    lexicon: &Lexicon,
    urls: &[Url],
    mut log: impl FnMut(&str),
) -> ScanOutcome {
    let mut result = ScanOutcome::default();
    let mut seen = HashMap::<String, usize>::new();
    for (i, url) in urls
        .iter()
        .take(settings.scan.max_catalog_requests)
        .enumerate()
    {
        log(&format!(
            "Batch {}/{} | catalogue request",
            i + 1,
            urls.len()
        ));
        let rows = match client.catalog(url) {
            Ok(rows) => rows,
            Err(e) => {
                result.error = Some(format!("{e:#}"));
                break;
            }
        };
        result.batches += 1;
        for mut row in rows {
            if let Err(e) = crate::app::prepare(&mut row, settings, lexicon) {
                result.error = Some(format!("Dictionary lookup failed: {e:#}"));
                break;
            }
            if let Some(index) = seen.get(&row.username) {
                result.items[*index] = row;
            } else {
                seen.insert(row.username.clone(), result.items.len());
                result.items.push(row);
            }
        }
        if result.error.is_some() {
            break;
        }
    }
    result.hidden = result
        .items
        .iter()
        .filter(|i| !matches_name(i, settings))
        .count();
    if result.error.is_none() {
        result
            .items
            .sort_by(|a, b| crate::app::compare(a, b, settings));
        let mut checked = 0;
        for row in &mut result.items {
            if checked >= settings.details {
                break;
            }
            if !matches_name(row, settings)
                || (!settings.statuses.is_empty() && !settings.statuses.contains(&row.status))
                || (row.action_price().is_some()
                    && !row.within_budget(settings.max_ton, settings.reserve_ton))
            {
                continue;
            }
            log(&format!(
                "Detail {}/{} | @{}",
                checked + 1,
                settings.details,
                row.username
            ));
            match client.detail(&row.username) {
                Ok(mut detail) => {
                    if let Err(e) = crate::app::prepare(&mut detail, settings, lexicon) {
                        result.error = Some(format!("{e:#}"));
                        break;
                    }
                    *row = detail;
                }
                Err(e) => {
                    result.error = Some(format!("{e:#}"));
                    break;
                }
            }
            checked += 1;
        }
    }
    result.requests = client.requests();
    result
}
