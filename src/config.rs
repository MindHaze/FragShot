use crate::model::{Status, Ton};
use anyhow::{ensure, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Parser)]
#[command(
    name = "fragshot",
    version,
    about = "Multilingual OG username scout for Fragment -- read-only, YAML driven"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,
    #[arg(long, global = true)]
    pub max_ton: Option<Ton>,
    #[arg(long, global = true)]
    pub reserve_ton: Option<Ton>,
    #[arg(long = "category", global = true, value_delimiter = ',')]
    pub categories: Vec<String>,
    #[arg(long = "status", global = true, value_delimiter = ',')]
    pub statuses: Vec<Status>,
    #[arg(long = "language", global = true, value_delimiter = ',')]
    pub languages: Vec<String>,
    #[arg(long, global = true)]
    pub min_length: Option<usize>,
    #[arg(long, global = true)]
    pub max_length: Option<usize>,
    /// Maximum displayed scan results or checked usernames.
    #[arg(long, global = true)]
    pub limit: Option<usize>,
    /// Maximum scan detail requests.
    #[arg(long, global = true)]
    pub details: Option<usize>,
    #[arg(long, global = true)]
    pub delay_ms: Option<u64>,
    #[arg(long, global = true)]
    pub timeout_seconds: Option<u64>,
    #[arg(long, global = true)]
    pub lexicon: Option<PathBuf>,
    /// Strict OG only; otherwise the default includes semi-OG.
    #[arg(long, global = true, num_args = 0..=1, default_missing_value = "true", require_equals = true)]
    pub og_only: Option<bool>,
    #[arg(long, global = true, num_args = 0..=1, default_missing_value = "true", require_equals = true)]
    pub show_unavailable: Option<bool>,
    #[arg(long, global = true, num_args = 0..=1, default_missing_value = "true", require_equals = true)]
    pub show_non_og: Option<bool>,
    #[arg(long, global = true, num_args = 0..=1, default_missing_value = "true", require_equals = true)]
    pub save_results: Option<bool>,
    #[arg(long, global = true, value_enum)]
    pub color: Option<Color>,
    /// Local result ranking, separate from Fragment catalogue --sort.
    #[arg(long, global = true, value_enum)]
    pub rank_by: Option<RankingSort>,
    #[arg(long, global = true)]
    pub json: Option<PathBuf>,
    #[arg(long, global = true)]
    pub csv: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Scan configured catalogue/search batches (not fictitious numbered pages).
    Scan {
        #[arg(long, value_enum)]
        market: Option<Market>,
        #[arg(long, value_enum)]
        sort: Option<Sort>,
        #[arg(long)]
        query: Option<String>,
        #[arg(long)]
        batches: Option<usize>,
    },
    /// Check specific names, a file, or a bounded dictionary sample.
    Check {
        names: Vec<String>,
        #[arg(long)]
        file: Option<PathBuf>,
        #[arg(long, num_args = 0..=1, default_missing_value = "true", require_equals = true)]
        dictionary: Option<bool>,
    },
    /// SIMULATED offline examples; never real offers.
    Demo,
    /// Install or inspect the local multilingual dictionary.
    Dictionary {
        #[command(subcommand)]
        action: DictionaryCommand,
    },
}
#[derive(Debug, Subcommand)]
pub enum DictionaryCommand {
    /// Download and stream the configured large dictionary (several GB).
    Sync,
    /// Import local Kaikki JSONL or JSONL.gz data.
    Import { file: PathBuf },
    /// Print actual installed coverage without network access.
    Status,
}

