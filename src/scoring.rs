use crate::{
    config::ScoringSettings,
    model::{Listing, Status, Tier, Ton},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionRating {
    pub basis: String,
    pub amount: Ton,
    pub price_tenths: u8,
    pub interest_tenths: u8,
    pub provisional: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evaluation {
    pub model: String,
    pub name_tenths: u8,
    pub price_tenths: u8,
    pub interest_tenths: u8,
    pub assessable: bool,
    pub basis: String,
    pub evidence_level: String,
    pub anchors: ScoringSettings,
    pub adjusted_reference: Ton,
    pub actions: Vec<ActionRating>,
    pub reasons: Vec<String>,
    pub warnings: Vec<String>,
}
pub fn format_score(tenths: u8) -> String {
    format!("{}.{}/10", tenths / 10, tenths % 10)
}
pub fn label(score: &Evaluation) -> &'static str {
    if !score.assessable {
        "Not assessable"
    } else if score.interest_tenths >= 70 {
        "High interest"
    } else if score.interest_tenths >= 40 {
        "Worth reviewing"
    } else {
        "Low interest"
    }
}
fn common_points(rank: usize) -> u8 {
    match rank {
        1..=1000 => 30,
        1001..=10000 => 20,
        10001..=50000 => 10,
        _ => 0,
    }
}
pub fn evaluate(item: &Listing, settings: &ScoringSettings, now: i64) -> Evaluation {
    let primary = item.primary.as_ref();
    let analysis = primary.and_then(|p| item.analyses.get(p.analysis_index));
    let tier = primary.map(|p| p.tier).unwrap_or(Tier::NonOg);
    let mut reasons = Vec::new();
    let mut warnings = Vec::new();
    let mut points: u8 = match tier {
        Tier::Og => 40,
        Tier::SemiOg => 25,
        Tier::NonOg => 0,
    };
    reasons.push(format!(
        "{} lexical evidence: {}",
        tier.label(),
        format_score(points)
    ));
    let length = match item.username.len() {
        3..=5 => 20,
        6..=8 => 15,
        9..=12 => 5,
        _ => 0,
    };
    points += length;
    reasons.push(format!(
        "{} characters: +{:.1} points",
        item.username.len(),
        f32::from(length) / 10.0
    ));
    let common = if let Some(rank) = primary.and_then(|p| p.frequency_rank) {
        reasons.push(format!("Installed language corpus rank: {rank}"));
        common_points(rank)
    } else if let Some(rank) = primary.and_then(|p| p.root_frequency_rank) {
        reasons.push(format!(
            "Verified root corpus rank: {rank} (half commonness credit)"
        ));
        common_points(rank) / 2
    } else {
        warnings
            .push("Corpus commonness unknown; absent ranking is not proof of low demand".into());
        0
    };
    points += common;
    if item.username.bytes().all(|c| c.is_ascii_alphabetic()) {
        if analysis.is_some_and(|a| a.normalization != "exact") {
            points += 5;
            reasons.push("Disclosed romanization/transliteration: +0.5 points".into());
        } else {
            points += 10;
            reasons.push("Clean alphabetic spelling: +1.0 points".into());
        }
    }
    points = points.min(if tier == Tier::NonOg { 20 } else { 100 });
    if item
        .categories
        .iter()
        .any(|c| c == "brand" || c == "company")
    {
        warnings.push("Brand/company meaning: trademark and other rights require review".into());
    }
    if item.categories.iter().any(|c| c == "profanity") {
        warnings.push("An offensive meaning is recorded; review audience suitability".into());
    }
    let anchor = match tier {
        Tier::Og => settings.og_reference_ton,
        Tier::SemiOg => settings.semi_og_reference_ton,
        Tier::NonOg => Ton(0),
    };
    let reference = u128::from(anchor.0) * u128::from(points).pow(2) / 10000;
    let mut actions = Vec::new();
    let active = item.status != Status::Auction || item.ends_at.is_none_or(|end| end > now);
    if active {
        for (basis, amount, valid, provisional) in [
            (
                "buy_now",
                item.buy_now,
                matches!(item.status, Status::Sale | Status::Auction),
                false,
            ),
            (
                "minimum_bid",
                item.minimum_bid,
                item.status == Status::Auction,
                true,
            ),
            (
                "start_bid",
                item.start_bid,
                item.status == Status::StartAuction,
                true,
            ),
        ] {
            if let Some(amount) = amount.filter(|p| valid && p.0 > 0) {
                let denominator = reference + u128::from(amount.0);
                let price = ((100 * reference + denominator / 2) / denominator) as u8;
                let interest = ((u16::from(points) * u16::from(price) + 50) / 100) as u8;
                actions.push(ActionRating {
                    basis: basis.into(),
                    amount,
                    price_tenths: price,
                    interest_tenths: interest.min(if provisional { 60 } else { 100 }),
                    provisional,
                });
            }
        }
    }
    let evidence = item.evidence.to_ascii_lowercase();
    let evidence_level = if evidence.contains("simulated") {
        "simulated"
    } else if evidence.contains("catalogue") {
        "catalogue-only"
    } else if evidence.starts_with("fragment listing:") {
        "detail-observed"
    } else {
        "unconfirmed"
    }
    .to_string();
    if evidence_level == "catalogue-only" {
        warnings
            .push("Catalogue observation only; re-confirm the action on the detail page".into());
    }
    if actions.iter().any(|a| a.provisional) {
        warnings.push("Auction score is provisional and capped at 6.0/10; the final purchase price is unknown".into());
    }
    if actions.is_empty() {
        warnings.push(
            "No confirmed positive actionable price; zero scores mean no assessable deal".into(),
        );
    }
    warnings.push("Heuristic ranking anchors, not comparable sales, resale value or profit forecasts; fees are not estimated".into());
    Evaluation {
        model: "interest-v1".into(),
        name_tenths: points,
        price_tenths: actions.first().map(|a| a.price_tenths).unwrap_or(0),
        interest_tenths: actions.first().map(|a| a.interest_tenths).unwrap_or(0),
        assessable: !actions.is_empty(),
        basis: actions
            .first()
            .map(|a| a.basis.clone())
            .unwrap_or_else(|| "none".into()),
        evidence_level,
        anchors: settings.clone(),
        adjusted_reference: Ton(reference as u64),
        actions,
        reasons,
        warnings,
    }
}
