use serde_json::Value;

use super::model::{EndPhase, RunEnd};

const OBJECT_START: &str = "{";
const OBJECT_END: &str = "}";
const OOM_KILL: &str = "oom_kill ";
const OOM_KILLED_REASON: &str = "OOMKilled";

pub fn run_end(phase: EndPhase, reason: Option<String>, log: &str) -> RunEnd {
    let oom = reason.as_deref() == Some(OOM_KILLED_REASON) || oom_kills(log) > 0;
    RunEnd {
        phase,
        reason,
        oom,
        output: command_output(log),
    }
}

pub fn unknown_end() -> RunEnd {
    RunEnd {
        phase: EndPhase::Unknown,
        reason: None,
        oom: false,
        output: None,
    }
}

fn command_output(log: &str) -> Option<Value> {
    let lines: Vec<&str> = log.lines().collect();
    let end = lines.iter().rposition(|line| *line == OBJECT_END)?;
    let start = lines[..end]
        .iter()
        .rposition(|line| *line == OBJECT_START)?;
    let value: Value = serde_json::from_str(&lines[start..=end].join("\n")).ok()?;
    value.is_object().then_some(value)
}

fn oom_kills(log: &str) -> u64 {
    log.lines()
        .rev()
        .find_map(|line| line.rsplit_once(OOM_KILL))
        .and_then(|(_, count)| count.trim().parse().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
