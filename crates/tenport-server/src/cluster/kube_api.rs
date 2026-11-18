use futures::FutureExt;
use futures::future::BoxFuture;
use k8s_openapi::api::batch::v1::Job;
use std::collections::BTreeMap;

use k8s_openapi::api::core::v1::{ContainerStatus, Pod, Secret};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, OwnerReference};
use kube::api::{DeleteParams, ListParams, LogParams, Patch, PatchParams, PostParams};
use kube::{Api, Client};
use serde_json::{Value, json};

use super::model::{
    ClusterError, CreatedJob, JobName, JobPhase, JobState, JobUid, LogReader, PodName, PodState,
    SecretName,
};
use super::{BOX_CONTAINER, Cluster, MANAGER};

const CONFLICT: u16 = 409;
const NOT_FOUND: u16 = 404;
const PENDING: &str = "Pending";
const COMPLETE: &str = "Complete";
const FAILED: &str = "Failed";
const TRUE: &str = "True";
const OPAQUE: &str = "Opaque";

pub struct KubeCluster {
    jobs: Api<Job>,
    pods: Api<Pod>,
    secrets: Api<Secret>,
}

impl KubeCluster {
    pub async fn connect(namespace: &str) -> Result<Self, ClusterError> {
        let client = Client::try_default().await.map_err(ClusterError::Kube)?;
        Ok(Self::new(client, namespace))
    }

    pub fn new(client: Client, namespace: &str) -> Self {
        Self {
            jobs: Api::namespaced(client.clone(), namespace),
            pods: Api::namespaced(client.clone(), namespace),
            secrets: Api::namespaced(client, namespace),
        }
    }

    async fn create_secret(
        &self,
        secret: &SecretName,
        key: &str,
        value: &str,
    ) -> Result<(), ClusterError> {
        match self
            .secrets
            .create(&as_tenport(), &new_secret(secret, None, key, value))
            .await
        {
            Ok(_) => Ok(()),
            Err(error) if status_code(&error) == Some(CONFLICT) => {
                self.patch_secret(secret, string_data(key, value)).await
            }
            Err(error) => Err(ClusterError::Kube(error)),
        }
    }

    async fn patch_secret(&self, secret: &SecretName, patch: Value) -> Result<(), ClusterError> {
        let params = PatchParams {
            field_manager: Some(MANAGER.to_owned()),
            ..PatchParams::default()
        };
        self.secrets
            .patch(secret.as_str(), &params, &Patch::Merge(patch))
            .await
            .map(|_| ())
            .map_err(ClusterError::Kube)
    }
}

