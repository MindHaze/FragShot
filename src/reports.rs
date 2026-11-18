use crate::{
    app::tier,
    config::Settings,
    dictionary::language_label,
    model::{normalize_username, Listing},
    output::Report,
};
use anyhow::{ensure, Result};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

fn safe_filename(name: &str) -> Result<String> {
    let name = normalize_username(name)?;
    let upper = name.to_ascii_uppercase();
    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    Ok(if reserved.contains(&upper.as_str()) {
        format!("_{name}.txt")
    } else {
        format!("{name}.txt")
    })
}
fn category_label(category: &str) -> &str {
    match category {
        "first_name" => "FirstName",
        "surname" => "Surname",
        "brand" => "Brand",
        "company" => "Company",
        "video_game" => "VideoGame",
        "food" => "Food",
        "city" => "City",
        "slang" => "Slang",
        "profanity" => "Profanity",
        "word" => "Word",
        "animal" => "Animal",
        "nature" => "Nature",
        "space" => "Space",
        "technology" => "Technology",
        "science" => "Science",
        "body" => "Body",
        "color" => "Color",
        "time" => "Time",
        _ => "Unspecified",
    }
}
pub fn plain(text: &str) -> String {
    text.chars().filter(|c| (!c.is_control() || *c == '\n' || *c == '\t') && !matches!(*c, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')).collect()
}
pub fn rating_text(item: &Listing) -> String {
    use crate::scoring::{format_score, label};
    let Some(score) = &item.evaluation else {
        return "Name quality: 0.0/10 | Price appeal: 0.0/10 | Interest: 0.0/10 — not evaluated\n"
            .into();
    };
    let mut text = format!(
        "Name quality: {} | Price appeal: {} | Interest: {} — {}\nBasis: {} | {}\n",
        format_score(score.name_tenths),
        format_score(score.price_tenths),
        format_score(score.interest_tenths),
        label(score),
        score.basis,
        score.evidence_level
    );
    for action in &score.actions {
        text.push_str(&format!(
            "  {}: {} TON | price {} | interest {}{}\n",
            action.basis,
            action.amount,
            format_score(action.price_tenths),
            format_score(action.interest_tenths),
            if action.provisional {
                " [PROVISIONAL]"
            } else {
                ""
            }
        ));
    }
    text.push_str(&format!("Why: {}\nRanking model: {} | OG anchor {} TON | semi-OG anchor {} TON | quality-adjusted anchor {} TON (not market values)\n", score.reasons.join("; "), score.model, score.anchors.og_reference_ton, score.anchors.semi_og_reference_ton, score.adjusted_reference));
    for warning in &score.warnings {
        text.push_str(&format!("Note: {warning}\n"));
    }
    plain(&text)
}
pub fn render_text(item: &Listing, report: &Report) -> String {
    let mut text = format!("+--------------------------------------------------+\n| FRAGSHOT / USERNAME REPORT                        |\n+--------------------------------------------------+\n{}\nUsername: @{}\nLink: {}\nTier: {}\nStatus: {:?}\nCategories: {}\nChecked at UTC: {}\nBudget: {} TON | reserve: {} TON\nRun complete: {}\n", if report.demo { "SIMULATED DATA -- NOT A REAL OFFER" } else { "POINT-IN-TIME OBSERVATION -- VERIFY BEFORE ACTING" }, item.username, item.url, tier(item).label(), item.status, item.categories.join(", "), report.checked_at, report.max_ton, report.reserve_ton, report.complete);
    if let Some(primary) = &item.primary {
        text.push_str(&format!(
            "Primary category: {} | {}\nOther meanings: {}\n",
            primary.category,
            language_label(&primary.language),
            item.categories
                .iter()
                .filter(|c| **c != primary.category)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    text.push_str(&rating_text(item));
    for (label, amount) in [
        ("Buy now", item.buy_now),
        ("Minimum next bid", item.minimum_bid),
        ("Starting bid", item.start_bid),
    ] {
        if let Some(amount) = amount {
            let budget = amount
                .0
                .checked_add(report.reserve_ton.0)
                .is_some_and(|p| p <= report.max_ton.0);
            text.push_str(&format!(
                "{label}: {amount} TON [{}]\n",
                if budget {
                    "within budget"
                } else {
                    "OVER BUDGET"
                }
            ));
        }
    }
    if let Some(last) = item.last_bid {
        text.push_str(&format!(
            "Historical highest bid: {last} TON (not an actionable price)\n"
        ));
    }
    if let Some(end) = item.ends_at {
        text.push_str(&format!(
            "Deadline UTC: {}\n",
            chrono::DateTime::from_timestamp(end, 0)
                .map(|d| d.to_rfc3339())
                .unwrap_or_default()
        ));
    }
    if item.action_price().is_none() {
        text.push_str("No confirmed actionable price; never treated as free.\n");
    }
    text.push_str(&format!(
        "Evidence: {}\n\nLEXICAL ANALYSES\n",
        item.evidence
    ));
    for analysis in &item.analyses {
        text.push_str(&format!("- {} | {} ({}) | {}\n  Original: {} | base: {} | rule: {}\n  Normalization: {}\n  Source: {}\n", analysis.tier.label(), language_label(&analysis.language), analysis.language, analysis.categories.join(", "), analysis.original, analysis.base.as_deref().unwrap_or("none"), analysis.rule, analysis.normalization, analysis.source));
    }
    if let Some(error) = &report.error {
        text.push_str(&format!("\nINCOMPLETE RUN: {error}\n"));
    }
    text.push_str("\nClassification is indicative, not a valuation or rights guarantee.\nFees are not estimated. No wallet or transaction was used.\n");
    plain(&text)
}
fn checked_directory(path: &Path) -> Result<()> {
    if path.exists() {
        ensure!(
            !fs::symlink_metadata(path)?.file_type().is_symlink() && path.is_dir(),
            "Report directory must be a real directory: {}",
            path.display()
        );
    } else {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            checked_directory(parent)?;
        }
        fs::create_dir(path)?;
    }
    Ok(())
}
pub fn save(report: &Report, settings: &Settings) -> Result<Option<PathBuf>> {
    if !settings.results.enabled {
        return Ok(None);
    }
    checked_directory(&settings.results.directory)?;
    let prefix = if report.demo { "demo" } else { "scan" };
    let id = format!(
        "{prefix}-{}-{}",
        chrono::Utc::now().format("%Y%m%dT%H%M%S%.9fZ"),
        std::process::id()
    );
    let root = settings.results.directory.join(id);
    fs::create_dir(&root)?;
    let marker = root.join("INCOMPLETE.txt");
    let mut pending = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker)?;
    pending.write_all(b"REPORT WRITING INCOMPLETE: output may be missing or truncated. This marker is removed only when every report and the run manifest have been saved.\n")?;
    pending.sync_all()?;
    drop(pending);
    for item in &report.listings {
        let language = item
            .primary
            .as_ref()
            .map(|p| language_label(&p.language))
            .unwrap_or("Unspecified");
        let category = item
            .primary
            .as_ref()
            .map(|p| p.category.as_str())
            .unwrap_or("unknown");
        let mut directory = root.clone();
        for grouping in &settings.results.grouping {
            directory.push(if grouping == "language" {
                language
            } else {
                category_label(category)
            });
        }
        fs::create_dir_all(&directory)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join(safe_filename(&item.username)?))?;
        file.write_all(render_text(item, report).as_bytes())?;
    }
    let manifest = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join("run.json"))?;
    serde_json::to_writer_pretty(&manifest, report)?;
    manifest.sync_all()?;
    fs::remove_file(marker)?;
    Ok(Some(root))
}
