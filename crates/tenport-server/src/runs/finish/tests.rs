use serde_json::json;

use super::*;

const LOG: &str = "tenbox-boot: ready
{
  \"ok\": true,
  \"inner\": {
    \"a\": 1
  }
}
tenbox-boot: memory peak 731 MiB, cpu 26 s, pids peak 81, oom_kill 0
tenbox-boot: home saved to the task archive: 8.5M
";

#[test]
fn the_output_is_the_last_top_level_json_object() {
    let log = format!("{{\n  \"first\": true\n}}\n{LOG}");
    assert_eq!(
        command_output(&log),
        Some(json!({ "ok": true, "inner": { "a": 1 } }))
    );
}

#[test]
fn a_log_without_a_json_object_has_no_output() {
    assert_eq!(command_output("tenbox-boot: setup failed\n"), None);
}

#[test]
fn a_broken_json_object_is_no_output() {
    assert_eq!(command_output("{\n  \"ok\": tru\n}\n"), None);
}

#[test]
fn the_oom_kill_count_of_the_boot_line_is_read_even_when_a_line_follows_it() {
    assert_eq!(oom_kill(&LOG.replace("oom_kill 0", "oom_kill 3")), Some(3));
    assert_eq!(oom_kill(LOG), Some(0));
}

#[test]
fn a_log_without_the_boot_line_has_no_oom_kill_count() {
    assert_eq!(oom_kill("tenbox-boot: setup failed\n"), None);
}
