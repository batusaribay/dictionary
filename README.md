# dictionary

A Rust CLI dictionary for offline lookups

![dictionary in a terminal](preview.png)

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
7. Output wraps to the terminal width and stays plain text. JSON is available
   for scripts. Nothing is fetched, cached, or sent anywhere.

## Usage

### Commands

| Command | Action |
| --- | --- |
| `dictionary <word>` | Look up a word or quoted phrase. |
| `dictionary "ice cream"` | Look up a multi-word phrase. |
| `dictionary --all run` | Show every sense and example. |
| `dictionary --limit 2 run` | Show at most two senses per part of speech. |
| `dictionary --json ephemeral` | Print machine-readable JSON. |
| `dictionary --license` | Print the MIT and WordNet licenses. |

By default, the text output shows five senses per part of speech and two
examples per sense. `--all` removes both limits. `--json` respects `--limit`.

When stdin is not a terminal, each non-empty line is treated as one lookup, so
word lists can be piped in.

### Examples

```bash
dictionary serendipity
dictionary "ice cream"
dictionary ran
dictionary "don't"
dictionary --all run
dictionary --json --all ephemeral
printf 'hello\nworld\n' | dictionary
```

If no entry is found, close spellings are suggested and the exit status is `1`.
A successful lookup exits `0`.

## Install

**Requirements:** Rust 1.88+ and `git`.

```bash
cargo install --git https://github.com/batusaribay/dictionary.git --locked
```

## Uninstall

```bash
cargo uninstall dictionary
```

## License

The `dictionary` code is licensed under the [MIT License](LICENSE). The bundled
WordNet 3.0 database has its own license, included at
[`assets/wordnet/LICENSE`](assets/wordnet/LICENSE).