macro_rules! config_enum {
    ($name:ident { $($variant:ident),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, ValueEnum)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($variant),+ }
    };
}
config_enum!(Color {
    Auto,
    Always,
    Never
});
config_enum!(Market { All, Auction, Sale });
config_enum!(Sort {
    PriceAsc,
    PriceDesc,
    Recent,
    Ending
});
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum RunMode {
    #[default]
    Scan,
    Check,
    Demo,
    DictionarySync,
    DictionaryImport,
    DictionaryStatus,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ScanSettings {
    pub markets: Vec<Market>,
    pub sorts: Vec<Sort>,
    pub queries: Vec<String>,
    pub max_catalog_requests: usize,
    pub max_detail_requests: usize,
    pub max_results: usize,
    pub dictionary_queries: usize,
}
impl Default for ScanSettings {
    fn default() -> Self {
        Self {
            markets: vec![Market::All],
            sorts: vec![Sort::PriceAsc, Sort::Recent, Sort::Ending],
            queries: vec![],
            max_catalog_requests: 32,
            max_detail_requests: 30,
            max_results: 100,
            dictionary_queries: 24,
        }
    }
}
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CheckSettings {
    pub names: Vec<String>,
    pub file: Option<PathBuf>,
    pub dictionary: bool,
    pub max_names: usize,
}
impl Default for CheckSettings {
    fn default() -> Self {
        Self {
            names: vec![],
            file: None,
            dictionary: false,
            max_names: 50,
        }
    }
}
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MatchingSettings {
    pub tiers: Vec<String>,
    pub show_non_og: bool,
    pub languages: Vec<String>,
    pub categories: Vec<String>,
    pub include_offensive: bool,
    pub min_length: usize,
    pub max_length: usize,
    pub suffixes: Vec<String>,
    pub min_root_length: usize,
    pub overrides: std::collections::BTreeMap<String, LocalOverride>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalOverride {
    pub tier: crate::model::Tier,
    #[serde(default)]
    pub base: Option<String>,
    pub language: String,
    pub categories: Vec<String>,
}
impl Default for MatchingSettings {
    fn default() -> Self {
        Self {
            tiers: vec!["og".into(), "semi_og".into()],
            show_non_og: false,
            languages: ["en", "fr", "ru", "de", "es", "it", "pt"]
                .map(String::from)
                .to_vec(),
            categories: vec![],
            include_offensive: true,
            min_length: 3,
            max_length: 32,
            suffixes: ["ers", "ing", "es", "er", "ed", "ly", "s"]
                .map(String::from)
                .to_vec(),
            min_root_length: 4,
            overrides: Default::default(),
        }
    }
}
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DictionarySettings {
    pub directory: PathBuf,
    pub input: Option<PathBuf>,
    pub source_url: String,
    pub max_compressed_bytes: u64,
    pub max_uncompressed_bytes: u64,
    pub max_record_bytes: usize,
    pub timeout_seconds: u64,
    pub transliterate: bool,
    pub frequency_urls: Vec<String>,
}
impl Default for DictionarySettings {
    fn default() -> Self {
        Self {
            directory: "data/dictionary".into(),
            input: None,
            source_url: "https://kaikki.org/dictionary/raw-wiktextract-data.jsonl.gz".into(),
            max_compressed_bytes: 4 * 1024 * 1024 * 1024,
            max_uncompressed_bytes: 40 * 1024 * 1024 * 1024,
            max_record_bytes: 4 * 1024 * 1024,
            timeout_seconds: 7200,
            transliterate: true,
            frequency_urls: vec![],
        }
    }
}
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConsoleSettings {
    pub color: Color,
    pub banner: bool,
    pub progress: bool,
}
impl Default for ConsoleSettings {
    fn default() -> Self {
        Self {
            color: Color::Auto,
            banner: true,
            progress: true,
        }
    }
}
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AlertSettings {
    pub console_matches: bool,
    pub bell: bool,
}
impl Default for AlertSettings {
    fn default() -> Self {
        Self {
            console_matches: true,
            bell: false,
        }
    }
}
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ResultSettings {
    pub enabled: bool,
    pub directory: PathBuf,
    pub grouping: Vec<String>,
    pub json: Option<PathBuf>,
    pub csv: Option<PathBuf>,
}
impl Default for ResultSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            directory: "results".into(),
            grouping: vec!["language".into(), "category".into()],
            json: None,
            csv: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct ScoringSettings {
    pub og_reference_ton: Ton,
    pub semi_og_reference_ton: Ton,
}
impl Default for ScoringSettings {
    fn default() -> Self {
        Self {
            og_reference_ton: Ton(100_000_000_000),
            semi_og_reference_ton: Ton(25_000_000_000),
        }
    }
}
config_enum!(RankingSort {
    Interest,
    PriceAsc,
    NameQuality
});
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RankingSettings {
    pub sort: RankingSort,
}
impl Default for RankingSettings {
    fn default() -> Self {
        Self {
            sort: RankingSort::Interest,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub max_ton: Ton,
    pub reserve_ton: Ton,
    pub categories: Vec<String>,
    pub statuses: Vec<Status>,
    pub min_length: usize,
    pub max_length: usize,
    pub limit: usize,
    pub details: usize,
    pub delay_ms: u64,
    pub timeout_seconds: u64,
    pub og_only: bool,
    pub show_unavailable: bool,
    pub lexicon: Option<PathBuf>,
    #[serde(skip)]
    pub mode: RunMode,
    #[serde(skip)]
    pub scan: ScanSettings,
    #[serde(skip)]
    pub check: CheckSettings,
    #[serde(skip)]
    pub matching: MatchingSettings,
    #[serde(skip)]
    pub dictionary: DictionarySettings,
    #[serde(skip)]
    pub console: ConsoleSettings,
    #[serde(skip)]
    pub alerts: AlertSettings,
    #[serde(skip)]
    pub results: ResultSettings,
    pub scoring: ScoringSettings,
    pub ranking: RankingSettings,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            max_ton: Ton(100_000_000_000),
            reserve_ton: Ton(0),
            categories: vec![],
            statuses: vec![],
            min_length: 3,
            max_length: 32,
            limit: 100,
            details: 30,
            delay_ms: 1500,
            timeout_seconds: 20,
            og_only: false,
            show_unavailable: false,
            lexicon: None,
            mode: RunMode::Scan,
            scan: ScanSettings::default(),
            check: CheckSettings::default(),
            matching: MatchingSettings::default(),
            dictionary: DictionarySettings::default(),
            console: ConsoleSettings::default(),
            alerts: AlertSettings::default(),
            results: ResultSettings::default(),
            scoring: ScoringSettings::default(),
            ranking: RankingSettings::default(),
        }
    }
}
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct YamlConfig {
    run: RunSection,
    budget: BudgetSection,
    scan: ScanSettings,
    check: CheckSettings,
    matching: MatchingSettings,
    dictionary: DictionarySettings,
    network: NetworkSection,
    console: ConsoleSettings,
    alerts: AlertSettings,
    results: ResultSettings,
    scoring: ScoringSettings,
    ranking: RankingSettings,
    lexicon: Option<PathBuf>,
}
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RunSection {
    mode: RunMode,
    statuses: Vec<Status>,
    show_unavailable: bool,
}
#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct BudgetSection {
    max_ton: Ton,
    reserve_ton: Ton,
}
impl Default for BudgetSection {
    fn default() -> Self {
        Self {
            max_ton: Ton(100_000_000_000),
            reserve_ton: Ton(0),
        }
    }
}
#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct NetworkSection {
    delay_ms: u64,
    timeout_seconds: u64,
}
impl Default for NetworkSection {
    fn default() -> Self {
        Self {
            delay_ms: 1500,
            timeout_seconds: 20,
        }
    }
}

