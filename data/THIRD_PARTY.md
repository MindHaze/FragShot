# Dictionary Data Attribution

FragShot application code is MIT licensed. Imported lexical data keeps its own licenses; the application license does not replace them.

## Wiktionary / Kaikki

- Original contributors: Wiktionary contributors, https://www.wiktionary.org/.
- Dataset extraction: Tatu Ylonen and Wiktextract contributors, https://github.com/tatuylonen/wiktextract.
- Download index: https://kaikki.org/dictionary/rawdata.html.
- Configured source: https://kaikki.org/dictionary/raw-wiktextract-data.jsonl.gz.
- Text data: Creative Commons Attribution-ShareAlike 4.0, https://creativecommons.org/licenses/by-sa/4.0/, and applicable GNU Free Documentation License terms, https://www.gnu.org/licenses/fdl-1.3.html. Review the current Wiktionary source terms for redistributed data.
- Changes made by FragShot: language filtering; username normalization/transliteration; metadata-based category mapping; deduplication; extraction into a local SQLite index. Original words, source, language, and transformation evidence are retained in each analysis.
- Source URLs and import timestamps are recorded in the index manifest. These are observations, not an assertion of an immutable upstream release.

## FrequencyWords

- Project: Hermit Dave and FrequencyWords contributors, https://github.com/hermitdave/FrequencyWords.
- Data: Creative Commons Attribution-ShareAlike 4.0. Generation code is MIT licensed.
- Underlying frequency lists: OpenSubtitles2016 / OpenSubtitles2018 via OPUS, https://opus.nlpl.eu/.
- Optional downloaded lists are used only as frequency ranking evidence, never as word definitions or proof of semantic categories.

## Limits

Neither dataset covers every word, person name, slang term, trademark, or company. Profanity labels are descriptive metadata, not endorsements. Dictionary language is taken from each entry, not inferred from ASCII spelling. Semantic labels and OG tiers are heuristic research aids, not valuations or purchasing rights.
