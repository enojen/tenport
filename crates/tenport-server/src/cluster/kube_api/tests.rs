use k8s_openapi::api::batch::v1::{Job, JobCondition, JobStatus};
use k8s_openapi::api::core::v1::{
    ContainerState, ContainerStateTerminated, ContainerStateWaiting, ContainerStatus, Pod,
    PodStatus,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, Time};
use k8s_openapi::jiff::Timestamp;

use super::*;
use crate::cluster::{PROJECT_ID_LABEL, TASK_LABEL};

fn meta(name: &str, second: i64) -> ObjectMeta {
    ObjectMeta {
        name: Some(name.to_owned()),
        uid: Some(format!("uid-of-{name}")),
        creation_timestamp: Some(Time(Timestamp::from_second(second).unwrap())),
        labels: Some([("task".to_owned(), "TEN-1".to_owned())].into()),
        ..ObjectMeta::default()
    }
}

fn job_with(status: JobStatus) -> Job {
    Job {
        metadata: meta("task-ten-1-1117-101500", 1_700_000_000),
        status: Some(status),
        ..Job::default()
    }
}

fn container(name: &str, state: ContainerState) -> ContainerStatus {
    ContainerStatus {
        name: name.to_owned(),
        state: Some(state),
        ..ContainerStatus::default()
    }
}

fn pod_with(name: &str, second: i64, phase: &str, statuses: Vec<ContainerStatus>) -> Pod {
    Pod {
        metadata: meta(name, second),
        status: Some(PodStatus {
            phase: Some(phase.to_owned()),
            container_statuses: Some(statuses),
            ..PodStatus::default()
        }),
        ..Pod::default()
    }
}

#[test]
fn a_job_with_no_counts_is_running() {
    let state = job_state(job_with(JobStatus::default())).unwrap();
    assert_eq!(state.phase, JobPhase::Running);
    assert_eq!(state.name.as_str(), "task-ten-1-1117-101500");
    assert_eq!(state.labels.get("task").map(String::as_str), Some("TEN-1"));
}

#[test]
fn a_succeeded_or_failed_count_ends_the_job() {
    let succeeded = job_with(JobStatus {
        succeeded: Some(1),
        ..JobStatus::default()
    });
    let failed = job_with(JobStatus {
        failed: Some(1),
        ..JobStatus::default()
    });
    assert_eq!(job_state(succeeded).unwrap().phase, JobPhase::Succeeded);
    assert_eq!(job_state(failed).unwrap().phase, JobPhase::Failed);
}

#[test]
fn a_job_without_a_name_is_left_out() {
    let mut job = job_with(JobStatus::default());
    job.metadata.name = None;
    assert_eq!(job_state(job), None);
}

#[test]
fn a_pending_pod_tells_why_the_box_waits() {
    let waiting = ContainerState {
        waiting: Some(ContainerStateWaiting {
            reason: Some("CreateContainerConfigError".to_owned()),
            ..ContainerStateWaiting::default()
        }),
        ..ContainerState::default()
    };
    let pod = pod_with("p", 1, "Pending", vec![container("box", waiting)]);
    let state = pod_state(&pod).unwrap();
    assert!(!state.started);
    assert_eq!(
        state.box_reason.as_deref(),
        Some("CreateContainerConfigError")
    );
}

#[test]
fn the_reason_is_the_box_containers_not_the_sidecars() {
    let ended = |reason: &str| ContainerState {
        terminated: Some(ContainerStateTerminated {
            reason: Some(reason.to_owned()),
            ..ContainerStateTerminated::default()
        }),
        ..ContainerState::default()
    };
    let statuses = vec![
        container("processes", ended("Completed")),
        container("box", ended("OOMKilled")),
    ];
    let state = pod_state(&pod_with("p", 1, "Failed", statuses)).unwrap();
    assert!(state.started);
    assert_eq!(state.box_reason.as_deref(), Some("OOMKilled"));
}

