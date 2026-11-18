use crate::{
    config::{Cli, Color, Settings},
    model::{Listing, Status, Ton},
};
use anyhow::{ensure, Context, Result};
use console::Style;
use serde::Serialize;
use std::{
    fs::OpenOptions,
    io::{self, IsTerminal, Write},
    path::Path,
};

#[derive(Serialize)]
pub struct Report {
    pub source: &'static str,
    pub demo: bool,
    pub complete: bool,
    pub error: Option<String>,
    pub hidden: usize,
    pub batches: usize,
    pub checked_at: String,
    pub max_ton: Ton,
    pub reserve_ton: Ton,
    pub examined: usize,
    pub requests: usize,
    pub listings: Vec<Listing>,
    pub selection: crate::app::SelectionStats,
}

pub fn preflight_exports(cli: &Cli) -> Result<()> {
    for path in [&cli.json, &cli.csv].into_iter().flatten() {
        ensure!(
            !path.exists(),
            "Export file already exists: {} (choose a different name)",
            path.display()
        );
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        ensure!(
            parent.is_dir(),
            "Export directory not found: {}",
            parent.display()
        );
    }
    if let (Some(json), Some(csv)) = (&cli.json, &cli.csv) {
        ensure!(
            json != csv,
            "JSON and CSV exports must use different file paths"
        );
    }
    Ok(())
}

pub fn log(settings: &Settings, level: &str, message: &str) {
    if !settings.console.progress && level == "INFO" {
        return;
    }
    let colors = match settings.console.color {
        Color::Always => true,
        Color::Never => false,
        Color::Auto => io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none(),
    };
    let style = match level {
        "ERROR" => Style::new().red().bold(),
        "WARN" => Style::new().yellow().bold(),
        "MATCH" => Style::new().green().bold(),
        _ => Style::new().cyan(),
    };
    eprintln!(
        "{} {} {}",
        Style::new()
            .dim()
            .force_styling(colors)
            .apply_to(chrono::Utc::now().format("%H:%M:%S")),
        style
            .force_styling(colors)
            .apply_to(format!("[{level:^5}]")),
        crate::reports::plain(message)
    );
}

fn state(item: &Listing) -> (&'static str, Style) {
    match item.status {
        Status::Sale => ("BUY NOW", Style::new().green().bold()),
        Status::StartAuction => ("START AUCTION", Style::new().cyan().bold()),
        Status::Auction if item.buy_now.is_some() => {
            ("BID + BUY NOW", Style::new().magenta().bold())
        }
        Status::Auction if item.last_bid.is_some_and(|p| p.0 > 0) => {
            ("OUTBID", Style::new().yellow().bold())
        }
        Status::Auction => ("BID", Style::new().yellow().bold()),
        Status::Sold => ("SOLD", Style::new().dim()),
        Status::Unavailable => ("UNAVAILABLE", Style::new().red()),
        Status::Unknown => ("UNKNOWN / UNCONFIRMED", Style::new().dim()),
    }
}
fn category_label(label: &str) -> &str {
    match label {
        "first_name" => "first name",
        "surname" => "surname",
        "video_game" => "video game",
        other => other,
    }
}
fn budget_label(amount: Ton, settings: &Settings) -> String {
    if amount
        .0
        .checked_add(settings.reserve_ton.0)
        .is_some_and(|n| n <= settings.max_ton.0)
    {
        "within budget".into()
    } else {
        "OVER BUDGET".into()
    }
}
fn remaining(end: i64) -> String {
    let seconds = end - chrono::Utc::now().timestamp();
    if seconds <= 0 {
        return "deadline reached, re-check needed".into();
    }
    let days = seconds / 86400;
    let hours = seconds % 86400 / 3600;
    let minutes = seconds % 3600 / 60;
    format!("{days}d {hours}h {minutes}min")
}

