use std::time::Duration;

use k8s_openapi::api::batch::v1::Job;
use reqwest::StatusCode;
use serde_json::{Value, json};
use tenport_server::accounts::{Role, Token};
use tenport_server::cluster::{
    Cluster, FakeCluster, JobName, JobPhase, PodName, PodState, SecretName,
};
use tenport_server::run_credentials::RunCredentialKey;

use crate::server::TestServer;

const CREATE_TIME: Duration = Duration::from_millis(300);

pub async fn project_of(server: &TestServer, token: &Token) -> String {
    project_named(server, token, "Tenport").await
}

async fn project_named(server: &TestServer, token: &Token, name: &str) -> String {
    let me: Value = reqwest::Client::new()
        .get(server.url("/me"))
        .bearer_auth(token.expose())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let project: Value = reqwest::Client::new()
        .post(server.url("/projects"))
        .bearer_auth(token.expose())
        .json(
            &json!({ "organization": me["organizations"][0]["id"], "name": name, "repo": "enojen/tenport" }),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    project["id"].as_str().unwrap().to_owned()
}

pub fn build(project: &str) -> Value {
    json!({
        "project": project,
        "task": "TEN-301",
        "command": ["tenbox", "build", "TEN-301"],
        "base": "main",
        "class": "build",
        "env": { "NODE_ENV": "test" },
        "setup": "bun install",
        "services": ["postgres"],
        "author": { "name": "Enes", "email": "1+enojen@users.noreply.github.com" }
    })
}

pub async fn post_run(server: &TestServer, token: &Token, body: &Value) -> reqwest::Response {
    reqwest::Client::new()
        .post(server.url("/runs"))
        .bearer_auth(token.expose())
        .json(body)
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn a_member_starts_a_run_and_tenport_creates_its_job() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let project = project_of(&server, &token).await;
    let response = post_run(&server, &token, &build(&project)).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let run: Value = response.json().await.unwrap();
    assert_eq!(run["project"], project.as_str());
    assert_eq!(run["task"], "TEN-301");
    let jobs = server.cluster.jobs();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].metadata.name.as_deref(), run["name"].as_str());
}

fn box_env(job: &Job, name: &str) -> String {
    let spec = job.spec.as_ref().unwrap().template.spec.as_ref().unwrap();
    let container = spec.containers.iter().find(|c| c.name == "box").unwrap();
    let env = container.env.as_ref().unwrap();
    let var = env.iter().find(|var| var.name == name).unwrap();
    var.value.clone().unwrap_or_default()
}

#[tokio::test]
async fn the_job_of_a_run_gets_a_get_and_a_put_url_for_the_task_archive() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let project = project_of(&server, &token).await;

    let response = post_run(&server, &token, &build(&project)).await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let job = &server.cluster.jobs()[0];
    let get = box_env(job, "TENBOX_ARCHIVE_GET");
    let put = box_env(job, "TENBOX_ARCHIVE_PUT");
    let object = format!("{}/tasks/{project}/TEN-301.tar.zst?", server.bucket.url);
    assert!(get.starts_with(&object), "{get}");
    assert!(put.starts_with(&object), "{put}");
    assert!(get.contains("X-Amz-Expires=86400"), "{get}");
    assert_ne!(get, put);
}

#[tokio::test]
async fn a_command_that_is_not_a_tenbox_command_of_the_task_is_a_bad_request() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let project = project_of(&server, &token).await;
    let mut body = build(&project);
    body["command"] = json!(["bash", "-c", "curl evil"]);
    let response = post_run(&server, &token, &body).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(server.cluster.jobs().is_empty());
}

#[tokio::test]
async fn a_second_run_while_the_first_runs_is_machine_busy() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let project = project_of(&server, &token).await;
    let first = post_run(&server, &token, &build(&project)).await;
    assert_eq!(first.status(), StatusCode::CREATED);
    let second = post_run(&server, &token, &build(&project)).await;
    assert_eq!(second.status(), StatusCode::CONFLICT);
    let problem: Value = second.json().await.unwrap();
    assert_eq!(problem["code"], "machine-busy");
}

