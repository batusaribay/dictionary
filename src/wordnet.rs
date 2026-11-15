//! Offline WordNet 3.0 lookup.
//!
//! The WordNet database is bundled (gzip-compressed) in the binary, so lookups
//! never touch the network. Indexes are decompressed and parsed lazily on the
//! first query.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::sync::OnceLock;

use flate2::read::GzDecoder;
use serde::Serialize;

const NOUN_INDEX: &[u8] = include_bytes!("../assets/wordnet/index.noun.gz");
const NOUN_DATA: &[u8] = include_bytes!("../assets/wordnet/data.noun.gz");
const NOUN_EXC: &[u8] = include_bytes!("../assets/wordnet/noun.exc.gz");

const VERB_INDEX: &[u8] = include_bytes!("../assets/wordnet/index.verb.gz");
const VERB_DATA: &[u8] = include_bytes!("../assets/wordnet/data.verb.gz");
const VERB_EXC: &[u8] = include_bytes!("../assets/wordnet/verb.exc.gz");

const ADJ_INDEX: &[u8] = include_bytes!("../assets/wordnet/index.adj.gz");
const ADJ_DATA: &[u8] = include_bytes!("../assets/wordnet/data.adj.gz");
const ADJ_EXC: &[u8] = include_bytes!("../assets/wordnet/adj.exc.gz");

const ADV_INDEX: &[u8] = include_bytes!("../assets/wordnet/index.adv.gz");
const ADV_DATA: &[u8] = include_bytes!("../assets/wordnet/data.adv.gz");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pos {
    Noun,
    Verb,
    Adjective,
    Adverb,
}