impl Cluster for KubeCluster {
    fn create_job(&self, job: Job) -> BoxFuture<'_, Result<CreatedJob, ClusterError>> {
        async move {
            let name = JobName::new(job.metadata.name.clone().ok_or(ClusterError::NoName)?);
            match self.jobs.create(&as_tenport(), &job).await {
                Ok(created) => created_job(name, created.metadata.uid),
                Err(error) if status_code(&error) == Some(CONFLICT) => {
                    Err(ClusterError::Exists(name))
                }
                Err(error) => Err(ClusterError::Kube(error)),
            }
        }
        .boxed()
    }

    fn list_jobs<'a>(
        &'a self,
        labels: &'a [(&'a str, &'a str)],
    ) -> BoxFuture<'a, Result<Vec<JobState>, ClusterError>> {
        async move {
            let params = ListParams::default().labels(&label_selector(labels));
            let list = self.jobs.list(&params).await.map_err(ClusterError::Kube)?;
            Ok(list.items.into_iter().filter_map(job_state).collect())
        }
        .boxed()
    }

    fn find_job<'a>(
        &'a self,
        job: &'a JobName,
    ) -> BoxFuture<'a, Result<Option<JobState>, ClusterError>> {
        async move {
            let found = self
                .jobs
                .get_opt(job.as_str())
                .await
                .map_err(ClusterError::Kube)?;
            Ok(found.and_then(job_state))
        }
        .boxed()
    }

    fn find_pod<'a>(
        &'a self,
        job: &'a JobName,
    ) -> BoxFuture<'a, Result<Option<PodState>, ClusterError>> {
        async move {
            let params = ListParams::default().labels(&format!("job-name={}", job.as_str()));
            let list = self.pods.list(&params).await.map_err(ClusterError::Kube)?;
            Ok(newest_pod(list.items).as_ref().and_then(pod_state))
        }
        .boxed()
    }

    fn read_log<'a>(
        &'a self,
        pod: &'a PodName,
        follow: bool,
    ) -> BoxFuture<'a, Result<LogReader, ClusterError>> {
        async move {
            let params = LogParams {
                container: Some(BOX_CONTAINER.to_owned()),
                follow,
                ..LogParams::default()
            };
            match self.pods.log_stream(pod.as_str(), &params).await {
                Ok(reader) => Ok(Box::pin(reader) as LogReader),
                Err(error) if status_code(&error) == Some(NOT_FOUND) => {
                    Err(ClusterError::NoPod(pod.clone()))
                }
                Err(error) => Err(ClusterError::Kube(error)),
            }
        }
        .boxed()
    }

    fn delete_job<'a>(&'a self, job: &'a JobName) -> BoxFuture<'a, Result<(), ClusterError>> {
        async move {
            match self
                .jobs
                .delete(job.as_str(), &DeleteParams::background())
                .await
            {
                Ok(_) => Ok(()),
                Err(error) if status_code(&error) == Some(NOT_FOUND) => Ok(()),
                Err(error) => Err(ClusterError::Kube(error)),
            }
        }
        .boxed()
    }

    fn write_secret_key<'a>(
        &'a self,
        secret: &'a SecretName,
        key: &'a str,
        value: &'a str,
    ) -> BoxFuture<'a, Result<(), ClusterError>> {
        async move {
            match self.patch_secret(secret, string_data(key, value)).await {
                Err(ClusterError::Kube(error)) if status_code(&error) == Some(NOT_FOUND) => {
                    self.create_secret(secret, key, value).await
                }
                other => other,
            }
        }
        .boxed()
    }

    fn delete_secret_key<'a>(
        &'a self,
        secret: &'a SecretName,
        key: &'a str,
    ) -> BoxFuture<'a, Result<bool, ClusterError>> {
        async move {
            let found = self
                .secrets
                .get_opt(secret.as_str())
                .await
                .map_err(ClusterError::Kube)?;
            let has_key = found
                .and_then(|secret| secret.data)
                .is_some_and(|data| data.contains_key(key));
            if has_key {
                self.patch_secret(secret, json!({ "data": { key: null } }))
                    .await?;
            }
            Ok(has_key)
        }
        .boxed()
    }

    fn create_job_secret<'a>(
        &'a self,
        secret: &'a SecretName,
        owner: &'a CreatedJob,
        key: &'a str,
        value: &'a str,
    ) -> BoxFuture<'a, Result<(), ClusterError>> {
        async move {
            self.secrets
                .create(&as_tenport(), &new_secret(secret, Some(owner), key, value))
                .await
                .map(|_| ())
                .map_err(ClusterError::Kube)
        }
        .boxed()
    }
}