#[tokio::test]
async fn an_unknown_class_is_a_bad_request() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let project = project_of(&server, &token).await;
    let mut body = build(&project);
    body["class"] = json!("huge");
    let response = post_run(&server, &token, &body).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_user_of_another_organization_is_forbidden() {
    let server = TestServer::start().await;
    let owner = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let stranger = server.add_user("Other", "someone", Role::Admin).await;
    let project = project_of(&server, &owner).await;
    let response = post_run(&server, &stranger, &build(&project)).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(server.cluster.jobs().is_empty());
}

pub async fn started_run(server: &TestServer, token: &Token) -> (String, String) {
    let project = project_of(server, token).await;
    let run: Value = post_run(server, token, &build(&project))
        .await
        .json()
        .await
        .unwrap();
    (project, run["name"].as_str().unwrap().to_owned())
}

pub async fn get(server: &TestServer, token: &Token, path: &str) -> reqwest::Response {
    reqwest::Client::new()
        .get(server.url(path))
        .bearer_auth(token.expose())
        .send()
        .await
        .unwrap()
}

async fn delete(server: &TestServer, token: &Token, path: &str) -> reqwest::Response {
    reqwest::Client::new()
        .delete(server.url(path))
        .bearer_auth(token.expose())
        .send()
        .await
        .unwrap()
}

fn started_pod(log: &str, run: &str) -> (JobName, PodState, String) {
    let state = PodState {
        name: PodName::new(format!("{run}-abcde")),
        started: true,
        reason: None,
        exit_code: None,
        pod_reason: None,
    };
    (JobName::new(run), state, log.to_owned())
}

#[tokio::test]
async fn the_runs_of_a_task_list_the_started_run_as_running() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let (project, name) = started_run(&server, &token).await;
    let response = get(
        &server,
        &token,
        &format!("/runs?project={project}&task=TEN-301"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let runs: Value = response.json().await.unwrap();
    assert_eq!(runs.as_array().unwrap().len(), 1);
    assert_eq!(runs[0]["name"], name.as_str());
    assert_eq!(runs[0]["phase"], "Running");
}

#[tokio::test]
async fn the_runs_of_another_task_are_empty() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let (project, _) = started_run(&server, &token).await;
    let runs: Value = get(
        &server,
        &token,
        &format!("/runs?project={project}&task=TEN-302"),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(runs, json!([]));
}

#[tokio::test]
async fn a_run_gives_its_fields_and_the_phase_and_reason_of_its_job() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let (project, name) = started_run(&server, &token).await;
    let job = JobName::new(&name);
    server.cluster.set_phase(&job, JobPhase::Failed);
    let pod = PodState {
        name: PodName::new(format!("{name}-abcde")),
        started: true,
        reason: Some("OOMKilled".to_owned()),
        exit_code: None,
        pod_reason: None,
    };
    server.cluster.set_pod(&job, pod, "");
    let response = get(&server, &token, &format!("/runs/{name}")).await;
    assert_eq!(response.status(), StatusCode::OK);
    let run: Value = response.json().await.unwrap();
    assert_eq!(run["name"], name.as_str());
    assert_eq!(run["project"], project.as_str());
    assert_eq!(run["task"], "TEN-301");
    assert_eq!(run["command"], "build");
    assert_eq!(run["class"], "build");
    assert!(run["created"].as_str().unwrap().ends_with('Z'));
    assert_eq!(run["phase"], "Failed");
    assert_eq!(run["reason"], "OOMKilled");
}

#[tokio::test]
async fn a_run_whose_job_went_before_its_end_was_seen_is_unknown() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let (_, name) = started_run(&server, &token).await;
    server
        .cluster
        .delete_job(&JobName::new(&name))
        .await
        .unwrap();
    assert!(server.cluster.jobs().is_empty());
    let run = run_of(&server, &token, &name).await;
    assert_eq!(run["phase"], "Unknown");
    assert_eq!(run["reason"], Value::Null);
}

