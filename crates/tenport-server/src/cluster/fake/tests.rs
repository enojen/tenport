use futures::AsyncReadExt;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

use super::*;

fn job(name: &str, task: &str) -> Job {
    Job {
        metadata: ObjectMeta {
            name: Some(name.to_owned()),
            labels: Some(
                [
                    ("app".to_owned(), "tenport".to_owned()),
                    ("task".to_owned(), task.to_owned()),
                ]
                .into(),
            ),
            ..ObjectMeta::default()
        },
        ..Job::default()
    }
}

#[tokio::test]
async fn a_created_job_is_listed_by_its_labels() {
    let cluster = FakeCluster::default();
    cluster.create_job(job("a", "TEN-1")).await.unwrap();
    cluster.create_job(job("b", "TEN-2")).await.unwrap();
    let listed = cluster.list_jobs("app=tenport,task=TEN-2").await.unwrap();
    let names: Vec<&str> = listed.iter().map(|state| state.name.as_str()).collect();
    assert_eq!(names, ["b"]);
    assert_eq!(listed[0].phase, JobPhase::Running);
}

#[tokio::test]
async fn a_second_job_of_the_same_name_is_refused() {
    let cluster = FakeCluster::default();
    cluster.create_job(job("a", "TEN-1")).await.unwrap();
    let again = cluster.create_job(job("a", "TEN-1")).await;
    assert!(matches!(again, Err(ClusterError::Exists(name)) if name.as_str() == "a"));
}

#[tokio::test]
async fn a_deleted_job_is_gone_and_a_second_delete_finds_nothing() {
    let cluster = FakeCluster::default();
    let name = cluster.create_job(job("a", "TEN-1")).await.unwrap().name;
    assert!(cluster.delete_job(&name).await.unwrap());
    assert!(!cluster.delete_job(&name).await.unwrap());
    assert!(cluster.jobs().is_empty());
}

#[tokio::test]
async fn the_log_of_a_pod_is_read() {
    let cluster = FakeCluster::default();
    let name = cluster.create_job(job("a", "TEN-1")).await.unwrap().name;
    assert_eq!(cluster.find_pod(&name).await.unwrap(), None);
    let pod = PodState {
        name: PodName::new("a-x"),
        started: true,
        reason: None,
        exit_code: None,
        pod_reason: None,
    };
    cluster.set_pod(&name, pod.clone(), "one\ntwo\n");
    cluster.set_phase(&name, JobPhase::Succeeded);
    assert_eq!(cluster.find_pod(&name).await.unwrap(), Some(pod.clone()));
    let mut text = String::new();
    let mut reader = cluster
        .read_log(&pod.name, LogRead::default())
        .await
        .unwrap();
    reader.read_to_string(&mut text).await.unwrap();
    assert_eq!(text, "one\ntwo\n");
    assert_eq!(
        cluster.list_jobs("task=TEN-1").await.unwrap()[0].phase,
        JobPhase::Succeeded
    );
}
