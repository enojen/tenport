use futures::FutureExt;
use futures::future::BoxFuture;
use k8s_openapi::api::batch::v1::Job;
use std::collections::BTreeMap;

use k8s_openapi::api::core::v1::{ContainerStatus, Pod, Secret};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, OwnerReference};
use kube::api::{DeleteParams, ListParams, LogParams, Patch, PatchParams, PostParams};
use kube::{Api, Client};
use serde_json::{Value, json};

use super::Cluster;
use super::model::{
    ClusterError, CreatedJob, JobName, JobPhase, JobState, JobUid, LogReader, PodName, PodState,
    SecretName,
};

const BOX_CONTAINER: &str = "box";
const CONFLICT: u16 = 409;
const NOT_FOUND: u16 = 404;
const PENDING: &str = "Pending";
const COMPLETE: &str = "Complete";
const FAILED: &str = "Failed";
const TRUE: &str = "True";
const FIELD_MANAGER: &str = "tenport";
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

    async fn create(&self, job: Job) -> Result<CreatedJob, ClusterError> {
        let name = JobName::new(job.metadata.name.clone().ok_or(ClusterError::NoName)?);
        match self.jobs.create(&PostParams::default(), &job).await {
            Ok(created) => match created.metadata.uid {
                Some(uid) => Ok(CreatedJob {
                    name,
                    uid: JobUid::new(uid),
                }),
                None => Err(ClusterError::NoUid(name)),
            },
            Err(error) if status_code(&error) == Some(CONFLICT) => Err(ClusterError::Exists(name)),
            Err(error) => Err(ClusterError::Kube(error)),
        }
    }

    async fn list(&self, selector: &str) -> Result<Vec<JobState>, ClusterError> {
        let params = ListParams::default().labels(selector);
        let list = self.jobs.list(&params).await.map_err(ClusterError::Kube)?;
        Ok(list.items.iter().filter_map(job_state).collect())
    }

    async fn find(&self, job: &JobName) -> Result<Option<PodState>, ClusterError> {
        let params = ListParams::default().labels(&format!("job-name={}", job.as_str()));
        let list = self.pods.list(&params).await.map_err(ClusterError::Kube)?;
        Ok(newest_pod(list.items).as_ref().and_then(pod_state))
    }

    async fn open_log(&self, pod: &PodName, follow: bool) -> Result<LogReader, ClusterError> {
        let params = LogParams {
            container: Some(BOX_CONTAINER.to_owned()),
            follow,
            ..LogParams::default()
        };
        match self.pods.log_stream(pod.as_str(), &params).await {
            Ok(reader) => Ok(Box::pin(reader)),
            Err(error) if status_code(&error) == Some(NOT_FOUND) => {
                Err(ClusterError::NoPod(pod.clone()))
            }
            Err(error) => Err(ClusterError::Kube(error)),
        }
    }

    async fn delete(&self, job: &JobName) -> Result<bool, ClusterError> {
        match self
            .jobs
            .delete(job.as_str(), &DeleteParams::background())
            .await
        {
            Ok(_) => Ok(true),
            Err(error) if status_code(&error) == Some(NOT_FOUND) => Ok(false),
            Err(error) => Err(ClusterError::Kube(error)),
        }
    }

    async fn write_key(
        &self,
        secret: &SecretName,
        key: &str,
        value: &str,
    ) -> Result<(), ClusterError> {
        match self.patch_secret(secret, string_data(key, value)).await {
            Err(ClusterError::Kube(error)) if status_code(&error) == Some(NOT_FOUND) => {
                self.create_secret(secret, key, value).await
            }
            other => other,
        }
    }

    async fn create_owned_secret(
        &self,
        secret: &SecretName,
        owner: &CreatedJob,
        key: &str,
        value: &str,
    ) -> Result<(), ClusterError> {
        let params = PostParams {
            field_manager: Some(FIELD_MANAGER.to_owned()),
            ..PostParams::default()
        };
        let mut new = new_secret(secret, key, value);
        new.metadata.owner_references = Some(vec![job_owner(owner)]);
        self.secrets
            .create(&params, &new)
            .await
            .map(|_| ())
            .map_err(ClusterError::Kube)
    }

    async fn create_secret(
        &self,
        secret: &SecretName,
        key: &str,
        value: &str,
    ) -> Result<(), ClusterError> {
        let params = PostParams {
            field_manager: Some(FIELD_MANAGER.to_owned()),
            ..PostParams::default()
        };
        match self
            .secrets
            .create(&params, &new_secret(secret, key, value))
            .await
        {
            Ok(_) => Ok(()),
            Err(error) if status_code(&error) == Some(CONFLICT) => {
                self.patch_secret(secret, string_data(key, value)).await
            }
            Err(error) => Err(ClusterError::Kube(error)),
        }
    }

    async fn delete_key(&self, secret: &SecretName, key: &str) -> Result<bool, ClusterError> {
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

    async fn patch_secret(&self, secret: &SecretName, patch: Value) -> Result<(), ClusterError> {
        let params = PatchParams {
            field_manager: Some(FIELD_MANAGER.to_owned()),
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
        self.create(job).boxed()
    }

    fn list_jobs<'a>(
        &'a self,
        selector: &'a str,
    ) -> BoxFuture<'a, Result<Vec<JobState>, ClusterError>> {
        self.list(selector).boxed()
    }

    fn find_pod<'a>(
        &'a self,
        job: &'a JobName,
    ) -> BoxFuture<'a, Result<Option<PodState>, ClusterError>> {
        self.find(job).boxed()
    }

    fn read_log<'a>(
        &'a self,
        pod: &'a PodName,
        follow: bool,
    ) -> BoxFuture<'a, Result<LogReader, ClusterError>> {
        self.open_log(pod, follow).boxed()
    }

    fn delete_job<'a>(&'a self, job: &'a JobName) -> BoxFuture<'a, Result<bool, ClusterError>> {
        self.delete(job).boxed()
    }

    fn write_secret_key<'a>(
        &'a self,
        secret: &'a SecretName,
        key: &'a str,
        value: &'a str,
    ) -> BoxFuture<'a, Result<(), ClusterError>> {
        self.write_key(secret, key, value).boxed()
    }

    fn delete_secret_key<'a>(
        &'a self,
        secret: &'a SecretName,
        key: &'a str,
    ) -> BoxFuture<'a, Result<bool, ClusterError>> {
        self.delete_key(secret, key).boxed()
    }

    fn create_job_secret<'a>(
        &'a self,
        secret: &'a SecretName,
        owner: &'a CreatedJob,
        key: &'a str,
        value: &'a str,
    ) -> BoxFuture<'a, Result<(), ClusterError>> {
        self.create_owned_secret(secret, owner, key, value).boxed()
    }
}

fn string_data(key: &str, value: &str) -> Value {
    json!({ "stringData": { key: value } })
}

fn new_secret(secret: &SecretName, key: &str, value: &str) -> Secret {
    Secret {
        metadata: ObjectMeta {
            name: Some(secret.as_str().to_owned()),
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

fn job_state(job: &Job) -> Option<JobState> {
    let name = JobName::new(job.metadata.name.clone()?);
    let uid = JobUid::new(job.metadata.uid.clone()?);
    Some(JobState {
        name,
        uid,
        labels: job.metadata.labels.clone().unwrap_or_default(),
        phase: job_phase(job),
        failed_reason: job_failed_reason(job),
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
        reason: box_status.and_then(box_reason),
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