#[tokio::test]
async fn an_unknown_run_is_not_found() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    for path in ["/runs/task-x-1", "/runs/task-x-1/log"] {
        assert_unknown_run(get(&server, &token, path).await).await;
    }
    assert_unknown_run(delete(&server, &token, "/runs/task-x-1").await).await;
}

async fn assert_unknown_run(response: reqwest::Response) {
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let problem: Value = response.json().await.unwrap();
    let detail = problem["detail"].as_str().unwrap();
    assert!(detail.contains("the run task-x-1"), "{detail}");
}

#[tokio::test]
async fn a_user_of_another_organization_cannot_see_or_stop_a_run() {
    let server = TestServer::start().await;
    let owner = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let stranger = server.add_user("Other", "someone", Role::Admin).await;
    let (project, name) = started_run(&server, &owner).await;
    let paths = [
        format!("/runs?project={project}&task=TEN-301"),
        format!("/runs/{name}"),
        format!("/runs/{name}/log"),
    ];
    for path in &paths {
        let response = get(&server, &stranger, path).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{path}");
    }
    let response = delete(&server, &stranger, &format!("/runs/{name}")).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(server.cluster.jobs().len(), 1);
}

#[tokio::test]
async fn the_log_of_a_run_is_the_plain_text_of_its_box_container() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let (_, name) = started_run(&server, &token).await;
    let (job, pod, log) = started_pod("cloning\nbuilding\n", &name);
    server.cluster.set_pod(&job, pod, &log);
    server.cluster.set_phase(&job, JobPhase::Succeeded);
    let response = get(&server, &token, &format!("/runs/{name}/log?follow=true")).await;
    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response.headers()["content-type"].to_str().unwrap();
    assert!(content_type.starts_with("text/plain"), "{content_type}");
    assert_eq!(response.text().await.unwrap(), "cloning\nbuilding\n");
}

#[tokio::test]
async fn the_log_of_a_run_whose_box_has_not_started_is_a_conflict_with_the_reason() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let (_, name) = started_run(&server, &token).await;
    let pod = PodState {
        name: PodName::new(format!("{name}-abcde")),
        started: false,
        reason: Some("CreateContainerConfigError".to_owned()),
        exit_code: None,
        pod_reason: None,
    };
    server.cluster.set_pod(&JobName::new(&name), pod, "");
    let response = get(&server, &token, &format!("/runs/{name}/log")).await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(response.headers()["retry-after"], "5");
    let problem: Value = response.json().await.unwrap();
    let detail = problem["detail"].as_str().unwrap();
    assert!(detail.contains("CreateContainerConfigError"), "{detail}");
    assert_eq!(problem["code"], "box-not-started");
}

#[tokio::test]
async fn the_log_of_a_stopped_run_is_a_conflict_that_says_the_job_is_gone() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let (_, name) = started_run(&server, &token).await;
    delete(&server, &token, &format!("/runs/{name}")).await;
    let response = get(&server, &token, &format!("/runs/{name}/log")).await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert!(response.headers().get("retry-after").is_none());
    let problem: Value = response.json().await.unwrap();
    let detail = problem["detail"].as_str().unwrap();
    assert!(detail.contains("its Job is gone"), "{detail}");
    assert_eq!(problem["code"], "job-gone");
}

