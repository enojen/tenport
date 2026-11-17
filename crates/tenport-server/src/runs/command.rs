use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::tasks::TaskId;

const TOOL: &str = "tenbox";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(rename_all = "lowercase")]
pub enum CommandName {
    Start,
    Build,
    Close,
    Approve,
    Status,
    Plan,
    Pr,
    Check,
    Decide,
    Answer,
}

impl CommandName {
    const ALL: [Self; 10] = [
        Self::Start,
        Self::Build,
        Self::Close,
        Self::Approve,
        Self::Status,
        Self::Plan,
        Self::Pr,
        Self::Check,
        Self::Decide,
        Self::Answer,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Build => "build",
            Self::Close => "close",
            Self::Approve => "approve",
            Self::Status => "status",
            Self::Plan => "plan",
            Self::Pr => "pr",
            Self::Check => "check",
            Self::Decide => "decide",
            Self::Answer => "answer",
        }
    }

    pub fn runs_tests(self) -> bool {
        self.shape().tests
    }

    fn find(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|name| name.as_str() == word)
    }

    fn shape(self) -> Shape {
        match self {
            Self::Start => Shape {
                value_flags: &["--title"],
                bool_flags: &["--force"],
                ..NO_FLAGS
            },
            Self::Build => Shape {
                tests: true,
                ..NO_FLAGS
            },
            Self::Close => Shape {
                bool_flags: &["--force"],
                ..NO_FLAGS
            },
            Self::Approve | Self::Status => NO_FLAGS,
            Self::Plan => Shape {
                before_task: &["show"],
                ..NO_FLAGS
            },
            Self::Pr => Shape {
                bool_flags: &["--ready", "--force", "--wait"],
                ..NO_FLAGS
            },
            Self::Check => Shape {
                value_flags: &["--group", "--tier"],
                tests: true,
                ..NO_FLAGS
            },
            Self::Decide => Shape {
                value_flags: &["--json"],
                bool_flags: &["--no-build"],
                tests: true,
                ..NO_FLAGS
            },
            Self::Answer => Shape {
                positionals: 1,
                tests: true,
                ..NO_FLAGS
            },
        }
    }
}

#[derive(Clone, Copy)]
struct Shape {
    before_task: &'static [&'static str],
    value_flags: &'static [&'static str],
    bool_flags: &'static [&'static str],
    positionals: usize,
    tests: bool,
}

const NO_FLAGS: Shape = Shape {
    before_task: &[],
    value_flags: &[],
    bool_flags: &[],
    positionals: 0,
    tests: false,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    name: CommandName,
    words: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "the command is tenbox start|build|close|approve|decide|answer|check|pr|status|plan show, then the task id, then only the flags of that command"
)]
pub struct CommandError;

impl Command {
    pub fn parse(words: Vec<String>, task: &TaskId) -> Result<Self, CommandError> {
        let name = checked_name(&words, task).ok_or(CommandError)?;
        Ok(Self { name, words })
    }

    pub fn name(&self) -> CommandName {
        self.name
    }

    pub fn shell_line(&self) -> String {
        self.words
            .iter()
            .map(|word| shell_word(word))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn checked_name(words: &[String], task: &TaskId) -> Option<CommandName> {
    let [tool, word, rest @ ..] = words else {
        return None;
    };
    let name = CommandName::find(word).filter(|_| tool == TOOL)?;
    let after_name = after_words(rest, name.shape().before_task)?;
    let [id, args @ ..] = after_name else {
        return None;
    };
    let is_task = TaskId::parse(id).is_ok_and(|id| id == *task);
    (is_task && args_fit(name.shape(), args)).then_some(name)
}

fn after_words<'a>(words: &'a [String], expected: &[&str]) -> Option<&'a [String]> {
    let head = words.get(..expected.len())?;
    head.iter()
        .zip(expected)
        .all(|(word, wanted)| word == wanted)
        .then(|| &words[expected.len()..])
}

fn args_fit(shape: Shape, args: &[String]) -> bool {
    let Some(mut flags) = args.get(shape.positionals..) else {
        return false;
    };
    while let [flag, rest @ ..] = flags {
        let width = flag_width(shape, flag, !rest.is_empty());
        if width == 0 {
            return false;
        }
        flags = &flags[width..];
    }
    true
}

fn flag_width(shape: Shape, flag: &str, has_value: bool) -> usize {
    if shape.bool_flags.contains(&flag) {
        return 1;
    }
    if shape.value_flags.contains(&flag) && has_value {
        return 2;
    }
    0
}

fn is_shell_safe(word: &str) -> bool {
    !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_@%+=:,./-".contains(c))
}

fn shell_word(word: &str) -> String {
    if is_shell_safe(word) {
        return word.to_owned();
    }
    format!("'{}'", word.replace('\'', r"'\''"))
}

pub fn command_rules() -> Value {
    let rules = CommandName::ALL
        .into_iter()
        .map(|name| (name.as_str().to_owned(), shape_rule(name.shape())));
    Value::Object(Map::from_iter(rules))
}

fn shape_rule(shape: Shape) -> Value {
    json!({
        "before_task": shape.before_task,
        "positionals": shape.positionals,
        "value_flags": shape.value_flags,
        "bool_flags": shape.bool_flags,
    })
}

#[cfg(test)]
mod tests;
