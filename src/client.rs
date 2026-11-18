use crate::{
    config::{Market, Settings, Sort},
    fragment,
    model::{normalize_username, Listing},
};
use anyhow::{bail, ensure, Context, Result};
use reqwest::{blocking::Client, redirect::Policy, Url};
use std::{
    io::Read,
    thread,
    time::{Duration, Instant},
};

const MAX_BODY: u64 = 8 * 1024 * 1024;
const AGENT: &str = "FragShot";

pub fn catalog_urls(market: Market, sort: Sort, query: Option<&str>) -> Vec<Url> {
    let sort = match sort {
        Sort::PriceAsc => "price_asc",
        Sort::PriceDesc => "price_desc",
        Sort::Recent => "listed",
        Sort::Ending => "ending",
    };
    let filters = match (market, query) {
        (Market::All, Some(_)) => vec![None],
        (Market::All, None) => vec![Some("auction"), Some("sale")],
        (Market::Auction, _) => vec![Some("auction")],
        (Market::Sale, _) => vec![Some("sale")],
    };
    filters
        .into_iter()
        .map(|filter| {
            let mut url = Url::parse("https://fragment.com/").expect("internal URL");
            url.query_pairs_mut().append_pair("sort", sort);
            if let Some(filter) = filter {
                url.query_pairs_mut().append_pair("filter", filter);
            }
            if let Some(query) = query {
                url.query_pairs_mut().append_pair("query", query);
            }
            url
        })
        .collect()
}

pub fn inspect_response(status: u16, body: &str) -> Result<Option<String>> {
    match status {
        404 => return Ok(None),
        403 => bail!("Fragment denied access (HTTP 403). Stopping; no bypass attempted."),
        429 => {
            bail!("Fragment rate-limited (HTTP 429). Stopping; retry later with a larger delay.")
        }
        200 => {}
        _ => bail!("HTTP {status} response; username status cannot be determined"),
    }
    let lower = body.to_ascii_lowercase();
    ensure!(
        ![
            "challenge-platform",
            "cf-chl-",
            "<title>just a moment",
            "id=\"captcha\"",
            "id='captcha'"
        ]
        .iter()
        .any(|marker| lower.contains(marker)),
        "Verification/CAPTCHA page detected; stopping without bypass"
    );
    Ok(Some(body.to_string()))
}

pub fn robots_allowed(robots: &str, url: &str) -> bool {
    robotstxt::DefaultMatcher::default().one_agent_allowed_by_robots(robots, AGENT, url)
}

pub struct FragmentClient {
    client: Client,
    delay: Duration,
    last_request: Option<Instant>,
    robots: String,
    pub requests: usize,
}
impl FragmentClient {
    pub fn new(settings: &Settings) -> Result<Self> {
        let client = Client::builder()
            .user_agent("FragShot/0.1 (read-only username research)")
            .https_only(true)
            .timeout(Duration::from_secs(settings.timeout_seconds))
            .redirect(Policy::custom(|attempt| {
                if attempt.previous().len() >= 5 {
                    return attempt.error("Too many redirects");
                }
                if attempt.url().scheme() == "https"
                    && attempt.url().host_str() == Some("fragment.com")
                {
                    attempt.follow()
                } else {
                    attempt.error("External redirect refused")
                }
            }))
            .build()?;
        let mut this = Self {
            client,
            delay: Duration::from_millis(settings.delay_ms),
            last_request: None,
            robots: String::new(),
            requests: 0,
        };
        this.robots = this
            .request(&Url::parse("https://fragment.com/robots.txt")?)?
            .unwrap_or_default();
        Ok(this)
    }
    fn request(&mut self, url: &Url) -> Result<Option<String>> {
        if let Some(last) = self.last_request {
            if let Some(wait) = self.delay.checked_sub(last.elapsed()) {
                thread::sleep(wait);
            }
        }
        self.last_request = Some(Instant::now());
        self.requests += 1;
        let response = self
            .client
            .get(url.clone())
            .send()
            .with_context(|| format!("Request failed: {url}"))?;
        let status = response.status().as_u16();
        if status != 200 {
            return inspect_response(status, "");
        }
        ensure!(
            response.content_length().unwrap_or(0) <= MAX_BODY,
            "Response body too large"
        );
        let mut bytes = Vec::new();
        response.take(MAX_BODY + 1).read_to_end(&mut bytes)?;
        ensure!(bytes.len() as u64 <= MAX_BODY, "Response body too large");
        inspect_response(
            status,
            &String::from_utf8(bytes).context("Response is not valid UTF-8")?,
        )
    }
    fn get(&mut self, url: &Url) -> Result<Option<String>> {
        ensure!(
            robots_allowed(&self.robots, url.as_str()),
            "robots.txt disallows fetching {url}"
        );
        self.request(url)
    }
    pub fn catalog(&mut self, url: &Url) -> Result<Vec<Listing>> {
        ensure!(
            url.scheme() == "https" && url.host_str() == Some("fragment.com") && url.path() == "/",
            "Catalogue URL refused"
        );
        let html = self.get(url)?.context("Catalogue not found (HTTP 404)")?;
        fragment::parse_catalog(&html, chrono::Utc::now().timestamp())
    }
    pub fn detail(&mut self, name: &str) -> Result<Listing> {
        let name = normalize_username(name)?;
        let url = Url::parse(&format!("https://fragment.com/username/{name}"))?;
        match self.get(&url)? {
            Some(html) => fragment::parse_detail(&html, &name, chrono::Utc::now().timestamp()),
            None => {
                let mut item = Listing::unknown(&name);
                item.evidence = "HTTP 404; availability not determined".into();
                Ok(item)
            }
        }
    }
}