fn relative(base: &Path, path: &mut PathBuf) {
    if path.is_relative() {
        *path = base.join(&*path);
    }
}
impl Settings {
    pub fn load(cli: &Cli) -> Result<Self> {
        let path = cli.config.clone().or_else(|| {
            let local = PathBuf::from("fragshot.yaml");
            if local.exists() {
                return Some(local);
            }
            std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(|p| p.join("fragshot.yaml")))
                .filter(|p| p.exists())
        });
        let mut settings = if let Some(path) = &path {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("Reading {}", path.display()))?;
            let text = text.trim_start_matches('\u{feff}');
            let yaml = path.extension().is_some_and(|e| e == "yaml" || e == "yml");
            let mut s: Self = if yaml {
                let value: serde_yaml_ng::Value =
                    serde_yaml_ng::from_str(text).context("Invalid YAML")?;
                for key in ["max_ton", "reserve_ton"] {
                    if let Some(v) = value.get("budget").and_then(|v| v.get(key)) {
                        ensure!(v.is_string(), "budget.{key} must be a quoted TON string");
                    }
                }
                for key in ["og_reference_ton", "semi_og_reference_ton"] {
                    if let Some(v) = value.get("scoring").and_then(|s| s.get(key)) {
                        ensure!(v.is_string(), "scoring.{key} must be a quoted TON string");
                    }
                }
                let y: YamlConfig =
                    serde_yaml_ng::from_value(value).context("Invalid YAML configuration")?;
                Self {
                    max_ton: y.budget.max_ton,
                    reserve_ton: y.budget.reserve_ton,
                    categories: y.matching.categories.clone(),
                    statuses: y.run.statuses,
                    min_length: y.matching.min_length,
                    max_length: y.matching.max_length,
                    limit: y.scan.max_results,
                    details: y.scan.max_detail_requests,
                    delay_ms: y.network.delay_ms,
                    timeout_seconds: y.network.timeout_seconds,
                    show_unavailable: y.run.show_unavailable,
                    lexicon: y.lexicon,
                    mode: y.run.mode,
                    scan: y.scan,
                    check: y.check,
                    matching: y.matching,
                    dictionary: y.dictionary,
                    console: y.console,
                    alerts: y.alerts,
                    results: y.results,
                    scoring: y.scoring,
                    ranking: y.ranking,
                    ..Self::default()
                }
            } else {
                eprintln!("Legacy TOML configuration: migrate to fragshot.yaml for complete operation settings.");
                toml::from_str(text).context("Invalid legacy TOML configuration")?
            };
            let base = path.parent().unwrap_or_else(|| Path::new("."));
            for p in [
                &mut s.lexicon,
                &mut s.check.file,
                &mut s.dictionary.input,
                &mut s.results.json,
                &mut s.results.csv,
            ]
            .into_iter()
            .flatten()
            {
                relative(base, p);
            }
            relative(base, &mut s.dictionary.directory);
            relative(base, &mut s.results.directory);
            s
        } else {
            Self::default()
        };
        if let Some(command) = &cli.command {
            match command {
                Command::Demo => settings.mode = RunMode::Demo,
                Command::Scan {
                    market,
                    sort,
                    query,
                    batches,
                } => {
                    settings.mode = RunMode::Scan;
                    if let Some(m) = market {
                        settings.scan.markets = vec![*m];
                    }
                    if let Some(s) = sort {
                        settings.scan.sorts = vec![*s];
                    }
                    if let Some(q) = query {
                        settings.scan.queries = vec![q.clone()];
                        settings.scan.dictionary_queries = 0;
                    }
                    if let Some(n) = batches {
                        settings.scan.max_catalog_requests = *n;
                    }
                }
                Command::Check {
                    names,
                    file,
                    dictionary,
                } => {
                    settings.mode = RunMode::Check;
                    if !names.is_empty() {
                        settings.check.names = names.clone();
                    }
                    if file.is_some() {
                        settings.check.file = file.clone();
                    }
                    if let Some(b) = dictionary {
                        settings.check.dictionary = *b;
                    }
                }
                Command::Dictionary { action } => match action {
                    DictionaryCommand::Sync => settings.mode = RunMode::DictionarySync,
                    DictionaryCommand::Status => settings.mode = RunMode::DictionaryStatus,
                    DictionaryCommand::Import { file } => {
                        settings.mode = RunMode::DictionaryImport;
                        settings.dictionary.input = Some(file.clone());
                    }
                },
            }
        }
        if settings.mode == RunMode::Check {
            settings.limit = settings.check.max_names;
        }
        macro_rules! apply { ($($f:ident),+) => { $(if let Some(v) = cli.$f { settings.$f = v; })+ }; }
        apply!(
            max_ton,
            reserve_ton,
            min_length,
            max_length,
            limit,
            details,
            delay_ms,
            timeout_seconds,
            og_only,
            show_unavailable
        );
        if !cli.categories.is_empty() {
            settings.categories = cli.categories.clone();
        }
        if !cli.statuses.is_empty() {
            settings.statuses = cli.statuses.clone();
        }
        if !cli.languages.is_empty() {
            settings.matching.languages = cli.languages.clone();
        }
        if let Some(v) = cli.show_non_og {
            settings.matching.show_non_og = v;
        }
        if let Some(v) = cli.save_results {
            settings.results.enabled = v;
        }
        if let Some(v) = cli.color {
            settings.console.color = v;
        }
        if cli.lexicon.is_some() {
            settings.lexicon = cli.lexicon.clone();
        }
        if cli.json.is_some() {
            settings.results.json = cli.json.clone();
        }
        if cli.csv.is_some() {
            settings.results.csv = cli.csv.clone();
        }
        if let Some(sort) = cli.rank_by {
            settings.ranking.sort = sort;
        }
        settings.validate()?;
        Ok(settings)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.scoring.og_reference_ton.0 > 0 && self.scoring.semi_og_reference_ton.0 > 0,
            "Scoring reference prices must be positive quoted TON strings"
        );
        ensure!(
            self.reserve_ton <= self.max_ton,
            "reserve-ton exceeds max-ton budget"
        );
        ensure!(
            (3..=32).contains(&self.min_length)
                && (self.min_length..=32).contains(&self.max_length),
            "Length: 3 <= min-length <= max-length <= 32"
        );
        ensure!(
            (1..=1000).contains(&self.limit),
            "limit must be between 1 and 1000"
        );
        ensure!(self.details <= 100, "details must be between 0 and 100");
        ensure!(
            (1000..=60000).contains(&self.delay_ms),
            "delay-ms must be between 1000 and 60000"
        );
        ensure!(
            (1..=120).contains(&self.timeout_seconds),
            "timeout-seconds must be between 1 and 120"
        );
        ensure!(
            self.categories
                .iter()
                .all(|c| crate::classify::CATEGORIES.contains(&c.as_str())),
            "Valid categories: {}",
            crate::classify::CATEGORIES.join(", ")
        );
        ensure!(
            !self.matching.tiers.is_empty()
                && self
                    .matching
                    .tiers
                    .iter()
                    .all(|t| ["og", "semi_og", "non_og"].contains(&t.as_str())),
            "matching.tiers: use og, semi_og, non_og"
        );
        ensure!(
            self.matching.languages.iter().all(|l| !l.is_empty()
                && l.len() <= 16
                && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')),
            "Invalid language code"
        );
        ensure!(
            (3..=16).contains(&self.matching.min_root_length)
                && self.matching.suffixes.len() <= 32
                && self.matching.suffixes.iter().all(|s| !s.is_empty()
                    && s.len() <= 6
                    && s.bytes().all(|c| c.is_ascii_lowercase())),
            "Invalid bounded suffix rules"
        );
        ensure!(
            !self.scan.markets.is_empty()
                && self.scan.markets.len() <= 3
                && !self.scan.sorts.is_empty()
                && self.scan.sorts.len() <= 4,
            "Configure 1-3 markets and 1-4 sorts"
        );
        ensure!(
            (1..=200).contains(&self.scan.max_catalog_requests)
                && self.scan.dictionary_queries <= 100
                && self.scan.queries.len() <= 200,
            "Scan batch/query limit exceeded"
        );
        ensure!(
            self.scan
                .queries
                .iter()
                .all(|q| !q.trim().is_empty() && q.len() <= 64 && !q.chars().any(char::is_control)),
            "Query: 1-64 characters without controls"
        );
        ensure!(
            self.results.grouping.len() <= 2
                && self
                    .results
                    .grouping
                    .iter()
                    .all(|g| ["language", "category"].contains(&g.as_str())),
            "results.grouping: language and/or category"
        );
        ensure!(
            self.dictionary.max_compressed_bytes > 0
                && self.dictionary.max_compressed_bytes <= 16 * 1024 * 1024 * 1024
                && self.dictionary.max_uncompressed_bytes > 0
                && self.dictionary.max_uncompressed_bytes <= 128 * 1024 * 1024 * 1024,
            "Invalid dictionary byte limits"
        );
        ensure!(
            (1024..=16 * 1024 * 1024).contains(&self.dictionary.max_record_bytes)
                && (1..=86400).contains(&self.dictionary.timeout_seconds),
            "Invalid dictionary record/timeout limit"
        );
        ensure!(
            self.matching.overrides.len() <= 10000,
            "At most 10000 local overrides are allowed"
        );
        for (name, value) in &self.matching.overrides {
            ensure!(
                crate::model::normalize_username(name)? == *name,
                "Override usernames must be lowercase without @"
            );
            ensure!(
                !value.language.is_empty()
                    && value.language.len() <= 16
                    && value
                        .language
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-'),
                "Invalid override language"
            );
            ensure!(
                !value.categories.is_empty()
                    && value
                        .categories
                        .iter()
                        .all(|c| crate::classify::CATEGORIES.contains(&c.as_str())),
                "Invalid override categories"
            );
            ensure!(
                value.tier != crate::model::Tier::SemiOg || value.base.is_some(),
                "Semi-OG overrides require a verified base"
            );
            if let Some(base) = &value.base {
                ensure!(
                    crate::model::normalize_username(base)? == *base && base != name,
                    "Invalid override base"
                );
            }
        }
        ensure!(
            self.dictionary.frequency_urls.len() <= 16,
            "At most 16 frequency sources are allowed"
        );
        for source in &self.dictionary.frequency_urls {
            crate::frequency::source_language(source)?;
        }
        ensure!(
            self.check.names.len() <= 10000 && (1..=1000).contains(&self.check.max_names),
            "Invalid check candidate limits"
        );
        Ok(())
    }
}
