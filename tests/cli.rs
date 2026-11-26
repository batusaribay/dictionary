use std::io::Write;
use std::process::{Command, Output, Stdio};

fn dictionary(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dictionary"))
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("failed to run dictionary")
}

fn dictionary_with_stdin(args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_dictionary"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn dictionary");
    child
        .stdin
        .take()
        .expect("missing stdin")
        .write_all(input.as_bytes())
        .expect("failed to write stdin");
    child
        .wait_with_output()
        .expect("failed to wait for dictionary")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn json_limits_output_and_ignores_color() {
    let out = dictionary(&["--json", "--color", "always", "--limit", "1", "run"]);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(!text.contains('\u{1b}'));
    let lookup: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    assert_eq!(lookup["found"], true);
    for group in lookup["groups"].as_array().expect("groups") {
        assert_eq!(group["senses"].as_array().expect("senses").len(), 1);
        for sense in group["senses"].as_array().expect("senses") {
            assert!(
                sense["examples"]
                    .as_array()
                    .is_none_or(|examples| examples.len() <= 2)
            );
        }
    }
}

#[test]
fn reads_multiple_lookups_from_stdin() {
    let out = dictionary_with_stdin(&["--json", "--limit", "1"], "serendipity\n\nice cream\n");
    assert!(out.status.success());
    let lookups: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    let lookups = lookups.as_array().expect("lookup array");
    assert_eq!(lookups.len(), 2);
    assert_eq!(lookups[0]["query"], "serendipity");
    assert_eq!(lookups[1]["matched"], "ice cream");
}

#[test]
fn missing_queries_cannot_inject_terminal_controls() {
    let out = dictionary(&["--color", "never", "missing\u{1b}[31m\nword"]);
    assert_eq!(out.status.code(), Some(1));
    let text = stdout(&out);
    assert!(!text.contains('\u{1b}'));
    assert!(text.contains("No entry for “missing [31m word”."));
}

#[test]
fn help_is_plain_and_successful() {
    let out = dictionary(&["--help"]);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("Usage: dictionary"));
    assert!(!text.contains('\u{1b}'));
}

#[test]
fn license_includes_wordnet_terms() {
    let out = dictionary(&["--license"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("WordNet Release 3.0"));
}

#[test]
fn rejects_a_zero_limit() {
    let out = dictionary(&["--limit", "0", "run"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("limit must be at least 1"));
}
