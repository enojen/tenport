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
    let end = run_end(EndPhase::Succeeded, None, &log);
    assert_eq!(end.output, Some(json!({ "ok": true, "inner": { "a": 1 } })));
}

#[test]
fn a_log_without_a_json_object_has_no_output() {
    let end = run_end(EndPhase::Failed, None, "tenbox-boot: setup failed\n");
    assert_eq!(end.output, None);
}

#[test]
fn a_broken_json_object_is_no_output() {
    let end = run_end(EndPhase::Failed, None, "{\n  \"ok\": tru\n}\n");
    assert_eq!(end.output, None);
}

#[test]
fn the_oom_kill_count_of_the_boot_line_counts_even_when_a_line_follows_it() {
    let log = LOG.replace("oom_kill 0", "oom_kill 3");
    assert!(run_end(EndPhase::Failed, None, &log).oom);
    assert!(!run_end(EndPhase::Succeeded, None, LOG).oom);
}

#[test]
fn a_box_container_killed_for_memory_is_oom() {
    let end = run_end(EndPhase::Failed, Some("OOMKilled".to_owned()), "");
    assert!(end.oom);
}