fn label_selector(labels: &[(&str, &str)]) -> String {
    labels
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn created_job(name: JobName, uid: Option<String>) -> Result<CreatedJob, ClusterError> {
    match uid {
        Some(uid) => Ok(CreatedJob {
            name,
            uid: JobUid::new(uid),
        }),
        None => Err(ClusterError::NoUid(name)),
    }
}

fn string_data(key: &str, value: &str) -> Value {
    json!({ "stringData": { key: value } })
}

fn as_tenport() -> PostParams {
    PostParams {
        field_manager: Some(MANAGER.to_owned()),
        ..PostParams::default()
    }
}

fn new_secret(secret: &SecretName, owner: Option<&CreatedJob>, key: &str, value: &str) -> Secret {
    Secret {
        metadata: ObjectMeta {
            name: Some(secret.as_str().to_owned()),
            owner_references: owner.map(|job| vec![job_owner(job)]),
            ..ObjectMeta::default()
        },
        type_: Some(OPAQUE.to_owned()),
        string_data: Some(BTreeMap::from([(key.to_owned(), value.to_owned())])),
        ..Secret::default()
    }
}

fn job_owner(job: &CreatedJob) -> OwnerReference {
    OwnerReference {
        api_version: "batch/v1".to_owned(),
        kind: "Job".to_owned(),
        name: job.name.as_str().to_owned(),
        uid: job.uid.as_str().to_owned(),
        ..OwnerReference::default()
    }
}

fn status_code(error: &kube::Error) -> Option<u16> {
    match error {
        kube::Error::Api(status) => Some(status.code),
        _ => None,
    }
}

fn job_state(job: Job) -> Option<JobState> {
    let phase = job_phase(&job);
    let failed_reason = job_failed_reason(&job);
    let metadata = job.metadata;
    Some(JobState {
        name: JobName::new(metadata.name?),
        uid: JobUid::new(metadata.uid?),
        labels: metadata.labels.unwrap_or_default(),
        phase,
        failed_reason,
    })
}

fn job_failed_reason(job: &Job) -> Option<String> {
    job.status
        .as_ref()?
        .conditions
        .as_deref()?
        .iter()
        .find(|condition| condition.type_ == FAILED && condition.status == TRUE)?
        .reason
        .clone()
}

fn has_condition(job: &Job, kind: &str) -> bool {
    job.status
        .as_ref()
        .and_then(|status| status.conditions.as_deref())
        .unwrap_or_default()
        .iter()
        .any(|condition| condition.type_ == kind && condition.status == TRUE)
}

fn job_phase(job: &Job) -> JobPhase {
    let Some(status) = job.status.as_ref() else {
        return JobPhase::Running;
    };
    if status.succeeded.unwrap_or(0) > 0 || has_condition(job, COMPLETE) {
        return JobPhase::Succeeded;
    }
    if status.failed.unwrap_or(0) > 0 || has_condition(job, FAILED) {
        return JobPhase::Failed;
    }
    JobPhase::Running
}

fn newest_pod(pods: Vec<Pod>) -> Option<Pod> {
    pods.into_iter()
        .max_by_key(|pod| pod.metadata.creation_timestamp.as_ref().map(|time| time.0))
}

fn pod_state(pod: &Pod) -> Option<PodState> {
    let name = PodName::new(pod.metadata.name.clone()?);
    let status = pod.status.as_ref();
    let started = status
        .and_then(|status| status.phase.as_deref())
        .is_some_and(|phase| phase != PENDING);
    let box_status = status
        .and_then(|status| status.container_statuses.as_deref())
        .unwrap_or_default()
        .iter()
        .find(|container| container.name == BOX_CONTAINER);
    Some(PodState {
        name,
        started,
        box_reason: box_status.and_then(box_reason),
        exit_code: box_status.and_then(box_exit_code),
        pod_reason: status.and_then(|status| status.reason.clone()),
    })
}

fn box_exit_code(status: &ContainerStatus) -> Option<i32> {
    let state = status.state.as_ref()?;
    state
        .terminated
        .as_ref()
        .map(|terminated| terminated.exit_code)
}

fn box_reason(status: &ContainerStatus) -> Option<String> {
    let state = status.state.as_ref()?;
    let terminated = state.terminated.as_ref().and_then(|t| t.reason.clone());
    terminated.or_else(|| state.waiting.as_ref().and_then(|w| w.reason.clone()))
}

#[cfg(test)]
mod tests;
