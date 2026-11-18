use crate::{
    classify::Lexicon,
    config::Settings,
    model::{Listing, Tier},
};
use std::collections::HashSet;

pub fn tier(item: &Listing) -> Tier {
    if let Some(primary) = &item.primary {
        return primary.tier;
    }
    if item.analyses.iter().any(|a| a.tier == Tier::Og) {
        Tier::Og
    } else if item.analyses.iter().any(|a| a.tier == Tier::SemiOg) {
        Tier::SemiOg
    } else {
        Tier::NonOg
    }
}
fn allowed_tier(quality: Tier, settings: &Settings) -> bool {
    (!settings.og_only || quality == Tier::Og)
        && if quality == Tier::NonOg {
            settings.matching.show_non_og
        } else {
            settings.matching.tiers.iter().any(|t| t == quality.key())
        }
}
pub fn analysis_matches(a: &crate::model::LexicalAnalysis, settings: &Settings) -> bool {
    (a.language == "und"
        || settings.matching.languages.is_empty()
        || settings.matching.languages.contains(&a.language))
        && allowed_tier(a.tier, settings)
        && (settings.categories.is_empty()
            || a.categories.iter().any(|c| settings.categories.contains(c)))
}
pub fn prepare(item: &mut Listing, settings: &Settings, lexicon: &Lexicon) -> anyhow::Result<()> {
    if item.analyses.is_empty() {
        lexicon.annotate(item)?;
    }
    let category_for = |a: &crate::model::LexicalAnalysis| {
        a.categories
            .iter()
            .filter(|c| settings.categories.is_empty() || settings.categories.contains(c))
            .min_by_key(|c| crate::classify::category_priority(c))
            .cloned()
            .unwrap_or_else(|| "unknown".into())
    };
    let mut chosen = item
        .analyses
        .iter()
        .enumerate()
        .filter(|(_, a)| analysis_matches(a, settings))
        .min_by_key(|(_, a)| {
            let source = if a.source == "YAML matching.overrides" {
                0
            } else if a.source.starts_with("FragShot curated semantics") {
                1
            } else if a.source.starts_with("FragShot curated") {
                2
            } else {
                3
            };
            let tier = if a.tier == Tier::Og { 0 } else { 1 };
            let language = settings
                .matching
                .languages
                .iter()
                .position(|l| l == &a.language)
                .unwrap_or(usize::MAX);
            let category = crate::classify::category_priority(&category_for(a));
            (source, tier, language, category)
        });
    let preferred_category = chosen.map(|(_, a)| category_for(a));
    if let Some((_, curated)) =
        chosen.filter(|(_, a)| a.language == "und" && a.source == "FragShot curated lexicon")
    {
        let category = preferred_category
            .as_ref()
            .expect("chosen analysis has a category");
        // Refine unspecified provenance only when the same meaning is independently evidenced.
        if let Some(specific) = item
            .analyses
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                a.language != "und"
                    && a.tier == curated.tier
                    && a.normalization == curated.normalization
                    && a.categories.contains(category)
                    && analysis_matches(a, settings)
            })
            .min_by_key(|(_, a)| {
                settings
                    .matching
                    .languages
                    .iter()
                    .position(|l| l == &a.language)
                    .unwrap_or(usize::MAX)
            })
        {
            chosen = Some(specific);
        }
    }
    item.primary = chosen.map(|(index, a)| crate::model::PrimaryClassification {
        analysis_index: index,
        language: a.language.clone(),
        tier: a.tier,
        category: preferred_category
            .clone()
            .unwrap_or_else(|| category_for(a)),
        frequency_rank: lexicon.frequency_rank(&item.username, &a.language),
        root_frequency_rank: if a.tier == Tier::SemiOg {
            a.base
                .as_deref()
                .and_then(|base| crate::dictionary::normalized_word(base, true))
                .and_then(|(base, _)| lexicon.frequency_rank(&base, &a.language))
        } else {
            None
        },
    });
    item.evaluation = Some(crate::scoring::evaluate(
        item,
        &settings.scoring,
        chrono::Utc::now().timestamp(),
    ));
    Ok(())
}
pub fn matches_name(item: &Listing, settings: &Settings) -> bool {
    if !(settings.min_length..=settings.max_length).contains(&item.username.len()) {
        return false;
    }
    if !settings.matching.include_offensive && item.categories.iter().any(|c| c == "profanity") {
        return false;
    }
    if item.analyses.is_empty() {
        return allowed_tier(Tier::NonOg, settings)
            && (settings.categories.is_empty()
                || item
                    .categories
                    .iter()
                    .any(|c| settings.categories.contains(c)));
    }
    item.analyses.iter().any(|a| analysis_matches(a, settings))
}
pub fn select_checked(
    listings: Vec<Listing>,
    settings: &Settings,
    lexicon: &Lexicon,
) -> anyhow::Result<Vec<Listing>> {
    Ok(select_report(listings, settings, lexicon, true)?.listings)
}
pub fn select(
    listings: Vec<Listing>,
    settings: &Settings,
    lexicon: &Lexicon,
) -> anyhow::Result<Vec<Listing>> {
    Ok(select_report(listings, settings, lexicon, false)?.listings)
}
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct SelectionStats {
    pub lexical_filtered: usize,
    pub status_filtered: usize,
    pub no_price: usize,
    pub over_budget: usize,
    pub eligible: usize,
    pub displayed: usize,
    pub omitted_by_limit: usize,
}
pub struct Selection {
    pub listings: Vec<Listing>,
    pub stats: SelectionStats,
}
pub fn compare(a: &Listing, b: &Listing, settings: &Settings) -> std::cmp::Ordering {
    let scores = |i: &Listing| {
        i.evaluation
            .as_ref()
            .map(|s| (s.interest_tenths, s.name_tenths))
            .unwrap_or((0, 0))
    };
    let price = || {
        a.action_price()
            .unwrap_or(crate::model::Ton(u64::MAX))
            .cmp(&b.action_price().unwrap_or(crate::model::Ton(u64::MAX)))
    };
    match settings.ranking.sort {
        crate::config::RankingSort::Interest => scores(b).cmp(&scores(a)).then_with(price),
        crate::config::RankingSort::NameQuality => scores(b)
            .1
            .cmp(&scores(a).1)
            .then_with(|| scores(b).0.cmp(&scores(a).0))
            .then_with(price),
        crate::config::RankingSort::PriceAsc => price(),
    }
    .then_with(|| a.username.cmp(&b.username))
}
pub fn select_report(
    listings: Vec<Listing>,
    settings: &Settings,
    lexicon: &Lexicon,
    show_checked: bool,
) -> anyhow::Result<Selection> {
    let mut seen = HashSet::new();
    let mut selected = Vec::new();
    let mut stats = SelectionStats::default();
    for mut item in listings {
        if !seen.insert(item.username.clone()) {
            continue;
        }
        prepare(&mut item, settings, lexicon)?;
        if !matches_name(&item, settings) {
            stats.lexical_filtered += 1;
            continue;
        }
        if !settings.statuses.is_empty() && !settings.statuses.contains(&item.status) {
            stats.status_filtered += 1;
            continue;
        }
        if !show_checked {
            if item.action_price().is_some() {
                if !item.within_budget(settings.max_ton, settings.reserve_ton) {
                    stats.over_budget += 1;
                    continue;
                }
            } else if !settings.show_unavailable {
                stats.no_price += 1;
                continue;
            }
        }
        selected.push(item);
    }
    selected.sort_by(|a, b| compare(a, b, settings));
    stats.eligible = selected.len();
    selected.truncate(settings.limit);
    stats.displayed = selected.len();
    stats.omitted_by_limit = stats.eligible - stats.displayed;
    Ok(Selection {
        listings: selected,
        stats,
    })
}