#[test]
fn the_newest_pod_is_the_one_read() {
    let pods = vec![
        pod_with("old", 1, "Failed", vec![]),
        pod_with("new", 2, "Running", vec![]),
    ];
    let newest = newest_pod(pods).unwrap();
    assert_eq!(newest.metadata.name.as_deref(), Some("new"));
}

#[test]
fn a_new_secret_is_opaque_and_holds_only_the_written_key() {
    let secret = new_secret(
        &SecretName::new("project-abc123"),
        "ntfy_url",
        "https://n/x",
    );
    assert_eq!(secret.metadata.name.as_deref(), Some("project-abc123"));
    assert_eq!(secret.type_.as_deref(), Some("Opaque"));
    assert_eq!(
        secret.string_data,
        Some([("ntfy_url".to_owned(), "https://n/x".to_owned())].into())
    );
}

#[test]
fn a_job_secret_names_its_job_as_owner_so_it_goes_with_the_job() {
    let job = CreatedJob {
        name: JobName::new("task-ten-301-1118-101500"),
        uid: JobUid::new("0f3c1c9e-5a7b-4e2d-9c1a-7b6d5e4f3a21"),
    };

    let owner = job_owner(&job);

    assert_eq!(owner.api_version, "batch/v1");
    assert_eq!(owner.kind, "Job");
    assert_eq!(owner.name, "task-ten-301-1118-101500");
    assert_eq!(owner.uid, "0f3c1c9e-5a7b-4e2d-9c1a-7b6d5e4f3a21");
}

#[test]
fn a_job_without_a_uid_is_left_out() {
    let mut job = job_with(JobStatus::default());
    job.metadata.uid = None;

    assert_eq!(job_state(job), None);
}

fn condition(kind: &str, reason: &str) -> JobCondition {
    JobCondition {
        type_: kind.to_owned(),
        status: "True".to_owned(),
        reason: Some(reason.to_owned()),
        ..JobCondition::default()
    }
}

#[test]
fn a_job_past_its_deadline_is_failed_with_its_reason_even_without_a_failed_count() {
    let job = job_with(JobStatus {
        conditions: Some(vec![condition("Failed", "DeadlineExceeded")]),
        ..JobStatus::default()
    });
    let state = job_state(job).unwrap();
    assert_eq!(state.phase, JobPhase::Failed);
    assert_eq!(state.failed_reason.as_deref(), Some("DeadlineExceeded"));
}

#[test]
fn a_complete_condition_ends_the_job_as_succeeded() {
    let job = job_with(JobStatus {
        conditions: Some(vec![condition("Complete", "CompletionsReached")]),
        ..JobStatus::default()
    });
    let state = job_state(job).unwrap();
    assert_eq!(state.phase, JobPhase::Succeeded);
    assert_eq!(state.failed_reason, None);
}

#[test]
fn an_ended_box_gives_its_exit_code_and_an_evicted_pod_its_reason() {
    let ended = ContainerState {
        terminated: Some(ContainerStateTerminated {
            reason: Some("Error".to_owned()),
            exit_code: 137,
            ..ContainerStateTerminated::default()
        }),
        ..ContainerState::default()
    };
    let mut pod = pod_with("p", 1, "Failed", vec![container("box", ended)]);
    if let Some(status) = pod.status.as_mut() {
        status.reason = Some("Evicted".to_owned());
    }
    let state = pod_state(&pod).unwrap();
    assert_eq!(state.exit_code, Some(137));
    assert_eq!(state.pod_reason.as_deref(), Some("Evicted"));
    assert_eq!(state.box_reason.as_deref(), Some("Error"));
}

#[test]
fn label_pairs_join_into_one_selector() {
    let selector = label_selector(&[(PROJECT_ID_LABEL, "abc123"), (TASK_LABEL, "TEN-1")]);
    assert_eq!(selector, "project-id=abc123,task=TEN-1");
}
