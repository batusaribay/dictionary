# dict-lookup

A fast, offline dictionary for your terminal. Definitions, parts of speech,
examples, and synonyms are all bundled — no accounts, API keys, network calls,
or rate limits.

![dict-lookup in a terminal](preview.png)

## What it is

A single Rust binary that looks up English words and phrases using Princeton's
WordNet 3.0. It covers about 155,000 words across nouns, verbs, adjectives, and
adverbs, and works the same on a plane, over SSH, or with the network unplugged.

## Why this exists

A dictionary is reference data. It should not need a key, a quota, a login, or
a round trip to a server before it can tell you what a word means. The free
dictionary APIs that exist are useful, but they rate-limit, change, and fail
when the network does. A terminal lookup should be immediate, scriptable, and
private.

So the database ships inside the executable. `dict-lookup` has no backend and
nothing to configure.

## How it works

1. The WordNet 3.0 database files are gzip-compressed and committed under
   `assets/wordnet/`.
2. `include_bytes!` embeds those files directly into the compiled binary, so
   the executable is self-contained and has no runtime data directory.
3. On the first lookup, the index files are decompressed once in memory and
   parsed into hash maps from lemma to synset offsets. Synset data stays in its
   original file and is read on demand by byte offset.
4. A query is trimmed, lowercased, and normalized to WordNet's key format, with
   spaces converted to underscores so phrases such as `ice cream` work.
5. Exact matches are tried first. If there is none, WordNet's exception lists
   and a small set of English suffix rules handle plurals, past tenses, and
   contractions (`ran` → `run`, `studies` → `study`, `don't` → `do`).
6. Each synset is parsed into a definition, examples, and synonyms. Senses are
   grouped by part of speech, with the most common part of speech first.
7. Output wraps to the terminal width, uses colour only on a terminal, and
   supports JSON for scripts. Nothing is fetched, cached, or sent anywhere.

The binary never opens a socket. There is no hidden download step: the
dictionary is in the binary, not in `~/.cache` or a sidecar directory.

## Usage

### Commands

| Command | Action |
| --- | --- |
| `dict-lookup <word>` | Look up a word or quoted phrase. |
| `dict-lookup` | Start the interactive prompt. |
| `dict-lookup "ice cream"` | Look up a multi-word phrase. |
| `dict-lookup --all run` | Show every sense and example. |
| `dict-lookup --limit 2 run` | Show at most two senses per part of speech. |
| `dict-lookup --json ephemeral` | Print machine-readable JSON. |
| `dict-lookup --no-color word` | Disable colour. |
| `dict-lookup --license` | Print the MIT and WordNet licenses. |

By default, the text output shows five senses per part of speech and two
examples per sense. `--all` removes both limits. `--json` respects `--limit`.

### Interactive mode

Run `dict-lookup` with no word and a terminal on stdin. Type a word or phrase
and press Enter.

| Command | Action |
| --- | --- |
| `<word>` | Look up a word or phrase. |
| `:all` | Toggle all senses and examples. |
| `:help` | Show available commands. |
| `:clear` | Clear the screen. |
| `:license` | Print license information. |
| `:q`, `:quit` | Exit. |

### Examples

```bash
dict-lookup serendipity
dict-lookup "ice cream"
dict-lookup ran
dict-lookup "don't"
dict-lookup --all run
dict-lookup --json --all ephemeral
printf 'hello\nworld\n' | dict-lookup
```

If no entry is found, close spellings are suggested and the exit status is `1`.
A successful lookup exits `0`.

## Installation

**Requirements:** Rust 1.88+ and `git`.

```bash
cargo install --git https://github.com/batusaribay/dict-lookup.git --locked
```

Or from a local checkout:

```bash
cargo install --path . --locked
```

The installed command is available from any directory because the dictionary
data is embedded in the binary. Shell completion is not required; every lookup
is a normal process.

## Uninstall

```bash
cargo uninstall dict-lookup
```

There are no config, cache, or data directories to remove.

## Data

Dictionary data is [WordNet 3.0](https://wordnet.princeton.edu/), Copyright
2006 by Princeton University. Its license is in
[`assets/wordnet/LICENSE`](assets/wordnet/LICENSE) and is also available with
`dict-lookup --license`.

## Future Ideas

- Optional import of a newer Wiktionary extract for words added after 2006
- Pronunciation playback from a local audio pack
- Shell completions for bash, zsh, and fish
- Additional languages and dictionaries

## Known Issues

- WordNet 3.0 is from 2006. Very new words, slang, and recent proper nouns may
  be missing.
- The database is English-only.
- Some WordNet definitions are terse or dated compared with modern learner's
  dictionaries.
- There is no audio pronunciation; the data is text-only.
- Proper nouns are stored lowercase in WordNet. `dict-lookup` keeps the
  capitalisation you typed for exact matches.

## License

The `dict-lookup` code is licensed under the [MIT License](LICENSE). The bundled
WordNet 3.0 database has its own license, included at
[`assets/wordnet/LICENSE`](assets/wordnet/LICENSE).
