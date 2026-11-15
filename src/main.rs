mod render;
mod wordnet;

use std::env;
use std::io::{self, BufRead, IsTerminal, Read, Write};
use std::process::ExitCode;

use clap::Parser;
use render::{DEFAULT_EXAMPLE_LIMIT, RenderOptions, render_lookup};
use serde_json::json;
use wordnet::{Lookup, WordNet};

const PROJECT_LICENSE: &str = include_str!("../LICENSE");
const WORDNET_LICENSE: &str = include_str!("../assets/wordnet/LICENSE");
const DEFAULT_LIMIT: usize = 5;

#[derive(Debug, Parser)]
#[command(
    name = "dict-lookup",
    version,
    about = "Look up word meanings offline — no API keys, no network, no rate limits.",
    after_help = "EXAMPLES:\n  dict-lookup serendipity\n  dict-lookup \"ice cream\"\n  dict-lookup --all run\n  dict-lookup --json ephemeral\n  dict-lookup                          interactive mode\n\nDATA:\n  WordNet 3.0 is bundled with the binary, so lookups work offline."
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

    /// Start interactive mode
    #[arg(short, long, conflicts_with = "word")]
    interactive: bool,

    /// Disable colored output
    #[arg(long)]
    no_color: bool,

    /// Print license information and exit
    #[arg(long)]
    license: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    if cli.license {
        print_license();
        return ExitCode::SUCCESS;
    }

    let color = color_enabled(cli.no_color);
    let limit = if cli.all {
        None
    } else {
        Some(cli.limit.unwrap_or(DEFAULT_LIMIT))
    };
    let options = RenderOptions::new(color, limit);
    let dictionary = WordNet::new();
    let stdin_is_tty = io::stdin().is_terminal();

    if cli.interactive || (cli.word.is_none() && stdin_is_tty) {
        return match run_interactive(&dictionary, options, cli.json) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("dict-lookup: {error}");
                ExitCode::from(2)
            }
        };
    }

    let words = match cli.word {
        Some(word) => vec![word],
        None => match read_words_from_stdin() {
            Ok(words) => words,
            Err(error) => {
                eprintln!("dict-lookup: could not read stdin: {error}");
                return ExitCode::from(2);
            }
        },
    };

    if words.is_empty() {
        eprintln!("dict-lookup: no input. Try `dict-lookup --help`.");
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
        println!(
            "{}",
            serde_json::to_string_pretty(&value).expect("JSON serialization failed")
        );
    } else {
        for (index, word) in words.iter().enumerate() {
            if index > 0 {
                println!();
            }
            let lookup = dictionary.lookup(word);
            all_found &= lookup.found;
            print!("{}", render_lookup(&lookup, &options));
        }
    }

    if all_found {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn run_interactive(
    dictionary: &WordNet,
    mut options: RenderOptions,
    json_output: bool,
) -> io::Result<()> {
    if !json_output {
        println!();
        println!("{}", paint(options.color, "1;36", "dict-lookup"));
        println!(
            "{}",
            paint(
                options.color,
                "2",
                "Offline dictionary · WordNet 3.0 · 155,000+ words"
            )
        );
        println!(
            "{}",
            paint(
                options.color,
                "2",
                "Type a word or phrase. :help for commands, :q to quit."
            )
        );
        println!();
    }

    let stdin = io::stdin();
    let mut input = stdin.lock();
    let mut line = String::new();

    loop {
        if !json_output {
            print!("{}", paint(options.color, "1;36", "› "));
            io::stdout().flush()?;
        }

        line.clear();
        if input.read_line(&mut line)? == 0 {
            if !json_output {
                println!();
            }
            break;
        }

        let query = line.trim();
        if query.is_empty() {
            continue;
        }

        match query {
            ":q" | ":quit" | "quit" | "exit" => {
                if !json_output {
                    println!();
                }
                break;
            }
            ":help" => {
                print_interactive_help(options.color);
                continue;
            }
            ":license" => {
                print_license();
                continue;
            }
            ":clear" => {
                print!("\x1b[2J\x1b[H");
                io::stdout().flush()?;
                continue;
            }
            ":all" => {
                options.limit = if options.limit.is_some() {
                    None
                } else {
                    Some(DEFAULT_LIMIT)
                };
                let state = if options.limit.is_some() {
                    format!("showing up to {DEFAULT_LIMIT} senses per part of speech")
                } else {
                    "showing every sense".to_string()
                };
                println!("{}", paint(options.color, "2", &state));
                continue;
            }
            command if command.starts_with(':') => {
                println!(
                    "{}",
                    paint(
                        options.color,
                        "33",
                        "Unknown command. Type :help for available commands."
                    )
                );
                continue;
            }
            _ => {}
        }

        let lookup = dictionary.lookup(query);
        if json_output {
            println!(
                "{}",
                serde_json::to_string(&lookup_json(&limited_lookup(&lookup, options.limit)))
                    .expect("JSON serialization failed")
            );
        } else {
            print!("{}", render_lookup(&lookup, &options));
            println!();
        }
    }

    Ok(())
}

fn print_interactive_help(color: bool) {
    let heading = |text: &str| paint(color, "1", text);
    let command = |text: &str| paint(color, "36", text);

    println!();
    println!("{}", heading("Commands"));
    println!("  {}          look up a word or phrase", command("<word>"));
    println!(
        "  {}           toggle all senses and examples",
        command(":all")
    );
    println!("  {}          show this help", command(":help"));
    println!("  {}       show license information", command(":license"));
    println!("  {}         clear the screen", command(":clear"));
    println!("  {}     quit", command(":q, :quit"));
    println!();
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

fn color_enabled(no_color: bool) -> bool {
    if no_color {
        return false;
    }
    if env::var_os("NO_COLOR").is_some() {
        return false;
    }
    if env::var("CLICOLOR").is_ok_and(|value| value == "0") {
        return false;
    }
    io::stdout().is_terminal()
}

fn paint(enabled: bool, code: &str, text: &str) -> String {
    if enabled {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

fn print_license() {
    println!("dict-lookup\n");
    print!("{PROJECT_LICENSE}");
    if !PROJECT_LICENSE.ends_with('\n') {
        println!();
    }
    println!("\nDictionary data: WordNet 3.0, Princeton University\n");
    print!("{WORDNET_LICENSE}");
    if !WORDNET_LICENSE.ends_with('\n') {
        println!();
    }
}
