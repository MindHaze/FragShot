use crate::{
    config::DictionarySettings,
    model::{normalize_username, LexicalAnalysis, Tier},
};
use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImportStats {
    pub records: u64,
    pub accepted: u64,
    pub skipped: u64,
    pub unique_names: u64,
    pub languages: BTreeMap<String, u64>,
    pub categories: BTreeMap<String, u64>,
    pub source: String,
    pub installed_at: String,
    pub license: String,
    #[serde(default)]
    pub frequency_sources: Vec<String>,
}
#[derive(Serialize, Deserialize)]
struct Manifest {
    schema: u32,
    database: String,
    stats: ImportStats,
}

pub struct DictionaryStore {
    connection: Connection,
    pub stats: ImportStats,
}
impl DictionaryStore {
    pub fn open(directory: &Path) -> Result<Option<Self>> {
        let pointer = directory.join("current.json");
        if !pointer.exists() {
            return Ok(None);
        }
        let manifest: Manifest = serde_json::from_reader(fs::File::open(pointer)?)?;
        ensure!(
            manifest.schema == 1
                && manifest.database.starts_with("index-")
                && manifest.database.ends_with(".sqlite")
                && manifest
                    .database
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-._".contains(&c)),
            "Invalid dictionary manifest"
        );
        let connection = Connection::open_with_flags(
            directory.join(manifest.database),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        Ok(Some(Self {
            connection,
            stats: manifest.stats,
        }))
    }
    pub fn lookup(&self, name: &str) -> Result<Vec<LexicalAnalysis>> {
        let mut stmt = self.connection.prepare_cached(
            "SELECT analysis FROM entries WHERE name=? ORDER BY language, analysis LIMIT 128",
        )?;
        let values = stmt.query_map([name], |row| row.get::<_, String>(0))?;
        values.map(|v| Ok(serde_json::from_str(&v?)?)).collect()
    }
    pub fn frequency_ranks(
        &self,
        languages: &[String],
    ) -> Result<BTreeMap<String, BTreeMap<String, usize>>> {
        let mut stmt = self.connection.prepare("SELECT language,name,MAX(frequency) AS f FROM entries WHERE frequency>0 GROUP BY language,name ORDER BY language,f DESC,name")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut ranks = BTreeMap::<String, BTreeMap<String, usize>>::new();
        for row in rows {
            let (language, name) = row?;
            if !languages.is_empty() && !languages.contains(&language) {
                continue;
            }
            let entries = ranks.entry(language).or_default();
            if entries.len() < 50000 {
                let rank = entries.len() + 1;
                entries.insert(name, rank);
            }
        }
        Ok(ranks)
    }
    pub fn names(&self, limit: usize) -> Result<Vec<String>> {
        let mut stmt = self.connection.prepare(
            "SELECT name FROM entries GROUP BY name ORDER BY MAX(frequency) DESC, name LIMIT ?",
        )?;
        let rows = stmt.query_map([limit as i64], |row| row.get(0))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
}

pub fn normalized_word(word: &str, transliterate: bool) -> Option<(String, String)> {
    if word.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return None;
    }
    if let Ok(name) = normalize_username(word) {
        return Some((name, "exact".into()));
    }
    if !transliterate {
        return None;
    }
    let ascii = deunicode::deunicode(word).to_ascii_lowercase();
    normalize_username(&ascii).ok().map(|name| {
        (
            name,
            "transliteration:deunicode (not exact spelling)".into(),
        )
    })
}
fn strings(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().or_else(|| v.get("name").and_then(Value::as_str)))
        .map(str::to_string)
        .collect()
}
fn categories(record: &Value, sense: &Value) -> Vec<String> {
    let mut labels = strings(record, "categories");
    labels.extend(strings(sense, "categories"));
    labels.extend(strings(record, "topics"));
    labels.extend(strings(sense, "topics"));
    labels.extend(strings(sense, "tags"));
    labels.extend(strings(record, "tags"));
    let mut found = BTreeSet::new();
    let pos = record.get("pos").and_then(Value::as_str).unwrap_or("");
    if [
        "noun", "verb", "adj", "adv", "intj", "interj", "pron", "prep", "conj", "num", "det",
    ]
    .contains(&pos)
    {
        found.insert("word".to_string());
    }
    for raw in labels {
        let label = raw
            .rsplit(':')
            .next()
            .unwrap_or(&raw)
            .to_lowercase()
            .replace(['_', '-'], " ");
        let label = format!(
            " {} ",
            label.split_whitespace().collect::<Vec<_>>().join(" ")
        );
        for (needles, category) in [
            (&["given names", "first names"] as &[&str], "first_name"),
            (&["surnames", "family names"], "surname"),
            (&["brand names", "trademarks"], "brand"),
            (&["company names", "companies"], "company"),
            (&["cities", "city names"], "city"),
            (
                &[
                    "foods",
                    "food",
                    "beverages",
                    "cuisine",
                    "fruits",
                    "vegetables",
                ],
                "food",
            ),
            (&["slang", "colloquial"], "slang"),
            (
                &[
                    "vulgar",
                    "offensive",
                    "derogatory",
                    "profanity",
                    "slur",
                    "obscene",
                ],
                "profanity",
            ),
            (&["video games", "video game characters"], "video_game"),
            (
                &["animals", "mammals", "birds", "fish", "insects"],
                "animal",
            ),
            (&["plants", "flowers", "forests", "weather"], "nature"),
            (&["astronomy", "planets", "stars", "space"], "space"),
            (
                &["computing", "technology", "software", "programming"],
                "technology",
            ),
            (&["physics", "chemistry", "biology", "science"], "science"),
            (&["anatomy", "body parts", "organs"], "body"),
            (&["colors", "colours"], "color"),
            (&["time", "seasons", "months", "days"], "time"),
        ] {
            if needles.iter().any(|n| label.contains(&format!(" {n} "))) {
                found.insert(category.to_string());
            }
        }
    }
    if found.is_empty() {
        found.insert("unknown".into());
    }
    found.into_iter().collect()
}
fn analyses(
    record: &Value,
    options: &DictionarySettings,
    source: &str,
) -> Vec<(String, LexicalAnalysis)> {
    let Some(word) = record.get("word").and_then(Value::as_str) else {
        return vec![];
    };
    let language = record
        .get("lang_code")
        .and_then(Value::as_str)
        .unwrap_or("und");
    let roman = record
        .get("forms")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find_map(|form| {
            let tags = strings(form, "tags");
            if tags.iter().any(|t| t == "romanization") {
                form.get("form").and_then(Value::as_str)
            } else {
                None
            }
        });
    let normalized = if !word.is_ascii() {
        roman
            .and_then(|s| normalized_word(s, false))
            .map(|(n, _)| (n, "source romanization".into()))
            .or_else(|| normalized_word(word, options.transliterate))
    } else {
        normalized_word(word, options.transliterate)
    };
    let Some((name, normalization)) = normalized else {
        return vec![];
    };
    if record
        .get("pos")
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
    {
        return vec![];
    }
    let Some(senses) = record.get("senses").and_then(Value::as_array) else {
        return vec![];
    };
    let mut out = Vec::new();
    for sense in senses {
        let base = ["form_of", "alt_of"].into_iter().find_map(|key| {
            sense
                .get(key)
                .and_then(Value::as_array)
                .and_then(|v| v.first())
                .and_then(|v| v.get("word"))
                .and_then(Value::as_str)
        });
        if !sense.is_object()
            || (base.is_none()
                && strings(sense, "tags")
                    .iter()
                    .any(|t| t == "form-of" || t == "alt-of" || t == "no-gloss"))
        {
            continue;
        }
        let tier = if base.is_some() {
            Tier::SemiOg
        } else {
            Tier::Og
        };
        let analysis = LexicalAnalysis {
            tier,
            original: word.into(),
            language: language.into(),
            categories: categories(record, sense),
            base: base.map(str::to_string),
            rule: if base.is_some() {
                "dictionary form/variant".into()
            } else {
                "dictionary lemma".into()
            },
            source: source.into(),
            normalization: normalization.clone(),
        };
        if !out.iter().any(|(_, a)| a == &analysis) {
            out.push((name.clone(), analysis));
        }
    }
    out
}

