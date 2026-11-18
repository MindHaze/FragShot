use anyhow::{ensure, Context, Result};
use clap::Parser;
use fragshot::{
    app::{matches_name, select_report},
    classify::Lexicon,
    client::FragmentClient,
    config::{Cli, RunMode, Settings},
    dictionary,
    model::{normalize_username, Listing, Status, Ton},
    output::{self, Report},
    reports, scanner,
};
use std::{
    collections::HashSet,
    fs,
    process::ExitCode,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::BrokenPipe)
            {
                return ExitCode::SUCCESS;
            }
            eprintln!("Error: {error:#}");
            ExitCode::FAILURE
        }
    }
}
fn run(mut cli: Cli) -> Result<()> {
    let settings = Settings::load(&cli)?;
    if matches!(
        settings.mode,
        RunMode::DictionarySync | RunMode::DictionaryImport | RunMode::DictionaryStatus
    ) {
        if settings.mode == RunMode::DictionaryStatus {
            match dictionary::DictionaryStore::open(&settings.dictionary.directory)? {
                Some(store) => println!("{}", serde_json::to_string_pretty(&store.stats)?),
                None => println!("No large dictionary installed. Curated fallback only. Run: fragshot dictionary sync"),
            }
            return Ok(());
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        ctrlc::set_handler(move || flag.store(true, Ordering::Relaxed))?;
        output::log(&settings, "WARN", "Large dictionary import: streaming to a new index. Ctrl+C cancels without replacing the previous index.");
        let progress = |records, bytes| {
            output::log(
                &settings,
                "INFO",
                &format!(
                    "Dictionary: {records} records | {} MiB decompressed",
                    bytes / 1024 / 1024
                ),
            )
        };
        let stats = if settings.mode == RunMode::DictionaryImport {
            dictionary::import_file(
                settings
                    .dictionary
                    .input
                    .as_deref()
                    .context("dictionary.input is required for import")?,
                &settings.dictionary,
                &settings.matching.languages,
                &cancelled,
                progress,
            )?
        } else {
            dictionary::sync(
                &settings.dictionary,
                &settings.matching.languages,
                &cancelled,
                progress,
            )?
        };
        println!("{}", serde_json::to_string_pretty(&stats)?);
        return Ok(());
    }
    cli.json = settings.results.json.clone();
    cli.csv = settings.results.csv.clone();
    output::preflight_exports(&cli)?;
    let mut lexicon = Lexicon::built_in();
    lexicon.attach(&settings.dictionary.directory)?;
    lexicon.configure(
        settings.matching.suffixes.clone(),
        settings.matching.min_root_length,
    );
    if let Some(path) = &settings.lexicon {
        lexicon.extend_tsv(
            &fs::read_to_string(path).with_context(|| format!("Lexicon {}", path.display()))?,
        )?;
    }
    lexicon.set_overrides(settings.matching.overrides.clone())?;
    output::log(
        &settings,
        "INFO",
        "Loading bounded per-language frequency rankings from the installed index",
    );
    lexicon.load_frequency(&settings.matching.languages)?;
    output::log(&settings, "INFO", &lexicon.coverage());
    let demo = settings.mode == RunMode::Demo;
    let outcome = match settings.mode {
        RunMode::Demo => {
            let mut rows = demo_listings();
            for row in &mut rows {
                lexicon.annotate(row)?;
            }
            let hidden = rows.iter().filter(|r| !matches_name(r, &settings)).count();
            scanner::ScanOutcome {
                items: rows,
                hidden,
                ..Default::default()
            }
        }
        RunMode::Scan => {
            output::log(&settings, "WARN", "No public next-page cursor exposed for the observed username catalogue; scanning configured batches, not invented pages.");
            let queries = lexicon.query_seeds(&settings, settings.scan.dictionary_queries)?;
            let urls = scanner::batch_urls(&settings, &queries);
            output::log(&settings, "INFO", &format!("Scheduled {} catalogue batches with {} dictionary seeds; detail cap {}, display cap {}. Sampling does not guarantee this many matches.", urls.len(), queries.len(), settings.details, settings.limit));
            let mut client = FragmentClient::new(&settings)?;
            scanner::scan(&mut client, &settings, &lexicon, &urls, |text| {
                output::log(&settings, "INFO", text)
            })
        }
        RunMode::Check => {
            let mut candidates = settings.check.names.clone();
            if let Some(path) = &settings.check.file {
                let file = fs::File::open(path)
                    .with_context(|| format!("Username list {}", path.display()))?;
                ensure!(
                    file.metadata()?.len() <= 1024 * 1024,
                    "Username list exceeds 1 MiB limit"
                );
                let text = fs::read_to_string(path)?;
                candidates.extend(
                    text.trim_start_matches('\u{feff}')
                        .lines()
                        .map(str::trim)
                        .filter(|l| !l.is_empty() && !l.starts_with('#'))
                        .map(str::to_string),
                );
            }
            if settings.check.dictionary {
                candidates
                    .extend(lexicon.bounded_names(settings.limit.saturating_mul(10).min(10000))?);
            }
            ensure!(
                !candidates.is_empty(),
                "Provide usernames, --file names.txt, or --dictionary"
            );
            let mut normalized = Vec::new();
            let mut seen = HashSet::new();
            let mut outcome = scanner::ScanOutcome::default();
            for raw in candidates {
                let name = normalize_username(&raw)?;
                if !seen.insert(name.clone()) {
                    continue;
                }
                let mut row = Listing::unknown(&name);
                lexicon.annotate(&mut row)?;
                if matches_name(&row, &settings) {
                    normalized.push(name);
                } else {
                    outcome.hidden += 1;
                }
            }
            normalized.truncate(settings.limit);
            output::log(
                &settings,
                "INFO",
                &format!(
                    "{} filtered usernames to check; {} hidden",
                    normalized.len(),
                    outcome.hidden
                ),
            );
            if !normalized.is_empty() {
                let mut client = FragmentClient::new(&settings)?;
                for (index, name) in normalized.iter().enumerate() {
                    output::log(
                        &settings,
                        "INFO",
                        &format!("Check {}/{} | @{name}", index + 1, normalized.len()),
                    );
                    match client.detail(name) {
                        Ok(mut row) => {
                            lexicon.annotate(&mut row)?;
                            outcome.items.push(row);
                        }
                        Err(e) => {
                            outcome.error = Some(format!("{e:#}"));
                            break;
                        }
                    }
                }
                outcome.requests = client.requests;
            }
            output::log(&settings, "INFO", "Checked names that pass lexical filters remain visible even over budget or without an offer.");
            outcome
        }
        _ => unreachable!("dictionary operations handled above"),
    };
    let examined = outcome.items.len();
    let selection = select_report(
        outcome.items,
        &settings,
        &lexicon,
        settings.mode == RunMode::Check,
    )?;
    let report = Report {
        source: "https://fragment.com",
        demo,
        complete: outcome.error.is_none(),
        error: outcome.error,
        hidden: outcome.hidden,
        batches: outcome.batches,
        checked_at: chrono::Utc::now().to_rfc3339(),
        max_ton: settings.max_ton,
        reserve_ton: settings.reserve_ton,
        examined,
        requests: outcome.requests,
        listings: selection.listings,
        selection: selection.stats,
    };
    if let Some(path) = reports::save(&report, &settings)? {
        output::log(
            &settings,
            "INFO",
            &format!("Reports saved: {}", path.display()),
        );
    }
    output::export(&report, &cli)?;
    output::print_report(&report, &settings, settings.console.color)?;
    if let Some(error) = report.error {
        anyhow::bail!("Incomplete scan: {error}");
    }
    Ok(())
}
fn demo_listings() -> Vec<Listing> {
    let mut rows = Vec::new();
    for (name, status, buy, bid, start, last) in [
        ("brain", Status::Sale, Some("8"), None, None, None),
        ("brainer", Status::Sale, Some("4"), None, None, None),
        ("alice", Status::Sale, Some("12.5"), None, None, None),
        ("paris", Status::StartAuction, None, None, Some("7"), None),
        (
            "mario",
            Status::Auction,
            Some("20"),
            Some("10.5"),
            None,
            Some("10"),
        ),
        (
            "nike",
            Status::Auction,
            None,
            Some("15.75"),
            None,
            Some("15"),
        ),
        ("moon", Status::Auction, None, Some("3"), None, None),
        ("winter", Status::Sold, None, None, None, None),
        ("unknownsample", Status::Unknown, None, None, None, None),
        ("tesla", Status::Unavailable, None, None, None, None),
    ] {
        let amount = |s: Option<&str>| s.map(|v| v.parse::<Ton>().expect("demo amount"));
        let mut row = Listing::unknown(name);
        row.status = status;
        row.buy_now = amount(buy);
        row.minimum_bid = amount(bid);
        row.start_bid = amount(start);
        row.last_bid = amount(last);
        if status == Status::Auction {
            row.ends_at = Some(chrono::Utc::now().timestamp() + 86400);
        }
        row.evidence = "SIMULATED -- no Fragment request, no real availability asserted".into();
        rows.push(row);
    }
    rows
}
