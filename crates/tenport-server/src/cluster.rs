mod fake;
mod kube_api;
mod model;
mod nodes;

use futures::future::BoxFuture;
use k8s_openapi::api::batch::v1::Job;

pub use fake::FakeCluster;
pub use kube_api::KubeCluster;
pub use model::{
    ClusterError, CreatedJob, JobName, JobPhase, JobState, JobUid, LogRead, LogReader, PodName,
    PodState, SecretName,
};

pub trait Cluster: Send + Sync {
    fn create_job(&self, job: Job) -> BoxFuture<'_, Result<CreatedJob, ClusterError>>;
    fn list_jobs<'a>(
        &'a self,
        selector: &'a str,
    ) -> BoxFuture<'a, Result<Vec<JobState>, ClusterError>>;
    fn find_pod<'a>(
        &'a self,
        job: &'a JobName,
    ) -> BoxFuture<'a, Result<Option<PodState>, ClusterError>>;
    fn read_log<'a>(
        &'a self,
        pod: &'a PodName,
        read: LogRead,
    ) -> BoxFuture<'a, Result<LogReader, ClusterError>>;
    fn delete_job<'a>(&'a self, job: &'a JobName) -> BoxFuture<'a, Result<bool, ClusterError>>;
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