pub fn import_reader<R: Read>(
    reader: R,
    options: &DictionarySettings,
    languages: &[String],
    source: &str,
    cancelled: &AtomicBool,
    mut progress: impl FnMut(u64, u64),
) -> Result<ImportStats> {
    fs::create_dir_all(&options.directory)?;
    let id = format!(
        "{}-{}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        std::process::id()
    );
    let filename = format!("index-{id}.sqlite");
    let db_path = options.directory.join(&filename);
    let mut connection = Connection::open(&db_path)?;
    connection.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA synchronous=NORMAL; CREATE TABLE entries(name TEXT NOT NULL, language TEXT NOT NULL, analysis TEXT NOT NULL, frequency INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(name,language,analysis));")?;
    let result = (|| -> Result<ImportStats> {
        let mut stats = ImportStats {
            source: source.into(),
            installed_at: chrono::Utc::now().to_rfc3339(),
            license: "Wiktionary: CC BY-SA 4.0 / GFDL; see data/THIRD_PARTY.md".into(),
            ..ImportStats::default()
        };
        let mut reader = BufReader::new(reader);
        let mut line = Vec::new();
        let mut total = 0u64;
        let mut transaction = connection.transaction()?;
        loop {
            ensure!(
                !cancelled.load(Ordering::Relaxed),
                "Dictionary import cancelled; previous index preserved"
            );
            line.clear();
            let count = reader
                .by_ref()
                .take(options.max_record_bytes as u64 + 1)
                .read_until(b'\n', &mut line)?;
            if count == 0 {
                break;
            }
            total = total
                .checked_add(count as u64)
                .context("Import size overflow")?;
            ensure!(
                count <= options.max_record_bytes && total <= options.max_uncompressed_bytes,
                "Dictionary record/decompressed byte limit exceeded"
            );
            if line.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            stats.records += 1;
            let record: Value = serde_json::from_slice(&line)
                .with_context(|| format!("Invalid dictionary JSON at record {}", stats.records))?;
            let language = record
                .get("lang_code")
                .and_then(Value::as_str)
                .unwrap_or("");
            if !languages.is_empty() && !languages.iter().any(|l| l == language) {
                stats.skipped += 1;
                continue;
            }
            let values = analyses(&record, options, source);
            if values.is_empty() {
                stats.skipped += 1;
            }
            for (name, analysis) in values {
                let count = transaction
                    .prepare_cached(
                        "INSERT OR IGNORE INTO entries(name,language,analysis) VALUES(?,?,?)",
                    )?
                    .execute(params![
                        name,
                        analysis.language,
                        serde_json::to_string(&analysis)?
                    ])?;
                stats.accepted += count as u64;
            }
            if stats.records.is_multiple_of(10000) {
                transaction.commit()?;
                progress(stats.records, total);
                transaction = connection.transaction()?;
            }
        }
        transaction.commit()?;
        // A source relationship is only usable when its independent root is present in the same language.
        connection.execute_batch("CREATE INDEX roots ON entries(language, json_extract(analysis,'$.original'), json_extract(analysis,'$.tier')); DELETE FROM entries AS form WHERE json_extract(form.analysis,'$.tier')='semi_og' AND NOT EXISTS (SELECT 1 FROM entries AS root WHERE root.language=form.language AND json_extract(root.analysis,'$.original')=json_extract(form.analysis,'$.base') AND json_extract(root.analysis,'$.tier')='og');")?;
        stats.accepted = connection.query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))?;
        ensure!(
            stats.accepted > 0,
            "Dictionary has no usable entries for the selected languages"
        );
        ensure!(
            !cancelled.load(Ordering::Relaxed),
            "Dictionary import cancelled; previous index preserved"
        );
        stats.unique_names =
            connection.query_row("SELECT COUNT(DISTINCT name) FROM entries", [], |r| r.get(0))?;
        let mut stmt = connection
            .prepare("SELECT language, COUNT(DISTINCT name) FROM entries GROUP BY language")?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u64>(1)?)))? {
            let (l, n) = row?;
            stats.languages.insert(l, n);
        }
        drop(stmt);
        let mut stmt = connection.prepare("SELECT j.value, COUNT(DISTINCT e.name) FROM entries e, json_each(e.analysis, '$.categories') j GROUP BY j.value")?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u64>(1)?)))? {
            let (c, n) = row?;
            stats.categories.insert(c, n);
        }
        drop(stmt);
        crate::frequency::download_into(&options.frequency_urls, &mut connection, cancelled)?;
        stats.frequency_sources = options.frequency_urls.clone();
        ensure!(
            !cancelled.load(Ordering::Relaxed),
            "Dictionary import cancelled; previous index preserved"
        );
        progress(stats.records, total);
        Ok(stats)
    })();
    drop(connection);
    match result {
        Ok(stats) => {
            let manifest = Manifest {
                schema: 1,
                database: filename,
                stats: stats.clone(),
            };
            let pointer = options.directory.join("current.json");
            let staged = options.directory.join(format!("manifest-{id}.json"));
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&staged)?;
            file.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
            file.sync_all()?;
            drop(file);
            // rename replaces a regular file atomically on supported Windows and Unix filesystems.
            fs::rename(staged, pointer)
                .context("Activating dictionary index; old manifest remains on failure")?;
            Ok(stats)
        }
        Err(error) => {
            let _ = fs::remove_file(db_path);
            Err(error)
        }
    }
}

