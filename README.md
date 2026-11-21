# dictionary

A Rust CLI for offline dictionary lookups

![dictionary in a terminal](preview.png)

## Why this exists

Looking up a word should not need a network, an account, or a browser. This
binary embeds a dictionary and answers instantly, so it works on a plane, in a
script, and in an offline shell.

## How it works

The WordNet 3.0 files are gzip-compressed and committed under `assets/wordnet/`.
`include_bytes!` embeds them in the binary, so the executable is self-contained
and needs no runtime data directory. On the first lookup, the index files are
decompressed once in memory and parsed into hash maps from lemma to synset
offsets, while synset data stays in its original file and is read on demand by
byte offset.

A query is trimmed, lowercased, and normalized to WordNet's key format, with
spaces converted to underscores so phrases such as `ice cream` work. Exact
matches come first; WordNet exception lists and a small set of English suffix
rules otherwise handle plurals, past tenses, and contractions. Each synset
becomes a definition, examples, and synonyms, grouped by part of speech with the
most common first.

Output wraps to the terminal width and stays plain text. JSON is available for
scripts. Nothing is fetched, cached, or sent anywhere.

## Usage

```text
dictionary [OPTIONS] [WORD]

Arguments:
  [WORD]  Word or phrase to look up

Options:
  -a, --all           Show every sense and example instead of the first 5 per part of speech
  -n, --limit <N>     Maximum senses per part of speech (default: 5)
      --json          Print JSON instead of formatted text (respects --limit)
      --license       Print license information and exit
      --color <WHEN>  Color output: auto, always, or never [default: auto]
  -h, --help          Print help
  -V, --version       Print version
```

Typical calls:

```bash
dictionary serendipity
dictionary "ice cream"
dictionary --all run
dictionary --json ephemeral
printf 'hello\nworld\n' | dictionary
```

## Notes

Text output shows five senses per part of speech and two examples per sense.
`--all` removes both limits, and `--json` respects `--limit`. When stdin is not
a terminal, each non-empty line is one lookup, so word lists can be piped in.

Color uses the terminal palette and ANSI attributes only. Body text inherits
the terminal foreground, while the headword, part of speech, numbers, examples,
and synonyms use semantic roles. `--color never`, `NO_COLOR`, `CLICOLOR=0`,
`TERM=dumb`, and non-terminal output all disable it. JSON is never colored.

A missing entry prints close spellings and exits `1`. A successful lookup exits
`0`. The code is MIT; WordNet 3.0 has its own license, included at
`assets/wordnet/LICENSE` and printed by `--license`.

## Install

**Requirements:** Rust 1.88+ and `git`.

```bash
cargo install --git https://github.com/batusaribay/dictionary.git --locked
```

## License

This project is licensed under the [MIT License](LICENSE).
