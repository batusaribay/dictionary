mod config;
mod render;
mod theme;
mod wordnet;

use std::io::{self, IsTerminal, Read, Write};
use std::process::ExitCode;

use clap::{CommandFactory, Parser};
use render::{DEFAULT_EXAMPLE_LIMIT, RenderOptions, render_lookup};
use serde_json::json;
use theme::{ColorMode, Theme};
use wordnet::{Lookup, WordNet};

const PROJECT_LICENSE: &str = include_str!("../LICENSE");
const WORDNET_LICENSE: &str = include_str!("../assets/wordnet/LICENSE");
const DEFAULT_LIMIT: usize = 5;

#[derive(Debug, Parser)]
#[command(
    name = "dictionary",
    version,
    color = clap::ColorChoice::Never,
    styles = clap::builder::Styles::plain(),
    about = "A Rust CLI for offline dictionary lookups"
)]
struct Cli {
    /// Word or phrase to look up
    #[arg(value_name = "WORD")]
    word: Option<String>,

    /// Show every sense and example instead of the first 5 per part of speech
    #[arg(short = 'a', long, conflicts_with = "limit")]
    all: bool,

    /// Maximum senses per part of speech (default: 5)
    #[arg(
        short = 'n',
        long,
        value_name = "N",
        value_parser = parse_limit
    )]
    limit: Option<usize>,

    /// Print JSON instead of formatted text (respects --limit)
    #[arg(long)]
    json: bool,

    /// Print license information and exit
    #[arg(long)]
    license: bool,

    /// Color output: auto, always, or never
    #[arg(long, value_enum, default_value = "auto", value_name = "WHEN")]
    color: ColorMode,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    if cli.license {
        return match print_license() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => output_error(error),
        };
    }

    if cli.word.is_none() && io::stdin().is_terminal() {
        let mut command = Cli::command();
        let result = command.print_help().and_then(|()| {
            let mut stdout = io::stdout().lock();
            writeln!(stdout)
        });
        return match result {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => output_error(error),
        };
    }

    let config = match config::load() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("dictionary: {error}");
            return ExitCode::from(2);
        }
    };
    let limit = if cli.all {
        None
    } else {
        Some(cli.limit.or(config.limit).unwrap_or(DEFAULT_LIMIT))
    };
    let options = RenderOptions::new(limit, Theme::from_env(cli.color));
    let dictionary = WordNet::new();

    let words = match cli.word {
        Some(word) => vec![word],
        None => match read_words_from_stdin() {
            Ok(words) => words,
            Err(error) => {
                eprintln!("dictionary: could not read stdin: {error}");
                return ExitCode::from(2);
            }
        },
    };

    if words.is_empty() {
        eprintln!("dictionary: no input. Try `dictionary --help`.");
        return ExitCode::from(2);
    }

    let mut all_found = true;

    if cli.json {
        let results: Vec<_> = words
            .iter()
            .map(|word| {
                let lookup = dictionary.lookup(word);
                all_found &= lookup.found;
                lookup_json(&limited_lookup(&lookup, limit))
            })
            .collect();

        let value = if results.len() == 1 {
            results.into_iter().next().unwrap_or_else(|| json!({}))
        } else {
            serde_json::Value::Array(results)
        };
        let mut output = serde_json::to_string_pretty(&value).expect("JSON serialization failed");
        output.push('\n');
        if let Err(error) = write_output(&output) {
            return output_error(error);
        }
    } else {
        for (index, word) in words.iter().enumerate() {
            if index > 0
                && let Err(error) = write_output("\n")
            {
                return output_error(error);
            }
            let lookup = dictionary.lookup(word);
            all_found &= lookup.found;
            if let Err(error) = write_output(&render_lookup(&lookup, &options)) {
                return output_error(error);
            }
        }
    }

    if all_found {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn parse_limit(value: &str) -> Result<usize, String> {
    let limit = value
        .parse::<usize>()
        .map_err(|_| format!("`{value}` is not a number"))?;
    if limit == 0 {
        Err("limit must be at least 1".to_string())
    } else {
        Ok(limit)
    }
}

fn read_words_from_stdin() -> io::Result<Vec<String>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    Ok(input
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(ToOwned::to_owned)
        .collect())
}

fn limited_lookup(lookup: &Lookup, limit: Option<usize>) -> Lookup {
    let mut limited = lookup.clone();
    if let Some(limit) = limit {
        for group in &mut limited.groups {
            group.senses.truncate(limit);
            for sense in &mut group.senses {
                sense.examples.truncate(DEFAULT_EXAMPLE_LIMIT);
            }
        }
    }
    limited
}

fn lookup_json(lookup: &Lookup) -> serde_json::Value {
    serde_json::to_value(lookup).expect("lookup should serialize")
}

fn write_output(output: &str) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(output.as_bytes())?;
    stdout.flush()
}

fn output_error(error: io::Error) -> ExitCode {
    if error.kind() == io::ErrorKind::BrokenPipe {
        ExitCode::SUCCESS
    } else {
        eprintln!("dictionary: could not write stdout: {error}");
        ExitCode::from(2)
    }
}

fn print_license() -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "dictionary\n")?;
    write!(stdout, "{PROJECT_LICENSE}")?;
    if !PROJECT_LICENSE.ends_with('\n') {
        writeln!(stdout)?;
    }
    writeln!(
        stdout,
        "\nDictionary data: WordNet 3.0, Princeton University\n"
    )?;
    write!(stdout, "{WORDNET_LICENSE}")?;
    if !WORDNET_LICENSE.ends_with('\n') {
        writeln!(stdout)?;
    }
    stdout.flush()
}
