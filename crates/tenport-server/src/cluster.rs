mod fake;
mod kube_api;
mod model;

use futures::future::BoxFuture;
use k8s_openapi::api::batch::v1::Job;

pub use fake::FakeCluster;
pub use kube_api::KubeCluster;
pub use model::{
    ClusterError, CreatedJob, JobName, JobPhase, JobState, JobUid, LogReader, PodName, PodState,
    SecretName,
};

pub const MANAGED_BY_LABEL: &str = "app.kubernetes.io/managed-by";
pub const MANAGER: &str = "tenport";
pub const PROJECT_ID_LABEL: &str = "project-id";
pub const TASK_LABEL: &str = "task";
pub const TENPORT_JOB_LABELS: &[(&str, &str)] = &[(MANAGED_BY_LABEL, MANAGER)];
pub const BOX_CONTAINER: &str = "box";

pub trait Cluster: Send + Sync {
    fn create_job(&self, job: Job) -> BoxFuture<'_, Result<CreatedJob, ClusterError>>;
    fn list_jobs<'a>(
        &'a self,
        labels: &'a [(&'a str, &'a str)],
    ) -> BoxFuture<'a, Result<Vec<JobState>, ClusterError>>;
    fn find_pod<'a>(
        &'a self,
        job: &'a JobName,
    ) -> BoxFuture<'a, Result<Option<PodState>, ClusterError>>;
    fn read_log<'a>(
        &'a self,
        pod: &'a PodName,
        follow: bool,
    ) -> BoxFuture<'a, Result<LogReader, ClusterError>>;
    fn delete_job<'a>(&'a self, job: &'a JobName) -> BoxFuture<'a, Result<(), ClusterError>>;
    fn write_secret_key<'a>(
        &'a self,
        secret: &'a SecretName,
        key: &'a str,
        value: &'a str,
    ) -> BoxFuture<'a, Result<(), ClusterError>>;
    fn delete_secret_key<'a>(
        &'a self,
        secret: &'a SecretName,
        key: &'a str,
    ) -> BoxFuture<'a, Result<bool, ClusterError>>;
    fn create_job_secret<'a>(
        &'a self,
        secret: &'a SecretName,
        owner: &'a CreatedJob,
        key: &'a str,
        value: &'a str,
    ) -> BoxFuture<'a, Result<(), ClusterError>>;
}