pub fn print_report(report: &Report, settings: &Settings, color: Color) -> Result<()> {
    let colors = match color {
        Color::Always => true,
        Color::Never => false,
        Color::Auto => io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none(),
    };
    let out = io::stdout();
    let mut out = out.lock();
    let heading = Style::new().cyan().bold().force_styling(colors);
    writeln!(
        out,
        "\n{}  |  Telegram usernames \u{b7} read-only",
        heading.apply_to("FRAGSHOT")
    )?;
    if settings.console.banner {
        let banner = "  _____ ____      _    ____ ____  _   _  ___ _____\n |  ___|  _ \\    / \\  / ___/ ___|| | | |/ _ \\_   _|\n | |_  | |_) |  / _ \\| |  _\\___ \\| |_| | | | || |\n |  _| |  _ <  / ___ \\ |_| |___) |  _  | |_| || |\n |_|   |_| \\_\\/_/   \\_\\____|____/|_| |_|\\___/ |_|";
        if console::Term::stdout().size().1 >= 60 {
            writeln!(out, "{}", heading.apply_to(banner))?;
        }
    }
    if !report.complete {
        writeln!(
            out,
            "{}",
            Style::new()
                .red()
                .bold()
                .force_styling(colors)
                .apply_to("INCOMPLETE RUN -- partial observations only")
        )?;
    }
    writeln!(
        out,
        "Batches: {} | requests: {} | hidden by lexical filters: {}",
        report.batches, report.requests, report.hidden
    )?;
    if report.demo {
        writeln!(
            out,
            "SIMULATED DATA -- no real offers, no network requests."
        )?;
    }
    writeln!(
        out,
        "Budget: {} TON \u{b7} reserve: {} TON \u{b7} {} results / {} examined",
        settings.max_ton,
        settings.reserve_ton,
        report.listings.len(),
        report.examined
    )?;
    writeln!(
        out,
        "Green: buy now \u{b7} cyan: start auction \u{b7} yellow: bid \u{b7} magenta: bid + buy now"
    )?;
    writeln!(out, "Selection: lexical {} | status {} | no price {} | over budget {} | eligible {} | displayed {} | omitted by limit {}", report.selection.lexical_filtered, report.selection.status_filtered, report.selection.no_price, report.selection.over_budget, report.selection.eligible, report.selection.displayed, report.selection.omitted_by_limit)?;
    writeln!(out, "Checked at UTC: {}\n", report.checked_at)?;
    for item in &report.listings {
        let (label, style) = state(item);
        let quality = crate::app::tier(item);
        let badge = if quality == crate::model::Tier::Og {
            Style::new().green().bold()
        } else {
            Style::new().magenta().bold()
        };
        writeln!(
            out,
            "{}",
            Style::new()
                .cyan()
                .force_styling(colors)
                .apply_to("+------------------------------------------------------")
        )?;
        writeln!(
            out,
            "  {}  @{}  {}",
            badge.force_styling(colors).apply_to(quality.label()),
            item.username,
            style.force_styling(colors).apply_to(label)
        )?;
        if settings.alerts.console_matches
            && !report.demo
            && item.within_budget(settings.max_ton, settings.reserve_ton)
        {
            writeln!(
                out,
                "  {}",
                Style::new()
                    .yellow()
                    .bold()
                    .force_styling(colors)
                    .apply_to("[MATCH] Action within your budget")
            )?;
            if settings.alerts.bell && io::stdout().is_terminal() {
                write!(out, "\u{7}")?;
            }
        }
        if let Some(primary) = &item.primary {
            writeln!(
                out,
                "  Primary category: {} | {}",
                primary.category,
                crate::dictionary::language_label(&primary.language)
            )?;
            let secondary: Vec<_> = item
                .categories
                .iter()
                .filter(|c| **c != primary.category)
                .map(|s| s.as_str())
                .collect();
            if !secondary.is_empty() {
                writeln!(out, "  Other meanings: {}", secondary.join(", "))?;
            }
        }
        let interest = item
            .evaluation
            .as_ref()
            .map(|e| e.interest_tenths)
            .unwrap_or(0);
        let rating_style = if interest >= 70 {
            Style::new().green().bold()
        } else if interest >= 40 {
            Style::new().yellow().bold()
        } else {
            Style::new().dim()
        };
        for line in crate::reports::rating_text(item).lines() {
            if line.starts_with("Name quality:") {
                writeln!(
                    out,
                    "  {}",
                    rating_style.clone().force_styling(colors).apply_to(line)
                )?;
            } else {
                writeln!(out, "  {line}")?;
            }
        }
        for analysis in &item.analyses {
            writeln!(
                out,
                "  {} | {} | base: {} | {}",
                crate::dictionary::language_label(&analysis.language),
                crate::reports::plain(&analysis.original),
                crate::reports::plain(analysis.base.as_deref().unwrap_or("--")),
                crate::reports::plain(&analysis.rule)
            )?;
        }
        writeln!(
            out,
            "  Categories: {} \u{b7} {} characters",
            item.categories
                .iter()
                .map(|c| category_label(c))
                .collect::<Vec<_>>()
                .join(", "),
            item.username.len()
        )?;
        for (label, amount) in [
            ("Buy now", item.buy_now),
            ("Minimum next bid", item.minimum_bid),
            ("Starting bid", item.start_bid),
        ] {
            if let Some(amount) = amount {
                writeln!(
                    out,
                    "  {label}: {amount} TON [{}]",
                    budget_label(amount, settings)
                )?;
            }
        }
        if let Some(amount) = item.last_bid {
            writeln!(
                out,
                "  Last bid: {amount} TON (history only, not the purchase price)"
            )?;
        }
        if let Some(end) = item.ends_at {
            let date = chrono::DateTime::from_timestamp(end, 0)
                .map(|d| d.to_rfc3339())
                .unwrap_or_default();
            writeln!(out, "  Ends: {} \u{b7} {}", remaining(end), date)?;
        }
        if item.action_price().is_none() {
            writeln!(
                out,
                "  No confirmed actionable price; never treated as 0 TON."
            )?;
        }
        writeln!(out, "  {}\n  {}\n", item.evidence, item.url)?;
    }
    if report.listings.is_empty() {
        writeln!(
            out,
            "No matching results in this sample; absence here does not prove no offers exist."
        )?;
    }
    writeln!(
        out,
        "Bids can increase. Fees not estimated: the reserve is your margin, not a guarantee."
    )?;
    writeln!(out, "Categories are indicative; the lexicon is not exhaustive. Always check the listing before acting.")?;
    Ok(())
}