#[tokio::test]
async fn a_run_is_recorded_when_the_client_leaves_before_the_answer() {
    let server = TestServer::start_on(FakeCluster::with_create_time(CREATE_TIME)).await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let project = project_of(&server, &token).await;
    let left = reqwest::Client::builder()
        .timeout(CREATE_TIME / 3)
        .build()
        .unwrap()
        .post(server.url("/runs"))
        .bearer_auth(token.expose())
        .json(&build(&project))
        .send()
        .await;
    assert!(left.unwrap_err().is_timeout());
    tokio::time::sleep(CREATE_TIME * 2).await;
    let runs: Value = get(
        &server,
        &token,
        &format!("/runs?project={project}&task=TEN-301"),
    )
    .await
    .json()
    .await
    .unwrap();
    assert_eq!(server.cluster.jobs().len(), 1);
    assert_eq!(runs.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn a_project_name_that_is_not_a_label_value_reaches_the_job_as_one() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let project = project_named(&server, &token, " Ödev  takibi: v2 ").await;

    let response = post_run(&server, &token, &build(&project)).await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let job = &server.cluster.jobs()[0];
    let labels = job.metadata.labels.as_ref().unwrap();
    assert_eq!(labels["project"], "dev-takibi-v2");
}

#[tokio::test]
async fn a_project_name_with_no_label_character_reaches_the_job_as_its_id() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let project = project_named(&server, &token, "ğüş").await;

    let response = post_run(&server, &token, &build(&project)).await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let job = &server.cluster.jobs()[0];
    let labels = job.metadata.labels.as_ref().unwrap();
    assert_eq!(labels["project"], project.as_str());
}

#[tokio::test]
async fn the_job_of_a_run_gets_its_own_placeholder_in_a_secret_that_goes_with_the_job() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let project = project_of(&server, &token).await;

    let response = post_run(&server, &token, &build(&project)).await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let run: Value = response.json().await.unwrap();
    let job = JobName::new(run["name"].as_str().unwrap());
    let secret = SecretName::new(format!("{}-placeholder", job.as_str()));
    let uid = FakeCluster::uid_of(&job);
    let placeholder = RunCredentialKey::new(crate::server::RUN_CREDENTIAL_KEY).placeholder(&uid);
    assert!(placeholder.starts_with("tbxph_"), "{placeholder}");
    assert_eq!(
        server.cluster.secret(&secret),
        Some([("placeholder".to_owned(), placeholder)].into())
    );
    let owner = server.cluster.secret_owner(&secret).unwrap();
    assert_eq!(owner.name, job);
    assert_eq!(owner.uid, uid);
}

const BUILD_LOG: &str = "tenbox-boot: ready at 2026-11-18T14:01:52Z
{
  \"ok\": true,
  \"task\": \"TEN-301\"
}
tenbox-boot: command ended with exit code 0
tenbox-boot: memory peak 731 MiB, cpu 26 s, pids peak 81, oom_kill 0
tenbox-boot: home saved to the task archive: 8.5M
";

struct BoxEnd<'a> {
    reason: &'a str,
    exit_code: i32,
    pod_reason: Option<&'a str>,
    log: &'a str,
}

const COMPLETED: BoxEnd = BoxEnd {
    reason: "Completed",
    exit_code: 0,
    pod_reason: None,
    log: BUILD_LOG,
};

async fn ended_run(server: &TestServer, token: &Token, phase: JobPhase, end: BoxEnd<'_>) -> String {
    let (_, name) = started_run(server, token).await;
    let job = JobName::new(&name);
    server.cluster.set_phase(&job, phase);
    let pod = PodState {
        name: PodName::new(format!("{name}-abcde")),
        started: true,
        reason: Some(end.reason.to_owned()),
        exit_code: Some(end.exit_code),
        pod_reason: end.pod_reason.map(str::to_owned),
    };
    server.cluster.set_pod(&job, pod, end.log);
    name
}

async fn run_of(server: &TestServer, token: &Token, name: &str) -> Value {
    get(server, token, &format!("/runs/{name}"))
        .await
        .json()
        .await
        .unwrap()
}

#[tokio::test]
async fn an_ended_run_gives_the_output_of_its_command() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let name = ended_run(&server, &token, JobPhase::Succeeded, COMPLETED).await;
    let run = run_of(&server, &token, &name).await;
    assert_eq!(run["phase"], "Succeeded");
    assert_eq!(run["reason"], "Completed");
    assert_eq!(run["exit_code"], 0);
    assert_eq!(run["oom_kill"], 0);
    assert_eq!(run["output"], json!({ "ok": true, "task": "TEN-301" }));
}

