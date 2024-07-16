use std::fmt::Display;

use cliclack::{intro as cliclak_intro, log, outro as cliclak_outro, Theme, ThemeState};
use console::{style, Emoji, Term};
use serde::Serialize;

use crate::prompt::CliclackTheme;

const S_BAR: Emoji = Emoji("│", "|");

fn term_write(msg: impl Display) {
    let msg = &format!("{}", msg);
    Term::stderr().write_str(msg).unwrap();
}

pub fn intro() {
    cliclak_intro(style(" ZKsync toolbox ").on_cyan().black()).unwrap();
}

pub fn outro(msg: impl Display) {
    cliclak_outro(msg).unwrap();
}

pub fn info(msg: impl Display) {
    log::info(msg).unwrap();
}

pub fn debug(msg: impl Display) {
    let msg = &format!("{}", msg);
    let log = CliclackTheme.format_log(msg, style("⚙").dim().to_string().as_str());
    Term::stderr().write_str(&log).unwrap();
}

pub fn warn(msg: impl Display) {
    log::warning(msg).unwrap();
}

pub fn error(msg: impl Display) {
    log::error(style(msg).red()).unwrap();
}

pub fn success(msg: impl Display) {
    log::success(msg).unwrap();
}

pub fn step(msg: impl Display) {
    log::step(msg).unwrap();
}

pub fn raw(msg: impl Display) {
    term_write(msg);
}

pub fn note(msg: impl Display, content: impl Display) {
    cliclack::note(msg, content).unwrap();
}

pub fn error_note(msg: &str, content: &str) {
    let symbol = CliclackTheme.state_symbol(&ThemeState::Submit);
    let note = CliclackTheme
        .format_note(msg, content)
        .replace(&symbol, &CliclackTheme.error_symbol());
    term_write(note);
}

pub fn object_to_string(obj: impl Serialize) -> String {
    let json = serde_json::to_value(obj).unwrap();

    fn print_object(key: &str, value: &str, indentation: usize) -> String {
        format!(
            "{:indent$}∙ {} {}\n",
            "",
            style(format!("{key}:")).bold(),
            style(value),
            indent = indentation
        )
    }

    fn print_header(header: &str, indentation: usize) -> String {
        format!(
            "{:indent$}∙ {}\n",
            "",
            style(format!("{header}:")).bold(),
            indent = indentation
        )
    }

    fn traverse_json(json: &serde_json::Value, indent: usize) -> String {
        let mut values = String::new();

        if let serde_json::Value::Object(obj) = json {
            for (key, value) in obj {
                match value {
                    serde_json::Value::Object(_) => {
                        values.push_str(&print_header(key, indent));
                        values.push_str(&traverse_json(value, indent + 2));
                    }
                    _ => values.push_str(&print_object(key, &value.to_string(), indent)),
                }
            }
        }

        values
    }

    traverse_json(&json, 2)
}

pub fn new_empty_line() {
    term_write("\n");
}

pub fn new_line() {
    term_write(format!(
        "{}\n",
        CliclackTheme.bar_color(&ThemeState::Submit).apply_to(S_BAR)
    ))
}
