use std::path::Path;

use k8s_openapi::api::core::v1::{Container, PodSpec};

use super::*;

fn template() -> RunTemplate {
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    RunTemplate {
        dir: tests.join("run-template"),
        services_dir: tests.join("run-services"),
    }
}

fn fields(class: &str) -> TemplateFields {
    TemplateFields {
        name: "ten-301".to_owned(),
        task: "TEN-301".to_owned(),
        run: "1117-101500".to_owned(),
        project: "Tenport".to_owned(),
        project_id: "abc123".to_owned(),
        repo: "https://github.com/enojen/tenport.git".to_owned(),
        base: "main".to_owned(),
        git_name: "Enes".to_owned(),
        git_email: "1+enojen@users.noreply.github.com".to_owned(),
        image: "box:1".to_owned(),
        command: "build".to_owned(),
        cmd: "tenbox build TEN-301".to_owned(),
        env: BTreeMap::from([("TEST_PG_URL".to_owned(), "from-project".to_owned())]),
        class: ClassName::parse(class).unwrap(),
        archive_get: String::new(),
        archive_put: String::new(),
        setup: "bun install".to_owned(),
    }
}

fn service(name: &str) -> ServiceName {
    ServiceName::parse(name).unwrap()
}

fn pod(job: &Job) -> &PodSpec {
    job.spec.as_ref().unwrap().template.spec.as_ref().unwrap()
}

fn box_container(job: &Job) -> &Container {
    pod(job)
        .containers
        .iter()
        .find(|c| c.name == "box")
        .unwrap()
}

fn env_value<'a>(container: &'a Container, name: &str) -> Option<&'a str> {
    let env = container.env.as_ref()?;
    env.iter().find(|e| e.name == name)?.value.as_deref()
}

#[tokio::test]
async fn the_fields_and_the_class_fill_the_job() {
    let job = template().job(&fields("build"), &[]).await.unwrap();
    assert_eq!(
        job.metadata.name.as_deref(),
        Some("task-ten-301-1117-101500")
    );
    let container = box_container(&job);
    assert_eq!(container.image.as_deref(), Some("box:1"));
    assert_eq!(
        env_value(container, "TENBOX_CMD"),
        Some("tenbox build TEN-301")
    );
    assert_eq!(
        env_value(container, "TENBOX_REPO"),
        Some("https://github.com/enojen/tenport.git")
    );
    let limits = container
        .resources
        .as_ref()
        .unwrap()
        .limits
        .as_ref()
        .unwrap();
    assert_eq!(limits["memory"].0, "2Gi");
}

#[tokio::test]
async fn tenport_manages_the_job_and_the_tenbox_server_does_not_see_it() {
    let job = template().job(&fields("build"), &[]).await.unwrap();
    let pod_labels = job
        .spec
        .as_ref()
        .unwrap()
        .template
        .metadata
        .as_ref()
        .unwrap()
        .labels
        .as_ref()
        .unwrap();
    for labels in [job.metadata.labels.as_ref().unwrap(), pod_labels] {
        assert_eq!(
            labels.get(MANAGED_BY_LABEL).map(String::as_str),
            Some(MANAGER)
        );
        assert_eq!(labels.get(APP_LABEL), None);
    }
    assert_eq!(pod_labels.get("box").map(String::as_str), Some("true"));
}

fn box_env_of(job: &Job) -> BTreeMap<String, String> {
    serde_json::from_str(env_value(box_container(job), "TENBOX_ENV").unwrap()).unwrap()
}

#[tokio::test]
async fn a_service_adds_its_sidecar_volume_and_env() {
    let mut without_project_env = fields("build");
    without_project_env.env.clear();
    let job = template()
        .job(&without_project_env, &[service("postgres")])
        .await
        .unwrap();
    let sidecars: Vec<&str> = pod(&job)
        .init_containers
        .as_ref()
        .unwrap()
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert_eq!(sidecars, ["processes", "postgres"]);
    let volumes: Vec<&str> = pod(&job)
        .volumes
        .as_ref()
        .unwrap()
        .iter()
        .map(|v| v.name.as_str())
        .collect();
    assert!(volumes.contains(&"pgdata"));
    assert_eq!(
        box_env_of(&job)["TEST_PG_URL"],
        "postgres://test:test@127.0.0.1:5432"
    );
}

#[tokio::test]
async fn the_projects_env_wins_over_a_services_env() {
    let job = template()
        .job(&fields("build"), &[service("postgres")])
        .await
        .unwrap();
    assert_eq!(box_env_of(&job)["TEST_PG_URL"], "from-project");
}

#[tokio::test]
async fn an_unknown_class_or_service_is_named() {
    assert!(matches!(
        template().job(&fields("huge"), &[]).await,
        Err(TemplateError::NoClass { class, .. }) if class.as_str() == "huge"
    ));
    assert!(matches!(
        template().job(&fields("build"), &[service("redis")]).await,
        Err(TemplateError::NoService { service }) if service.as_str() == "redis"
    ));
}
