use crate::wordnet::Lookup;

pub const DEFAULT_EXAMPLE_LIMIT: usize = 2;

pub struct RenderOptions {
    pub color: bool,
    pub limit: Option<usize>,
    pub width: usize,
}

impl RenderOptions {
    pub fn new(color: bool, limit: Option<usize>) -> Self {
        Self {
            color,
            limit,
            width: terminal_width().clamp(48, 100),
        }
    }
}

#[derive(Clone, Copy)]
struct Style {
    enabled: bool,
}

impl Style {
    fn paint(self, code: &str, text: &str) -> String {
        if self.enabled {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    fn bold(self, text: &str) -> String {
        self.paint("1", text)
    }

    fn bold_underline(self, text: &str) -> String {
        self.paint("1;4", text)
    }

    fn dim(self, text: &str) -> String {
        self.paint("2", text)
    }

    fn italic_dim(self, text: &str) -> String {
        self.paint("2;3", text)
    }

    fn cyan(self, text: &str) -> String {
        self.paint("36", text)
    }

    fn green(self, text: &str) -> String {
        self.paint("32", text)
    }

    fn yellow(self, text: &str) -> String {
        self.paint("33", text)
    }
}

fn part_of_speech_style(part_of_speech: &str) -> &'static str {
    match part_of_speech {
        "noun" => "1;36",
        "verb" => "1;32",
        "adjective" => "1;33",
        "adverb" => "1;35",
        _ => "1",
    }
}

pub fn render_lookup(lookup: &Lookup, options: &RenderOptions) -> String {
    let style = Style {
        enabled: options.color,
    };

    if !lookup.found {
        return render_not_found(lookup, style);
    }

    let head = lookup.matched.as_deref().unwrap_or(&lookup.query);
    let mut output = String::new();
    output.push_str(&style.bold_underline(head));

    if let Some(from) = &lookup.resolved_from {
        output.push_str("  ");
        output.push_str(&style.dim(&format!("(from {from})")));
    }
    output.push('\n');

    let mut first_group = true;
    for group in &lookup.groups {
        if !first_group {
            output.push('\n');
        }
        first_group = false;

        output.push_str("  ");
        output.push_str(&style.paint(
            part_of_speech_style(group.part_of_speech),
            group.part_of_speech,
        ));
        output.push('\n');

        let shown = match options.limit {
            Some(limit) => group.senses.len().min(limit.max(1)),
            None => group.senses.len(),
        };

        for (index, sense) in group.senses.iter().take(shown).enumerate() {
            render_sense(
                &mut output,
                index + 1,
                &sense.definition,
                &sense.examples,
                &sense.synonyms,
                options,
                style,
            );
        }

        if shown < group.senses.len() {
            let omitted = group.senses.len() - shown;
            output.push_str("      ");
            output.push_str(&style.dim(&format!(
                "… {omitted} more {} sense{} · use --all",
                group.part_of_speech,
                if omitted == 1 { "" } else { "s" }
            )));
            output.push('\n');
        }
    }

    output
}

fn render_sense(
    output: &mut String,
    number: usize,
    definition: &str,
    examples: &[String],
    synonyms: &[String],
    options: &RenderOptions,
    style: Style,
) {
    let number = format!("{number}.");
    let hanging = 4 + number.len() + 1;
    let available = options.width.saturating_sub(hanging).max(20);

    for (line_index, line) in textwrap::wrap(definition, available).iter().enumerate() {
        if line_index == 0 {
            output.push_str("    ");
            output.push_str(&style.dim(&number));
            output.push(' ');
        } else {
            output.push_str(&" ".repeat(hanging));
        }
        output.push_str(line);
        output.push('\n');
    }

    let example_limit = if options.limit.is_none() {
        usize::MAX
    } else {
        DEFAULT_EXAMPLE_LIMIT
    };
    for example in examples.iter().take(example_limit) {
        push_wrapped(
            output,
            &format!("“{example}”"),
            6,
            options.width,
            |line| style.italic_dim(line),
        );
    }

    if !synonyms.is_empty() {
        let label = "synonyms: ";
        let joined = synonyms.join(", ");
        let indent = 6;
        let available = options.width.saturating_sub(indent + label.len()).max(20);
        for (line_index, line) in textwrap::wrap(&joined, available).iter().enumerate() {
            if line_index == 0 {
                output.push_str(&" ".repeat(indent));
                output.push_str(&style.dim(label));
            } else {
                output.push_str(&" ".repeat(indent + label.len()));
            }
            output.push_str(&style.green(line));
            output.push('\n');
        }
    }
}

fn push_wrapped<F>(output: &mut String, text: &str, indent: usize, width: usize, style: F)
where
    F: Fn(&str) -> String,
{
    let available = width.saturating_sub(indent).max(20);
    for line in textwrap::wrap(text, available) {
        output.push_str(&" ".repeat(indent));
        output.push_str(&style(&line));
        output.push('\n');
    }
}

fn render_not_found(lookup: &Lookup, style: Style) -> String {
    let query = lookup.query.trim();
    let query = if query.is_empty() { "(nothing)" } else { query };

    let mut output = String::new();
    output.push_str(&style.yellow("No entry for "));
    output.push_str(&style.bold(&format!("“{query}”")));
    output.push_str(".\n");

    if !lookup.suggestions.is_empty() {
        output.push('\n');
        output.push_str("  ");
        output.push_str(&style.dim("Did you mean: "));
        output.push_str(&style.cyan(&lookup.suggestions.join(", ")));
        output.push('\n');
    }

    output
}

pub fn terminal_width() -> usize {
    std::env::var("COLUMNS")
        .ok()
        .and_then(|columns| columns.parse::<usize>().ok())
        .filter(|columns| *columns >= 40)
        .unwrap_or(90)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wordnet::{PosGroup, Sense};

    fn sample_lookup() -> Lookup {
        Lookup {
            query: "test".to_string(),
            found: true,
            matched: Some("test".to_string()),
            resolved_from: None,
            groups: vec![PosGroup {
                part_of_speech: "noun",
                senses: vec![
                    Sense {
                        definition: "a procedure intended to establish the quality, performance, or reliability of something".to_string(),
                        examples: vec!["the test was successful".to_string()],
                        synonyms: vec!["trial".to_string(), "check".to_string()],
                    },
                    Sense {
                        definition: "a short written or spoken examination".to_string(),
                        examples: Vec::new(),
                        synonyms: Vec::new(),
                    },
                ],
            }],
            suggestions: Vec::new(),
        }
    }

    #[test]
    fn renders_plain_text_definitions() {
        let options = RenderOptions {
            color: false,
            limit: Some(1),
            width: 60,
        };
        let rendered = render_lookup(&sample_lookup(), &options);
        assert!(rendered.starts_with("test\n  noun\n    1. a procedure"));
        assert!(rendered.contains("“the test was successful”"));
        assert!(rendered.contains("synonyms: trial, check"));
        assert!(rendered.contains("1 more noun sense"));
        assert!(!rendered.contains("\x1b["));
    }

    #[test]
    fn renders_not_found_with_suggestions() {
        let lookup = Lookup {
            query: "serendipitty".to_string(),
            found: false,
            matched: None,
            resolved_from: None,
            groups: Vec::new(),
            suggestions: vec!["serendipity".to_string()],
        };
        let options = RenderOptions {
            color: false,
            limit: None,
            width: 80,
        };
        let rendered = render_lookup(&lookup, &options);
        assert!(rendered.contains("No entry for “serendipitty”."));
        assert!(rendered.contains("Did you mean: serendipity"));
    }

    #[test]
    fn uses_theme_palette_ansi_colors_when_enabled() {
        let options = RenderOptions {
            color: true,
            limit: Some(1),
            width: 60,
        };
        let rendered = render_lookup(&sample_lookup(), &options);
        assert!(rendered.contains("\x1b[1;4mtest\x1b[0m"));
        assert!(rendered.contains("\x1b[1;36mnoun\x1b[0m"));
        assert!(rendered.contains("\x1b[2;3m“the test was successful”\x1b[0m"));
        assert!(rendered.contains("\x1b[32mtrial, check\x1b[0m"));
    }
}