pub fn import_file(
    path: &Path,
    options: &DictionarySettings,
    languages: &[String],
    cancelled: &AtomicBool,
    progress: impl FnMut(u64, u64),
) -> Result<ImportStats> {
    let file = fs::File::open(path)?;
    ensure!(
        file.metadata()?.len() <= options.max_compressed_bytes,
        "Dictionary input exceeds compressed/input byte limit"
    );
    let source = format!("{} (imported Wiktionary data)", path.display());
    if path.extension().is_some_and(|e| e == "gz") {
        import_reader(
            flate2::read::MultiGzDecoder::new(file),
            options,
            languages,
            &source,
            cancelled,
            progress,
        )
    } else {
        import_reader(file, options, languages, &source, cancelled, progress)
    }
}

pub fn sync(
    options: &DictionarySettings,
    languages: &[String],
    cancelled: &AtomicBool,
    progress: impl FnMut(u64, u64),
) -> Result<ImportStats> {
    let url = reqwest::Url::parse(&options.source_url)?;
    ensure!(
        url.scheme() == "https"
            && url.host_str() == Some("kaikki.org")
            && url.username().is_empty()
            && url.password().is_none(),
        "Dictionary sync only accepts HTTPS kaikki.org sources"
    );
    let client = reqwest::blocking::Client::builder()
        .https_only(true)
        .user_agent("FragShot/0.2 dictionary research")
        .timeout(Duration::from_secs(options.timeout_seconds))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() < 5
                && attempt.url().scheme() == "https"
                && attempt.url().host_str() == Some("kaikki.org")
            {
                attempt.follow()
            } else {
                attempt.error("Dictionary redirect refused")
            }
        }))
        .build()?;
    let response = client.get(url).send()?;
    ensure!(
        response.status().is_success(),
        "Dictionary download HTTP {}",
        response.status()
    );
    ensure!(
        response.content_length().unwrap_or(0) <= options.max_compressed_bytes,
        "Dictionary download exceeds compressed limit"
    );
    let limited = ByteLimit {
        inner: response,
        remaining: options.max_compressed_bytes,
    };
    import_reader(
        flate2::read::MultiGzDecoder::new(limited),
        options,
        languages,
        &options.source_url,
        cancelled,
        progress,
    )
}
struct ByteLimit<R> {
    inner: R,
    remaining: u64,
}
impl<R: Read> Read for ByteLimit<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        if self.remaining == 0 {
            let mut extra = [0u8];
            return if self.inner.read(&mut extra)? == 0 {
                Ok(0)
            } else {
                Err(std::io::Error::other(
                    "Compressed dictionary limit exceeded",
                ))
            };
        }
        let size = buffer
            .len()
            .min(self.remaining.min(usize::MAX as u64) as usize);
        let n = self.inner.read(&mut buffer[..size])?;
        self.remaining -= n as u64;
        Ok(n)
    }
}

pub fn language_label(code: &str) -> &str {
    match code {
        "en" => "English",
        "fr" => "French",
        "ru" => "Russian",
        "de" => "German",
        "es" => "Spanish",
        "it" => "Italian",
        "pt" => "Portuguese",
        _ => "Unspecified",
    }
}
