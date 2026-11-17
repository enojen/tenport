use futures::FutureExt;
use futures::future::BoxFuture;
use k8s_openapi::api::batch::v1::Job;
use k8s_openapi::api::core::v1::{ContainerStatus, Pod};
use kube::api::{DeleteParams, ListParams, LogParams, PostParams};
use kube::{Api, Client};
use time::OffsetDateTime;

use super::Cluster;
use super::model::{
    ClusterError, JobName, JobPhase, JobState, LogRead, LogReader, PodName, PodState,
};

const BOX_CONTAINER: &str = "box";
const CONFLICT: u16 = 409;
const NOT_FOUND: u16 = 404;
const PENDING: &str = "Pending";

pub struct KubeCluster {
    jobs: Api<Job>,
    pods: Api<Pod>,
}

impl KubeCluster {
    pub async fn connect(namespace: &str) -> Result<Self, ClusterError> {
        let client = Client::try_default().await.map_err(ClusterError::Kube)?;
        Ok(Self::new(client, namespace))
    }

    pub fn new(client: Client, namespace: &str) -> Self {
        Self {
            jobs: Api::namespaced(client.clone(), namespace),
            pods: Api::namespaced(client, namespace),
        }
    }

    async fn create(&self, job: Job) -> Result<JobName, ClusterError> {
        let name = JobName::new(job.metadata.name.clone().ok_or(ClusterError::NoName)?);
        match self.jobs.create(&PostParams::default(), &job).await {
            Ok(_) => Ok(name),
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

    async fn log(&self, pod: &PodName, read: LogRead) -> Result<LogReader, ClusterError> {
        let params = LogParams {
            container: Some(BOX_CONTAINER.to_owned()),
            follow: read.follow,
            tail_lines: read.tail_lines,
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
}

impl Cluster for KubeCluster {
    fn create_job(&self, job: Job) -> BoxFuture<'_, Result<JobName, ClusterError>> {
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
        read: LogRead,
    ) -> BoxFuture<'a, Result<LogReader, ClusterError>> {
        self.log(pod, read).boxed()
    }

    fn delete_job<'a>(&'a self, job: &'a JobName) -> BoxFuture<'a, Result<bool, ClusterError>> {
        self.delete(job).boxed()
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
    let created = to_offset(job.metadata.creation_timestamp.as_ref()?.0)?;
    Some(JobState {
        name,
        labels: job.metadata.labels.clone().unwrap_or_default(),
        created,
        phase: job_phase(job),
    })
}

fn job_phase(job: &Job) -> JobPhase {
    let status = job.status.clone().unwrap_or_default();
    if status.succeeded.unwrap_or(0) > 0 {
        return JobPhase::Succeeded;
    }
    if status.failed.unwrap_or(0) > 0 {
        return JobPhase::Failed;
    }
    JobPhase::Running
}

fn to_offset(timestamp: k8s_openapi::jiff::Timestamp) -> Option<OffsetDateTime> {
    OffsetDateTime::from_unix_timestamp_nanos(timestamp.as_nanosecond()).ok()
}

fn newest_pod(pods: Vec<Pod>) -> Option<Pod> {
    pods.into_iter()
        .max_by_key(|pod| pod.metadata.creation_timestamp.as_ref().map(|time| time.0))
}

fn pod_state(pod: &Pod) -> Option<PodState> {
    let name = PodName::new(pod.metadata.name.clone()?);
    let status = pod.status.clone().unwrap_or_default();
    let started = status
        .phase
        .as_deref()
        .is_some_and(|phase| phase != PENDING);
    let box_status = status
        .container_statuses
        .unwrap_or_default()
        .into_iter()
        .find(|container| container.name == BOX_CONTAINER);
    Some(PodState {
        name,
        started,
        reason: box_status.as_ref().and_then(box_reason),
    })
}

fn box_reason(status: &ContainerStatus) -> Option<String> {
    let state = status.state.as_ref()?;
    let terminated = state.terminated.as_ref().and_then(|t| t.reason.clone());
    terminated.or_else(|| state.waiting.as_ref().and_then(|w| w.reason.clone()))
}

#[cfg(test)]
mod tests;