pub fn export(report: &Report, cli: &Cli) -> Result<()> {
    if let Some(path) = &cli.json {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .with_context(|| format!("JSON export {}", path.display()))?;
        serde_json::to_writer_pretty(file, report)?;
        eprintln!("JSON: {}", path.display());
    }
    if let Some(path) = &cli.csv {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .with_context(|| format!("CSV export {}", path.display()))?;
        let mut writer = csv::Writer::from_writer(file);
        writer.write_record([
            "username",
            "status",
            "categories",
            "buy_now_ton",
            "minimum_bid_ton",
            "start_bid_ton",
            "last_bid_ton",
            "ends_at_unix",
            "url",
            "evidence",
            "demo",
            "checked_at_utc",
            "max_ton",
            "reserve_ton",
            "primary_category",
            "primary_language",
            "name_score",
            "price_score",
            "interest_score",
            "rating_basis",
            "rating_assessable",
            "rating_details_json",
        ])?;
        for item in &report.listings {
            let amount = |price: Option<Ton>| price.map(|p| p.to_string()).unwrap_or_default();
            writer.write_record([
                item.username.clone(),
                serde_json::to_value(item.status)?
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string(),
                item.categories.join("|"),
                amount(item.buy_now),
                amount(item.minimum_bid),
                amount(item.start_bid),
                amount(item.last_bid),
                item.ends_at.map(|n| n.to_string()).unwrap_or_default(),
                item.url.clone(),
                item.evidence.clone(),
                report.demo.to_string(),
                report.checked_at.clone(),
                report.max_ton.to_string(),
                report.reserve_ton.to_string(),
                item.primary
                    .as_ref()
                    .map(|p| p.category.clone())
                    .unwrap_or_default(),
                item.primary
                    .as_ref()
                    .map(|p| p.language.clone())
                    .unwrap_or_default(),
                crate::scoring::format_score(
                    item.evaluation.as_ref().map(|e| e.name_tenths).unwrap_or(0),
                ),
                crate::scoring::format_score(
                    item.evaluation
                        .as_ref()
                        .map(|e| e.price_tenths)
                        .unwrap_or(0),
                ),
                crate::scoring::format_score(
                    item.evaluation
                        .as_ref()
                        .map(|e| e.interest_tenths)
                        .unwrap_or(0),
                ),
                item.evaluation
                    .as_ref()
                    .map(|e| e.basis.clone())
                    .unwrap_or_default(),
                item.evaluation
                    .as_ref()
                    .is_some_and(|e| e.assessable)
                    .to_string(),
                serde_json::to_string(&item.evaluation)?,
            ])?;
        }
        writer.flush()?;
        eprintln!("CSV: {}", path.display());
    }
    Ok(())
}
