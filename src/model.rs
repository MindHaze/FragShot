use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

/// Exact amount, never a float. 1 TON = 1_000_000_000 nanoTON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Ton(pub u64);

impl FromStr for Ton {
    type Err = anyhow::Error;
    fn from_str(input: &str) -> Result<Self> {
        let input = input.trim();
        let (whole, fraction) = match input.split_once('.') {
            Some((whole, fraction)) => {
                ensure!(!fraction.is_empty(), "Decimal digits missing after point");
                (whole, fraction)
            }
            None => (input, ""),
        };
        ensure!(
            !whole.is_empty() && whole.bytes().all(|c| c.is_ascii_digit()),
            "Invalid TON amount: use a non-negative number with an optional decimal point"
        );
        ensure!(
            fraction.len() <= 9 && fraction.bytes().all(|c| c.is_ascii_digit()),
            "TON amounts support at most 9 decimal places"
        );
        let whole = whole.parse::<u64>().context("Amount too large")?;
        let fractional = if fraction.is_empty() {
            0
        } else {
            fraction.parse::<u64>()? * 10u64.pow(9 - fraction.len() as u32)
        };
        Ok(Self(
            whole
                .checked_mul(1_000_000_000)
                .and_then(|n| n.checked_add(fractional))
                .context("TON amount overflows u64")?,
        ))
    }
}
impl fmt::Display for Ton {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let whole = self.0 / 1_000_000_000;
        let fractional = self.0 % 1_000_000_000;
        if fractional == 0 {
            write!(f, "{whole}")
        } else {
            write!(
                f,
                "{whole}.{}",
                format!("{fractional:09}").trim_end_matches('0')
            )
        }
    }
}
impl TryFrom<String> for Ton {
    type Error = anyhow::Error;
    fn try_from(value: String) -> Result<Self> {
        value.parse()
    }
}
impl From<Ton> for String {
    fn from(value: Ton) -> Self {
        value.to_string()
    }
}

pub fn normalize_username(input: &str) -> Result<String> {
    let name = input.trim().strip_prefix('@').unwrap_or(input.trim());
    if !(3..=32).contains(&name.len())
        || !name.as_bytes()[0].is_ascii_alphabetic()
        || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
    {
        bail!("Invalid username {input:?}: must be 3-32 ASCII characters, starting with a letter, containing only letters/digits/underscore");
    }
    Ok(name.to_ascii_lowercase())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Sale,
    Auction,
    StartAuction,
    Sold,
    Unavailable,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    Og,
    SemiOg,
    NonOg,
}
impl Tier {
    pub fn key(self) -> &'static str {
        match self {
            Self::Og => "og",
            Self::SemiOg => "semi_og",
            Self::NonOg => "non_og",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Og => "OG",
            Self::SemiOg => "SEMI-OG",
            Self::NonOg => "NON-OG",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LexicalAnalysis {
    pub tier: Tier,
    pub original: String,
    pub language: String,
    pub categories: Vec<String>,
    pub base: Option<String>,
    pub rule: String,
    pub source: String,
    pub normalization: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrimaryClassification {
    pub analysis_index: usize,
    pub language: String,
    pub category: String,
    pub tier: Tier,
    pub frequency_rank: Option<usize>,
    pub root_frequency_rank: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Listing {
    pub username: String,
    pub status: Status,
    pub buy_now: Option<Ton>,
    pub minimum_bid: Option<Ton>,
    pub start_bid: Option<Ton>,
    pub last_bid: Option<Ton>,
    pub bid_count: Option<u64>,
    pub ends_at: Option<i64>,
    pub categories: Vec<String>,
    #[serde(default)]
    pub analyses: Vec<LexicalAnalysis>,
    #[serde(default)]
    pub primary: Option<PrimaryClassification>,
    #[serde(default)]
    pub evaluation: Option<crate::scoring::Evaluation>,
    pub url: String,
    pub evidence: String,
}
impl Listing {
    pub fn unknown(username: &str) -> Self {
        Self {
            username: username.to_string(),
            status: Status::Unknown,
            buy_now: None,
            minimum_bid: None,
            start_bid: None,
            last_bid: None,
            bid_count: None,
            ends_at: None,
            categories: vec![],
            analyses: vec![],
            primary: None,
            evaluation: None,
            url: format!("https://fragment.com/username/{username}"),
            evidence: String::new(),
        }
    }
    pub fn action_price(&self) -> Option<Ton> {
        let active = self.status != Status::Auction
            || self
                .ends_at
                .is_none_or(|end| end > chrono::Utc::now().timestamp());
        if !active {
            return None;
        }
        match self.status {
            Status::Sale => self.buy_now.filter(|price| price.0 > 0),
            Status::Auction => self
                .minimum_bid
                .into_iter()
                .chain(self.buy_now)
                .filter(|price| price.0 > 0)
                .min(),
            Status::StartAuction => self.start_bid.filter(|price| price.0 > 0),
            _ => None,
        }
    }
    pub fn within_budget(&self, max: Ton, reserve: Ton) -> bool {
        self.action_price()
            .and_then(|p| p.0.checked_add(reserve.0))
            .is_some_and(|total| total <= max.0)
    }
}
