use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection};
use std::{
    io::{BufRead, BufReader, Read},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

pub fn source_language(source: &str) -> Result<String> {
    let url = reqwest::Url::parse(source)?;
    ensure!(
        url.scheme() == "https"
            && url.host_str() == Some("raw.githubusercontent.com")
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "Frequency source must be a public HTTPS FrequencyWords raw URL"
    );
    let parts: Vec<_> = url.path().trim_start_matches('/').split('/').collect();
    ensure!(
        parts.len() == 7
            && parts[0] == "hermitdave"
            && parts[1] == "FrequencyWords"
            && parts[3] == "content"
            && ["2016", "2018"].contains(&parts[4]),
        "Unexpected FrequencyWords source path"
    );
    let language = parts[5];
    ensure!(
        !language.is_empty()
            && language.len() <= 16
            && language
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b == b'-')
            && [
                format!("{language}_50k.txt"),
                format!("{language}_full.txt")
            ]
            .contains(&parts[6].to_string()),
        "Invalid frequency language/file"
    );
    Ok(language.into())
}

pub fn apply_reader(
    reader: impl Read,
    db: &mut Connection,
    language: &str,
    cancelled: &AtomicBool,
) -> Result<u64> {
    let mut reader = BufReader::new(reader);
    let transaction = db.transaction()?;
    let mut updated = 0;
    let mut total = 0;
    let mut line = String::new();
    loop {
        ensure!(
            !cancelled.load(Ordering::Relaxed),
            "Frequency import cancelled"
        );
        line.clear();
        let size = reader.by_ref().take(1025).read_line(&mut line)?;
        if size == 0 {
            break;
        }
        total += size;
        ensure!(
            size <= 1024 && total <= 16 * 1024 * 1024,
            "Frequency list exceeds line/body limit"
        );
        if line.trim().is_empty() {
            continue;
        }
        let (word, frequency) = line
            .trim()
            .rsplit_once(char::is_whitespace)
            .context("Invalid frequency row")?;
        let frequency: i64 = frequency
            .trim()
            .parse()
            .context("Invalid frequency count")?;
        ensure!(frequency >= 0, "Negative frequency count");
        if let Some((name, _)) = crate::dictionary::normalized_word(word, true) {
            updated += transaction
                .prepare_cached(
                    "UPDATE entries SET frequency=MAX(frequency,?) WHERE name=? AND language=?",
                )?
                .execute(params![frequency, name, language])? as u64;
        }
    }
    transaction.commit()?;
    Ok(updated)
}

pub fn download_into(
    sources: &[String],
    db: &mut Connection,
    cancelled: &AtomicBool,
) -> Result<()> {
    if sources.is_empty() {
        return Ok(());
    }
    let client = reqwest::blocking::Client::builder()
        .https_only(true)
        .user_agent("FragShot/0.2 dictionary research")
        .timeout(Duration::from_secs(120))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    for source in sources {
        let language = source_language(source)?;
        ensure!(
            !cancelled.load(Ordering::Relaxed),
            "Frequency import cancelled"
        );
        let response = client.get(source).send()?.error_for_status()?;
        ensure!(
            response.status().as_u16() == 200
                && response.content_length().unwrap_or(0) <= 16 * 1024 * 1024,
            "Unexpected or oversized frequency response"
        );
        apply_reader(response, db, &language, cancelled)?;
    }
    Ok(())
}
