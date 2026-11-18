use serde_json::Value;

const OBJECT_START: &str = "{";
const OBJECT_END: &str = "}";
const OOM_KILL: &str = "oom_kill ";

pub fn command_output(log: &str) -> Option<Value> {
    let lines: Vec<&str> = log.lines().collect();
    let end = lines.iter().rposition(|line| *line == OBJECT_END)?;
    let start = lines[..end]
        .iter()
        .rposition(|line| *line == OBJECT_START)?;
    let value: Value = serde_json::from_str(&lines[start..=end].join("\n")).ok()?;
    value.is_object().then_some(value)
}

pub fn oom_kill(log: &str) -> Option<u64> {
    log.lines()
        .rev()
        .find_map(|line| line.rsplit_once(OOM_KILL))
        .and_then(|(_, count)| count.trim().parse().ok())
}

#[cfg(test)]
mod tests;