impl Pos {
    fn query_char(self) -> &'static str {
        match self {
            Pos::Noun => "n",
            Pos::Verb => "v",
            Pos::Adjective => "a",
            Pos::Adverb => "r",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Pos::Noun => "noun",
            Pos::Verb => "verb",
            Pos::Adjective => "adjective",
            Pos::Adverb => "adverb",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Sense {
    pub definition: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub synonyms: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PosGroup {
    pub part_of_speech: &'static str,
    pub senses: Vec<Sense>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Lookup {
    pub query: String,
    pub found: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched: Option<String>,
    /// Set when the query was inflected or otherwise needed a fallback lookup.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_from: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<PosGroup>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub suggestions: Vec<String>,
}

#[derive(Debug)]
struct IndexEntry {
    lemma: String,
    offsets: Vec<u32>,
    /// Number of senses that appear in WordNet's tagged corpora; a rough
    /// frequency signal used to put the most common part of speech first.
    tagged_senses: usize,
}

#[derive(Debug)]
struct Synset {
    words: Vec<String>,
    definition: String,
    examples: Vec<String>,
}

struct PosFile {
    pos: Pos,
    index_gz: &'static [u8],
    data_gz: &'static [u8],
    exc_gz: Option<&'static [u8]>,
    index: OnceLock<HashMap<String, IndexEntry>>,
    data: OnceLock<String>,
    offsets: OnceLock<HashMap<u32, (usize, usize)>>,
    exceptions: OnceLock<HashMap<String, Vec<String>>>,
}

impl PosFile {
    fn new(
        pos: Pos,
        index_gz: &'static [u8],
        data_gz: &'static [u8],
        exc_gz: Option<&'static [u8]>,
    ) -> Self {
        Self {
            pos,
            index_gz,
            data_gz,
            exc_gz,
            index: OnceLock::new(),
            data: OnceLock::new(),
            offsets: OnceLock::new(),
            exceptions: OnceLock::new(),
        }
    }

    fn index(&self) -> &HashMap<String, IndexEntry> {
        self.index.get_or_init(|| {
            let text = decompress(self.index_gz);
            build_index(&text, self.pos)
        })
    }

    fn data(&self) -> &str {
        self.data.get_or_init(|| decompress(self.data_gz))
    }

    fn offsets(&self) -> &HashMap<u32, (usize, usize)> {
        self.offsets.get_or_init(|| build_offsets(self.data()))
    }

    fn exceptions(&self) -> &HashMap<String, Vec<String>> {
        self.exceptions.get_or_init(|| match self.exc_gz {
            Some(gz) => build_exceptions(&decompress(gz)),
            None => HashMap::new(),
        })
    }

    fn line_at(&self, offset: u32) -> Option<&str> {
        let (start, end) = *self.offsets().get(&offset)?;
        self.data().get(start..end)
    }

    fn synset_at(&self, offset: u32) -> Option<Synset> {
        parse_synset(self.line_at(offset)?)
    }
}

struct Dictionary {
    files: [PosFile; 4],
}

impl Dictionary {
    fn new() -> Self {
        Self {
            files: [
                PosFile::new(Pos::Noun, NOUN_INDEX, NOUN_DATA, Some(NOUN_EXC)),
                PosFile::new(Pos::Verb, VERB_INDEX, VERB_DATA, Some(VERB_EXC)),
                PosFile::new(Pos::Adjective, ADJ_INDEX, ADJ_DATA, Some(ADJ_EXC)),
                PosFile::new(Pos::Adverb, ADV_INDEX, ADV_DATA, None),
            ],
        }
    }

    fn lookup(&self, raw: &str) -> Lookup {
        let query = raw.trim().to_string();
        let key = normalize(&query);

        if key.is_empty() {
            return Lookup {
                query,
                found: false,
                matched: None,
                resolved_from: None,
                groups: Vec::new(),
                suggestions: Vec::new(),
            };
        }

        let (mut matched, mut groups) = self.find(&key);

        if groups.is_empty() {
            let bases: Vec<String> = self
                .files
                .iter()
                .filter_map(|file| file.exceptions().get(&key))
                .flatten()
                .cloned()
                .collect();
            if !bases.is_empty() {
                (matched, groups) = self.find_many(&bases);
            }
        }

        if groups.is_empty() {
            let mut best: Option<(usize, Option<String>, Vec<PosGroup>)> = None;
            for candidate in morph_candidates(&key) {
                let (candidate_match, candidate_groups) = self.find(&candidate);
                if candidate_groups.is_empty() {
                    continue;
                }
                let score = candidate_groups
                    .iter()
                    .map(|group| group.senses.len())
                    .sum();
                if best
                    .as_ref()
                    .is_none_or(|(best_score, _, _)| score > *best_score)
                {
                    best = Some((score, candidate_match, candidate_groups));
                }
            }
            if let Some((_, candidate_match, candidate_groups)) = best {
                matched = candidate_match;
                groups = candidate_groups;
            }
        }

        let found = !groups.is_empty();
        let resolved_from = if found {
            matched
                .as_deref()
                .map(normalize)
                .filter(|matched_key| matched_key != &key)
                .map(|_| query.clone())
        } else {
            None
        };

        // Preserve the user's capitalization for exact matches ("NASA", "U.S.").
        if found && resolved_from.is_none() {
            let display = display_form(&query);
            if !display.is_empty() {
                matched = Some(display);
            }
        }

        let suggestions = if found {
            Vec::new()
        } else {
            self.suggest(&key, 5)
        };

        Lookup {
            query,
            found,
            matched,
            resolved_from,
            groups,
            suggestions,
        }
    }

    fn find(&self, key: &str) -> (Option<String>, Vec<PosGroup>) {
        let mut matched = None;
        let mut groups: Vec<(usize, PosGroup)> = Vec::new();

        for file in &self.files {
            let Some(entry) = file.index().get(key) else {
                continue;
            };

            if matched.is_none() {
                matched = Some(entry.lemma.clone());
            }

            let mut senses = Vec::new();
            let mut seen_offsets = HashSet::new();

            for offset in &entry.offsets {
                if !seen_offsets.insert(*offset) {
                    continue;
                }
                let Some(synset) = file.synset_at(*offset) else {
                    continue;
                };
                if synset.definition.is_empty() {
                    continue;
                }

                let mut synonyms = Vec::new();
                let mut seen_synonyms = HashSet::new();
                for word in &synset.words {
                    let normalized = normalize(word);
                    if normalized != key
                        && !normalized.is_empty()
                        && seen_synonyms.insert(normalized)
                    {
                        synonyms.push(word.clone());
                    }
                }

                senses.push(Sense {
                    definition: synset.definition,
                    examples: synset.examples,
                    synonyms,
                });
            }

            if !senses.is_empty() {
                groups.push((
                    entry.tagged_senses,
                    PosGroup {
                        part_of_speech: file.pos.label(),
                        senses,
                    },
                ));
            }
        }

        groups.sort_by(|(tags_a, _), (tags_b, _)| tags_b.cmp(tags_a));
        (
            matched,
            groups.into_iter().map(|(_, group)| group).collect(),
        )
    }

    fn find_many(&self, bases: &[String]) -> (Option<String>, Vec<PosGroup>) {
        let mut matched: Vec<String> = Vec::new();
        let mut groups: Vec<PosGroup> = Vec::new();
        let mut seen_bases = HashSet::new();

        for base in bases {
            if !seen_bases.insert(normalize(base)) {
                continue;
            }
            let (base_match, base_groups) = self.find(&normalize(base));
            if base_groups.is_empty() {
                continue;
            }
            if let Some(base_match) = base_match
                && !matched.contains(&base_match)
            {
                matched.push(base_match);
            }
            for group in base_groups {
                if let Some(existing) = groups
                    .iter_mut()
                    .find(|existing| existing.part_of_speech == group.part_of_speech)
                {
                    existing.senses.extend(group.senses);
                } else {
                    groups.push(group);
                }
            }
        }

        let matched = if matched.is_empty() {
            None
        } else {
            Some(matched.join(" / "))
        };
        (matched, groups)
    }

    fn suggest(&self, key: &str, limit: usize) -> Vec<String> {
        let target = key.replace('_', " ");
        let target_len = target.chars().count();
        let max_distance = if target_len <= 3 {
            1
        } else if target_len <= 7 {
            2
        } else {
            3
        };

        let mut candidates: Vec<(usize, String)> = Vec::new();
        let mut seen = HashSet::new();

        for file in &self.files {
            for candidate_key in file.index().keys() {
                let candidate = candidate_key.replace('_', " ");
                if !seen.insert(candidate.clone()) {
                    continue;
                }
                let candidate_len = candidate.chars().count();
                if candidate_len.abs_diff(target_len) > max_distance {
                    continue;
                }
                let distance = levenshtein(&target, &candidate, max_distance);
                if let Some(distance) = distance {
                    candidates.push((distance, candidate));
                }
            }
        }

        candidates.sort_by(|(dist_a, word_a), (dist_b, word_b)| {
            dist_a
                .cmp(dist_b)
                .then_with(|| word_a.len().cmp(&word_b.len()))
                .then_with(|| word_a.cmp(word_b))
        });
        candidates
            .into_iter()
            .take(limit)
            .map(|(_, word)| word)
            .collect()
    }
}

/// The public dictionary handle. Construction is cheap; data is loaded lazily.
pub struct WordNet {
    dictionary: Dictionary,
}

impl WordNet {
    pub fn new() -> Self {
        Self {
            dictionary: Dictionary::new(),
        }
    }

    pub fn lookup(&self, word: &str) -> Lookup {
        self.dictionary.lookup(word)
    }
}

fn decompress(bytes: &[u8]) -> String {
    let mut decoder = GzDecoder::new(bytes);
    let mut text = String::new();
    decoder
        .read_to_string(&mut text)
        .expect("bundled WordNet data is corrupt");
    text
}

fn build_index(text: &str, pos: Pos) -> HashMap<String, IndexEntry> {
    let mut index: HashMap<String, IndexEntry> = HashMap::new();

    for line in text.lines() {
        if line.is_empty() || line.as_bytes().first().is_some_and(u8::is_ascii_whitespace) {
            continue;
        }

        let mut fields = line.split_whitespace();
        let Some(lemma) = fields.next() else {
            continue;
        };
        let Some(line_pos) = fields.next() else {
            continue;
        };
        if line_pos != pos.query_char() {
            continue;
        }
        let Some(synset_count) = fields.next().and_then(|n| n.parse::<usize>().ok()) else {
            continue;
        };
        let Some(pointer_count) = fields.next().and_then(|n| n.parse::<usize>().ok()) else {
            continue;
        };

        for _ in 0..pointer_count {
            if fields.next().is_none() {
                break;
            }
        }
        let _sense_count = fields.next();
        let tagged_senses = fields
            .next()
            .and_then(|count| count.parse::<usize>().ok())
            .unwrap_or(0);

        let mut offsets = Vec::with_capacity(synset_count);
        for _ in 0..synset_count {
            let Some(offset) = fields.next().and_then(|n| n.parse::<u32>().ok()) else {
                break;
            };
            offsets.push(offset);
        }

        if offsets.is_empty() {
            continue;
        }

        let entry = index
            .entry(lemma.to_lowercase())
            .or_insert_with(|| IndexEntry {
                lemma: lemma.replace('_', " "),
                offsets: Vec::new(),
                tagged_senses: 0,
            });
        entry.offsets.extend(offsets);
        entry.tagged_senses += tagged_senses;
    }

    index
}

fn build_offsets(text: &str) -> HashMap<u32, (usize, usize)> {
    let mut offsets = HashMap::new();
    let mut byte_offset = 0;

    for line in text.split_inclusive('\n') {
        if !line.starts_with(' ')
            && let Some(offset) = line
                .split_whitespace()
                .next()
                .and_then(|token| token.parse::<u32>().ok())
        {
            let end = byte_offset + line.trim_end_matches(['\r', '\n']).len();
            offsets.insert(offset, (byte_offset, end));
        }
        byte_offset += line.len();
    }

    offsets
}

fn build_exceptions(text: &str) -> HashMap<String, Vec<String>> {
    let mut exceptions: HashMap<String, Vec<String>> = HashMap::new();
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let Some(inflected) = fields.next() else {
            continue;
        };
        let bases = exceptions.entry(inflected.to_lowercase()).or_default();
        for base in fields {
            let base = base.replace('_', " ");
            if !bases.contains(&base) {
                bases.push(base);
            }
        }
    }
    exceptions
}

fn parse_synset(line: &str) -> Option<Synset> {
    let gloss_start = line.find(" | ").unwrap_or(line.len());
    let body = &line[..gloss_start];
    let fields: Vec<&str> = body.split_whitespace().collect();
    if fields.len() < 5 {
        return None;
    }

    let word_count = usize::from_str_radix(fields[3], 16).ok()?;
    if fields.len() < 4 + word_count * 2 {
        return None;
    }

    let mut words = Vec::with_capacity(word_count);
    for i in 0..word_count {
        words.push(fields[4 + i * 2].replace('_', " "));
    }

    let (definition, examples) = if gloss_start < line.len() {
        split_gloss(&line[gloss_start + 3..])
    } else {
        (String::new(), Vec::new())
    };

    Some(Synset {
        words,
        definition,
        examples,
    })
}

fn split_gloss(gloss: &str) -> (String, Vec<String>) {
    let gloss = gloss.trim();
    let split_at = gloss.find("; \"").or_else(|| gloss.find(";\""));
    let (definition, examples_text) = match split_at {
        Some(index) => (&gloss[..index], &gloss[index + 1..]),
        None => (gloss, ""),
    };

    let definition = definition.trim().trim_end_matches(';').trim().to_string();
    let mut examples = Vec::new();
    let mut rest = examples_text;

    while let Some(open) = rest.find('"') {
        rest = &rest[open + 1..];
        let Some(close) = rest.find('"') else {
            break;
        };
        let example = rest[..close].trim();
        if !example.is_empty() {
            examples.push(example.to_string());
        }
        rest = &rest[close + 1..];
    }

    (definition, examples)
}

/// Normalize a user query to the key format used by WordNet.
pub fn normalize(input: &str) -> String {
    let cleaned = cleaned_input(input);

    let mut result = String::new();
    for part in cleaned.split_whitespace() {
        if !result.is_empty() {
            result.push('_');
        }
        for c in part.chars() {
            let c = match c {
                '\u{2018}' | '\u{2019}' => '\'',
                '\u{2010}' | '\u{2011}' => '-',
                _ => c,
            };
            for lower in c.to_lowercase() {
                result.push(lower);
            }
        }
    }
    result
}

fn display_form(input: &str) -> String {
    cleaned_input(input)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn cleaned_input(input: &str) -> String {
    let mut cleaned = input
        .trim()
        .trim_matches(|c: char| c.is_whitespace() || matches!(c, '"' | '\u{201c}' | '\u{201d}'))
        .to_string();
    cleaned = cleaned
        .trim_end_matches([',', ';', ':', '!', '?'])
        .to_string();
    if let Some(prefix) = cleaned.strip_suffix('.')
        && !prefix.contains('.')
    {
        cleaned.truncate(prefix.len());
    }
    if cleaned.ends_with('\'') && cleaned.len() > 1 {
        cleaned.pop();
    }
    cleaned
}

fn morph_candidates(word: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    let mut push = |candidate: String| {
        if !candidate.is_empty() && candidate != word && !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    };

    if let Some(stem) = word.strip_suffix("n't") {
        let base = match stem {
            "do" | "does" | "did" => "do",
            "is" | "are" | "was" | "were" => "be",
            "have" | "has" | "had" => "have",
            "ca" => "can",
            "wo" => "will",
            "sha" => "shall",
            other => other,
        };
        push(base.to_string());
    }
    if word.ends_with("'re") || word.ends_with("'m") {
        push("be".to_string());
    }
    if word.ends_with("'ve") {
        push("have".to_string());
    }
    if word.ends_with("'ll") {
        push("will".to_string());
    }
    if word.ends_with("'d") {
        push("would".to_string());
    }
    if matches!(
        word,
        "he's"
            | "she's"
            | "it's"
            | "that's"
            | "what's"
            | "who's"
            | "there's"
            | "here's"
            | "where's"
            | "when's"
            | "why's"
            | "how's"
    ) {
        push("be".to_string());
    }

    let undouble = |stem: &str| -> Option<String> {
        let mut chars = stem.chars().rev();
        let last = chars.next()?;
        let previous = chars.next()?;
        if last == previous && last.is_ascii_alphabetic() {
            Some(stem[..stem.len() - last.len_utf8()].to_string())
        } else {
            None
        }
    };

    if let Some(stem) = word.strip_suffix("'s") {
        push(stem.to_string());
    }
    if let Some(stem) = word.strip_suffix("ies") {
        push(format!("{stem}y"));
    }
    if let Some(stem) = word.strip_suffix("ves") {
        push(format!("{stem}f"));
        push(format!("{stem}fe"));
    }
    if let Some(stem) = word.strip_suffix("es") {
        push(stem.to_string());
        if let Some(base) = undouble(stem) {
            push(base);
        }
    }
    if let Some(stem) = word.strip_suffix('s')
        && word.len() > 3
    {
        push(stem.to_string());
    }
    if let Some(stem) = word.strip_suffix("ied") {
        push(format!("{stem}y"));
    }
    if let Some(stem) = word.strip_suffix("ed") {
        push(stem.to_string());
        push(format!("{stem}e"));
        if let Some(base) = undouble(stem) {
            push(base);
        }
    }
    if let Some(stem) = word.strip_suffix("ing") {
        push(stem.to_string());
        push(format!("{stem}e"));
        if let Some(ie_stem) = stem.strip_suffix('y') {
            push(format!("{ie_stem}ie"));
        }
        if let Some(base) = undouble(stem) {
            push(base);
        }
    }
    if let Some(stem) = word.strip_suffix("est") {
        push(stem.to_string());
        if let Some(base) = undouble(stem) {
            push(base);
        }
    }
    if let Some(stem) = word.strip_suffix("er") {
        push(stem.to_string());
        if let Some(base) = undouble(stem) {
            push(base);
        }
    }
    if let Some(stem) = word.strip_suffix("ly") {
        push(stem.to_string());
    }

    candidates
}

/// Bounded Levenshtein distance. Returns `None` when the distance exceeds `max`.
fn levenshtein(a: &str, b: &str, max: usize) -> Option<usize> {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();

    if a.len().abs_diff(b.len()) > max {
        return None;
    }

    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0; b.len() + 1];

    for (i, a_char) in a.iter().enumerate() {
        current[0] = i + 1;
        let mut row_min = current[0];

        for (j, b_char) in b.iter().enumerate() {
            let substitution = previous[j] + usize::from(a_char != b_char);
            let insertion = current[j] + 1;
            let deletion = previous[j + 1] + 1;
            current[j + 1] = substitution.min(insertion).min(deletion);
            row_min = row_min.min(current[j + 1]);
        }

        if row_min > max {
            return None;
        }
        std::mem::swap(&mut previous, &mut current);
    }

    let distance = previous[b.len()];
    (distance <= max).then_some(distance)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_spacing_and_case() {
        assert_eq!(normalize("  Ice   Cream "), "ice_cream");
        assert_eq!(normalize("Don’t"), "don't");
        assert_eq!(normalize("serendipity!"), "serendipity");
        assert_eq!(normalize("serendipity."), "serendipity");
        assert_eq!(normalize("U.S."), "u.s.");
        assert_eq!(display_form("  Ice   Cream "), "Ice Cream");
    }

    #[test]
    fn parses_a_synset() {
        let line = "11463895 19 n 01 serendipity 0 002 @ 11463746 n 0000 + 01468552 a 0101 | good luck in making unexpected and fortunate discoveries";
        let synset = parse_synset(line).expect("synset should parse");
        assert_eq!(synset.words, vec!["serendipity"]);
        assert_eq!(
            synset.definition,
            "good luck in making unexpected and fortunate discoveries"
        );
        assert!(synset.examples.is_empty());
    }

    #[test]
    fn splits_examples_from_a_gloss() {
        let (definition, examples) = split_gloss(
            "move fast; \"Don't run--you'll be out of breath\"; \"The children ran to the store\"",
        );
        assert_eq!(definition, "move fast");
        assert_eq!(examples.len(), 2);
    }

    #[test]
    fn finds_morphological_base_forms() {
        let candidates = morph_candidates("running");
        assert!(candidates.contains(&"run".to_string()));
        let candidates = morph_candidates("studies");
        assert!(candidates.contains(&"study".to_string()));
        let candidates = morph_candidates("isn't");
        assert!(candidates.contains(&"be".to_string()));
    }

    #[test]
    fn resolves_common_contractions() {
        let dictionary = WordNet::new();
        assert_eq!(dictionary.lookup("don't").matched.as_deref(), Some("do"));
        assert_eq!(dictionary.lookup("isn't").matched.as_deref(), Some("be"));
        assert_eq!(dictionary.lookup("you're").matched.as_deref(), Some("be"));
        assert_eq!(dictionary.lookup("i've").matched.as_deref(), Some("have"));
    }

    #[test]
    fn levenshtein_is_bounded() {
        assert_eq!(levenshtein("kitten", "sitting", 3), Some(3));
        assert_eq!(levenshtein("kitten", "sitting", 2), None);
    }

    #[test]
    fn looks_up_bundled_wordnet_data() {
        let dictionary = WordNet::new();
        let lookup = dictionary.lookup("serendipity");
        assert!(lookup.found);
        assert_eq!(lookup.matched.as_deref(), Some("serendipity"));
        assert_eq!(
            lookup.groups[0].senses[0].definition,
            "good luck in making unexpected and fortunate discoveries"
        );
    }

    #[test]
    fn resolves_inflected_forms() {
        let dictionary = WordNet::new();
        let lookup = dictionary.lookup("ran");
        assert!(lookup.found);
        assert_eq!(lookup.matched.as_deref(), Some("run"));
        assert_eq!(lookup.resolved_from.as_deref(), Some("ran"));
    }

    #[test]
    fn prefers_the_more_common_morphological_base() {
        let dictionary = WordNet::new();
        assert_eq!(dictionary.lookup("uses").matched.as_deref(), Some("use"));
        assert_eq!(dictionary.lookup("does").matched.as_deref(), Some("do"));
    }

    #[test]
    fn combines_multiple_exception_bases() {
        let dictionary = WordNet::new();
        let lookup = dictionary.lookup("leaves");
        assert!(lookup.found);
        assert_eq!(lookup.matched.as_deref(), Some("leaf / leave"));
    }

    #[test]
    fn puts_the_most_frequent_part_of_speech_first() {
        let dictionary = WordNet::new();
        let lookup = dictionary.lookup("run");
        assert_eq!(lookup.groups[0].part_of_speech, "verb");
    }

    #[test]
    fn preserves_input_capitalization_for_exact_matches() {
        let dictionary = WordNet::new();
        assert_eq!(dictionary.lookup("NASA").matched.as_deref(), Some("NASA"));
        assert_eq!(dictionary.lookup("U.S.").matched.as_deref(), Some("U.S."));
    }
}
