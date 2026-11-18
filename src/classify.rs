use crate::{
    dictionary::DictionaryStore,
    model::{normalize_username, LexicalAnalysis, Listing, Tier},
};
use anyhow::{ensure, Context, Result};
use std::{collections::BTreeMap, path::Path};

pub const CATEGORIES: &[&str] = &[
    "first_name",
    "surname",
    "brand",
    "company",
    "video_game",
    "word",
    "city",
    "food",
    "slang",
    "profanity",
    "unknown",
    "animal",
    "nature",
    "space",
    "technology",
    "science",
    "body",
    "color",
    "time",
];

pub fn category_priority(category: &str) -> usize {
    [
        "body",
        "space",
        "animal",
        "nature",
        "technology",
        "science",
        "color",
        "time",
        "food",
        "city",
        "first_name",
        "video_game",
        "brand",
        "company",
        "surname",
        "word",
        "slang",
        "profanity",
        "unknown",
    ]
    .iter()
    .position(|c| *c == category)
    .unwrap_or(usize::MAX)
}

fn enrich_semantics(name: &str, analyses: &mut Vec<LexicalAnalysis>) {
    for line in include_str!("../data/semantics.tsv")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        let fields: Vec<_> = line.split('\t').collect();
        if fields.len() != 3 || fields[0] != name {
            continue;
        }
        let (language, category) = (fields[1], fields[2]);
        if let Some(root) = analyses
            .iter()
            .filter(|a| {
                a.language == language
                    || (a.language == "und" && a.source.starts_with("FragShot curated"))
            })
            .min_by_key(|a| {
                (
                    a.language != language,
                    a.tier != Tier::Og,
                    a.normalization != "exact",
                )
            })
        {
            let mut enriched = root.clone();
            enriched.language = language.into();
            enriched.categories.push(category.into());
            enriched.categories.sort();
            enriched.categories.dedup();
            enriched.source = format!(
                "FragShot curated semantics; lexical source: {}",
                root.source
            );
            enriched.rule = "curated semantic label on verified spelling".into();
            analyses.insert(0, enriched);
        }
    }
}
pub struct Lexicon {
    entries: BTreeMap<String, Vec<String>>,
    store: Option<DictionaryStore>,
    suffixes: Vec<String>,
    min_root: usize,
    overrides: BTreeMap<String, crate::config::LocalOverride>,
    frequency_ranks: BTreeMap<String, BTreeMap<String, usize>>,
}
impl Lexicon {
    pub fn built_in() -> Self {
        let mut lexicon = Self {
            entries: BTreeMap::new(),
            store: None,
            suffixes: ["ers", "ing", "es", "er", "ed", "ly", "s"]
                .map(String::from)
                .to_vec(),
            min_root: 4,
            overrides: BTreeMap::new(),
            frequency_ranks: BTreeMap::new(),
        };
        lexicon
            .extend_tsv(include_str!("../data/lexicon.tsv"))
            .expect("built-in lexicon validated by tests");
        lexicon.entries.insert("brain".into(), vec!["word".into()]);
        lexicon.entries.insert("flower".into(), vec!["word".into()]);
        lexicon
    }
    pub fn attach(&mut self, directory: &Path) -> Result<()> {
        self.store = DictionaryStore::open(directory)?;
        Ok(())
    }
    pub fn load_frequency(&mut self, languages: &[String]) -> Result<()> {
        self.frequency_ranks = self
            .store
            .as_ref()
            .map(|s| s.frequency_ranks(languages))
            .transpose()?
            .unwrap_or_default();
        Ok(())
    }
    pub fn frequency_rank(&self, name: &str, language: &str) -> Option<usize> {
        self.frequency_ranks
            .get(language)
            .and_then(|ranks| ranks.get(name))
            .copied()
    }
    pub fn configure(&mut self, suffixes: Vec<String>, min_root: usize) {
        self.suffixes = suffixes;
        self.min_root = min_root;
    }
    pub fn set_overrides(
        &mut self,
        overrides: BTreeMap<String, crate::config::LocalOverride>,
    ) -> Result<()> {
        for value in overrides.values().filter(|v| v.tier == Tier::SemiOg) {
            let base = value
                .base
                .as_deref()
                .context("Semi-OG override requires a base")?;
            let verified = if let Some(root) = overrides.get(base) {
                root.tier == Tier::Og && root.language == value.language
            } else {
                self.exact(base)?.iter().any(|a| {
                    a.tier == Tier::Og && (a.language == value.language || a.language == "und")
                })
            };
            ensure!(
                verified,
                "Override base {base} is not a verified OG root in {}",
                value.language
            );
        }
        self.overrides = overrides;
        Ok(())
    }
    pub fn coverage(&self) -> String {
        match &self.store {
            Some(s) => format!("Installed dictionary: {} unique names, {} analyses; languages {:?}; {}", s.stats.unique_names, s.stats.accepted, s.stats.languages, s.stats.installed_at),
            None => format!("CURATED FALLBACK ONLY: {} entries; run dictionary sync for the large multilingual index", self.entries.len()),
        }
    }
    pub fn extend_tsv(&mut self, text: &str) -> Result<()> {
        for (index, line) in text.trim_start_matches('\u{feff}').lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (name, labels) = line.split_once('\t').with_context(|| {
                format!(
                    "Lexicon line {}: expected username<TAB>categories",
                    index + 1
                )
            })?;
            let name =
                normalize_username(name).with_context(|| format!("Lexicon line {}", index + 1))?;
            let mut labels: Vec<String> = labels.split(',').map(|s| s.trim().to_string()).collect();
            ensure!(
                labels
                    .iter()
                    .all(|label| CATEGORIES.contains(&label.as_str())),
                "Invalid category at line {}; valid categories: {}",
                index + 1,
                CATEGORIES.join(", ")
            );
            let existing = self.entries.entry(name).or_default();
            existing.append(&mut labels);
            existing.sort();
            existing.dedup();
        }
        Ok(())
    }
    pub fn classify(&self, username: &str) -> Vec<String> {
        self.entries
            .get(&username.to_ascii_lowercase())
            .cloned()
            .unwrap_or_else(|| vec!["unknown".into()])
    }
    fn exact(&self, name: &str) -> Result<Vec<LexicalAnalysis>> {
        if let Some(value) = self.overrides.get(name) {
            return Ok(vec![LexicalAnalysis {
                tier: value.tier,
                original: name.into(),
                language: value.language.clone(),
                categories: value.categories.clone(),
                base: value.base.clone(),
                rule: "explicit local override".into(),
                source: "YAML matching.overrides".into(),
                normalization: "exact".into(),
            }]);
        }
        let mut found = self
            .store
            .as_ref()
            .map(|s| s.lookup(name))
            .transpose()?
            .unwrap_or_default();
        if let Some(categories) = self.entries.get(name) {
            let language = match name {
                "brain" | "flower" | "moon" | "winter" => "en",
                _ => "und",
            };
            found.insert(
                0,
                LexicalAnalysis {
                    tier: Tier::Og,
                    original: name.into(),
                    language: language.into(),
                    categories: categories.clone(),
                    base: None,
                    rule: "curated exact match".into(),
                    source: "FragShot curated lexicon".into(),
                    normalization: "exact".into(),
                },
            );
        }
        enrich_semantics(name, &mut found);
        Ok(found)
    }
    pub fn analyze(&self, name: &str) -> Result<Vec<LexicalAnalysis>> {
        if self.overrides.contains_key(name) {
            return self.exact(name);
        }
        if name == "brainer" {
            return Ok(vec![LexicalAnalysis {
                tier: Tier::SemiOg,
                original: name.into(),
                language: "en".into(),
                categories: vec!["word".into()],
                base: Some("brain".into()),
                rule: "suffix:er (curated override)".into(),
                source: "FragShot curated rules".into(),
                normalization: "exact".into(),
            }]);
        }
        let mut exact = self.exact(name)?;
        if !exact.is_empty() {
            exact.sort_by_key(|a| if a.tier == Tier::Og { 0 } else { 1 });
            return Ok(exact);
        }
        if !name.bytes().all(|c| c.is_ascii_alphabetic()) {
            return Ok(vec![]);
        }
        for suffix in &self.suffixes {
            if let Some(base) = name
                .strip_suffix(suffix)
                .filter(|b| b.len() >= self.min_root)
            {
                for mut root in self.exact(base)? {
                    if root.tier == Tier::Og && root.language == "en" {
                        root.tier = Tier::SemiOg;
                        root.base = Some(base.into());
                        root.rule = format!("suffix:{suffix}");
                        root.original = name.into();
                        exact.push(root);
                    }
                }
            }
        }
        Ok(exact)
    }
    pub fn annotate(&self, listing: &mut Listing) -> Result<()> {
        listing.analyses = self.analyze(&listing.username)?;
        listing.categories = listing
            .analyses
            .iter()
            .flat_map(|a| a.categories.iter().cloned())
            .collect();
        listing.categories.sort();
        listing.categories.dedup();
        if listing.categories.is_empty() {
            listing.categories.push("unknown".into());
        }
        Ok(())
    }
    pub fn names(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }
    pub fn query_seeds(
        &self,
        settings: &crate::config::Settings,
        limit: usize,
    ) -> Result<Vec<String>> {
        if limit == 0 {
            return Ok(vec![]);
        }
        let mut pool: std::collections::BTreeSet<String> = self
            .entries
            .keys()
            .chain(self.overrides.keys())
            .cloned()
            .collect();
        for line in include_str!("../data/semantics.tsv")
            .lines()
            .filter(|l| !l.starts_with('#'))
        {
            if let Some(name) = line.split('\t').next() {
                pool.insert(name.into());
            }
        }
        for ranks in self.frequency_ranks.values() {
            pool.extend(
                ranks
                    .iter()
                    .filter(|(name, rank)| **rank <= 500 && name.len() >= 4)
                    .map(|(name, _)| name.clone()),
            );
        }
        let mut buckets = BTreeMap::<(usize, String, String), Vec<Listing>>::new();
        for name in pool {
            let mut item = Listing::unknown(&name);
            crate::app::prepare(&mut item, settings, self)?;
            if !crate::app::matches_name(&item, settings) {
                continue;
            }
            let Some(primary) = &item.primary else {
                continue;
            };
            if primary.tier != Tier::Og {
                continue;
            }
            // Bare high-frequency grammatical words do not make useful discovery seeds.
            if ["word", "unknown", "slang", "profanity"].contains(&primary.category.as_str())
                && !self.entries.contains_key(&name)
                && !self.overrides.contains_key(&name)
            {
                continue;
            }
            let language_order = settings
                .matching
                .languages
                .iter()
                .position(|l| l == &primary.language)
                .unwrap_or(usize::MAX);
            buckets
                .entry((
                    language_order,
                    primary.language.clone(),
                    primary.category.clone(),
                ))
                .or_default()
                .push(item);
        }
        for rows in buckets.values_mut() {
            rows.sort_by(|a, b| {
                b.evaluation
                    .as_ref()
                    .unwrap()
                    .name_tenths
                    .cmp(&a.evaluation.as_ref().unwrap().name_tenths)
                    .then_with(|| a.username.cmp(&b.username))
            });
        }
        let mut languages = BTreeMap::<(usize, String), Vec<String>>::new();
        let mut depth = 0;
        loop {
            let mut added = false;
            for ((order, language, _), rows) in &buckets {
                if let Some(row) = rows.get(depth) {
                    languages
                        .entry((*order, language.clone()))
                        .or_default()
                        .push(row.username.clone());
                    added = true;
                }
            }
            if !added {
                break;
            }
            depth += 1;
        }
        let mut names = Vec::new();
        let mut depth = 0;
        loop {
            let mut added = false;
            for rows in languages.values() {
                if let Some(name) = rows.get(depth) {
                    names.push(name.clone());
                    added = true;
                    if names.len() >= limit {
                        return Ok(names);
                    }
                }
            }
            if !added {
                return Ok(names);
            }
            depth += 1;
        }
    }
    pub fn bounded_names(&self, limit: usize) -> Result<Vec<String>> {
        if limit == 0 {
            return Ok(vec![]);
        }
        let mut names: Vec<_> = self.overrides.keys().take(limit).cloned().collect();
        if let Some(store) = &self.store {
            for name in store.names(limit)? {
                if names.len() >= limit {
                    break;
                }
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
        for name in self.entries.keys() {
            if names.len() >= limit {
                break;
            }
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
        Ok(names)
    }
}