#[tokio::test]
async fn an_ended_run_keeps_its_phase_reason_and_output_after_its_job_is_gone() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let name = ended_run(&server, &token, JobPhase::Failed, COMPLETED).await;
    get(&server, &token, &format!("/runs/{name}")).await;
    delete(&server, &token, &format!("/runs/{name}")).await;
    assert!(server.cluster.jobs().is_empty());
    let run = run_of(&server, &token, &name).await;
    assert_eq!(run["phase"], "Failed");
    assert_eq!(run["reason"], "Completed");
    assert_eq!(run["output"], json!({ "ok": true, "task": "TEN-301" }));
}

#[tokio::test]
async fn a_run_counts_the_processes_killed_for_memory_and_still_succeeds() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let log = BUILD_LOG.replace("oom_kill 0", "oom_kill 2");
    let end = BoxEnd {
        log: &log,
        ..COMPLETED
    };
    let name = ended_run(&server, &token, JobPhase::Succeeded, end).await;
    let run = run_of(&server, &token, &name).await;
    assert_eq!(run["phase"], "Succeeded");
    assert_eq!(run["oom_kill"], 2);
}

#[tokio::test]
async fn a_box_killed_for_memory_ends_oom_killed_with_exit_code_137() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let end = BoxEnd {
        reason: "OOMKilled",
        exit_code: 137,
        pod_reason: None,
        log: "tenbox-boot: ready\n",
    };
    let name = ended_run(&server, &token, JobPhase::Failed, end).await;
    let run = run_of(&server, &token, &name).await;
    assert_eq!(run["phase"], "Failed");
    assert_eq!(run["reason"], "OOMKilled");
    assert_eq!(run["exit_code"], 137);
    assert_eq!(run["oom_kill"], Value::Null);
    assert_eq!(run["output"], Value::Null);
}

#[tokio::test]
async fn an_evicted_pod_ends_evicted_not_error() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let end = BoxEnd {
        reason: "Error",
        exit_code: 137,
        pod_reason: Some("Evicted"),
        log: "",
    };
    let name = ended_run(&server, &token, JobPhase::Failed, end).await;
    server
        .cluster
        .fail(&JobName::new(&name), "BackoffLimitExceeded");
    assert_eq!(run_of(&server, &token, &name).await["reason"], "Evicted");
}

#[tokio::test]
async fn a_job_past_its_deadline_ends_deadline_exceeded() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let end = BoxEnd {
        reason: "Error",
        exit_code: 137,
        pod_reason: None,
        log: "",
    };
    let name = ended_run(&server, &token, JobPhase::Failed, end).await;
    server
        .cluster
        .fail(&JobName::new(&name), "DeadlineExceeded");
    let run = run_of(&server, &token, &name).await;
    assert_eq!(run["reason"], "DeadlineExceeded");
    assert_eq!(run["exit_code"], 137);
}

#[tokio::test]
async fn a_stopped_run_ends_failed_and_stopped() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let (_, name) = started_run(&server, &token).await;
    let response = delete(&server, &token, &format!("/runs/{name}")).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let run = run_of(&server, &token, &name).await;
    assert_eq!(run["phase"], "Failed");
    assert_eq!(run["reason"], "Stopped");
    assert_eq!(run["exit_code"], Value::Null);
}

#[tokio::test]
async fn stopping_an_ended_run_keeps_its_own_end() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let name = ended_run(&server, &token, JobPhase::Succeeded, COMPLETED).await;
    delete(&server, &token, &format!("/runs/{name}")).await;
    let run = run_of(&server, &token, &name).await;
    assert_eq!(run["phase"], "Succeeded");
    assert_eq!(run["reason"], "Completed");
}

#[tokio::test]
async fn a_running_run_has_no_output_yet() {
    let server = TestServer::start().await;
    let token = server.add_user("Wegobi", "enojen", Role::Admin).await;
    let (_, name) = started_run(&server, &token).await;
    let run = run_of(&server, &token, &name).await;
    assert_eq!(run["phase"], "Running");
    assert_eq!(run["output"], Value::Null);
    assert_eq!(run["exit_code"], Value::Null);
    assert_eq!(run["oom_kill"], Value::Null);
}
