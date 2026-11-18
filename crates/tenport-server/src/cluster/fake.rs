use std::collections::BTreeMap;
use std::pin::Pin;
use std::sync::Mutex;
use std::task::{Context, Poll};
use std::time::Duration;

use futures::FutureExt;
use futures::future::{BoxFuture, ready};
use futures::io::{AsyncRead, AsyncReadExt, BufReader, Cursor};
use k8s_openapi::api::batch::v1::Job;

use super::Cluster;
use super::model::{
    ClusterError, CreatedJob, JobName, JobPhase, JobState, JobUid, LogReader, PodName, PodState,
    SecretName,
};

type SecretData = BTreeMap<String, String>;

#[derive(Default)]
pub struct FakeCluster {
    jobs: Mutex<BTreeMap<JobName, FakeJob>>,
    secrets: Mutex<BTreeMap<SecretName, SecretData>>,
    secret_owners: Mutex<BTreeMap<SecretName, CreatedJob>>,
    create_time: Duration,
}

struct FakeJob {
    job: Job,
    phase: JobPhase,
    failed_reason: Option<String>,
    pod: Option<FakePod>,
}

struct FakePod {
    state: PodState,
    log: String,
}

impl FakeCluster {
    pub fn with_create_time(create_time: Duration) -> Self {
        Self {
            create_time,
            ..Self::default()
        }
    }

    pub fn jobs(&self) -> Vec<Job> {
        self.lock().values().map(|fake| fake.job.clone()).collect()
    }

    pub fn set_phase(&self, job: &JobName, phase: JobPhase) {
        if let Some(fake) = self.lock().get_mut(job) {
            fake.phase = phase;
        }
    }

    pub fn fail(&self, job: &JobName, reason: &str) {
        if let Some(fake) = self.lock().get_mut(job) {
            fake.phase = JobPhase::Failed;
            fake.failed_reason = Some(reason.to_owned());
        }
    }

    pub fn set_pod(&self, job: &JobName, state: PodState, log: &str) {
        if let Some(fake) = self.lock().get_mut(job) {
            fake.pod = Some(FakePod {
                state,
                log: log.to_owned(),
            });
        }
    }

    pub fn secret(&self, name: &SecretName) -> Option<SecretData> {
        self.lock_secrets().get(name).cloned()
    }

    pub fn secret_owner(&self, name: &SecretName) -> Option<CreatedJob> {
        self.secret_owners
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(name)
            .cloned()
    }

    pub fn uid_of(job: &JobName) -> JobUid {
        JobUid::new(format!("uid-of-{}", job.as_str()))
    }

    fn lock_secrets(&self) -> std::sync::MutexGuard<'_, BTreeMap<SecretName, SecretData>> {
        self.secrets
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<JobName, FakeJob>> {
        self.jobs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl Cluster for FakeCluster {
    fn create_job(&self, job: Job) -> BoxFuture<'_, Result<CreatedJob, ClusterError>> {
        async move {
            tokio::time::sleep(self.create_time).await;
            let name = JobName::new(job.metadata.name.clone().ok_or(ClusterError::NoName)?);
            let mut jobs = self.lock();
            if jobs.contains_key(&name) {
                return Err(ClusterError::Exists(name));
            }
            let fake = FakeJob {
                job,
                phase: JobPhase::Running,
                failed_reason: None,
                pod: None,
            };
            jobs.insert(name.clone(), fake);
            Ok(CreatedJob {
                uid: Self::uid_of(&name),
                name,
            })
        }
        .boxed()
    }

    fn list_jobs<'a>(
        &'a self,
        labels: &'a [(&'a str, &'a str)],
    ) -> BoxFuture<'a, Result<Vec<JobState>, ClusterError>> {
        let states = self
            .lock()
            .iter()
            .map(|(name, fake)| job_state(name, fake))
            .filter(|state| {
                labels
                    .iter()
                    .all(|(key, value)| state.labels.get(*key).map(String::as_str) == Some(*value))
            })
            .collect();
        ready(Ok(states)).boxed()
    }

    fn find_pod<'a>(
        &'a self,
        job: &'a JobName,
    ) -> BoxFuture<'a, Result<Option<PodState>, ClusterError>> {
        let pod = self
            .lock()
            .get(job)
            .and_then(|fake| fake.pod.as_ref())
            .map(|pod| pod.state.clone());
        ready(Ok(pod)).boxed()
    }

    fn read_log<'a>(
        &'a self,
        pod: &'a PodName,
        follow: bool,
    ) -> BoxFuture<'a, Result<LogReader, ClusterError>> {
        let found = self.lock().values().find_map(|fake| {
            let found = fake.pod.as_ref().filter(|found| found.state.name == *pod)?;
            Some((fake.phase, found.log.clone()))
        });
        let reader = found
            .map(|(phase, log)| log_reader(log, follow && phase == JobPhase::Running))
            .ok_or_else(|| ClusterError::NoPod(pod.clone()));
        ready(reader).boxed()
    }

    fn delete_job<'a>(&'a self, job: &'a JobName) -> BoxFuture<'a, Result<(), ClusterError>> {
        self.lock().remove(job);
        ready(Ok(())).boxed()
    }

    fn write_secret_key<'a>(
        &'a self,
        secret: &'a SecretName,
        key: &'a str,
        value: &'a str,
    ) -> BoxFuture<'a, Result<(), ClusterError>> {
        self.lock_secrets()
            .entry(secret.clone())
            .or_default()
            .insert(key.to_owned(), value.to_owned());
        ready(Ok(())).boxed()
    }

    fn delete_secret_key<'a>(
        &'a self,
        secret: &'a SecretName,
        key: &'a str,
    ) -> BoxFuture<'a, Result<bool, ClusterError>> {
        let deleted = self
            .lock_secrets()
            .get_mut(secret)
            .is_some_and(|data| data.remove(key).is_some());
        ready(Ok(deleted)).boxed()
    }

    fn create_job_secret<'a>(
        &'a self,
        secret: &'a SecretName,
        owner: &'a CreatedJob,
        key: &'a str,
        value: &'a str,
    ) -> BoxFuture<'a, Result<(), ClusterError>> {
        async move {
            self.write_secret_key(secret, key, value).await?;
            self.secret_owners
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(secret.clone(), owner.clone());
            Ok(())
        }
        .boxed()
    }
}

fn log_reader(log: String, still_running: bool) -> LogReader {
    let written = Cursor::new(log.into_bytes());
    if still_running {
        return Box::pin(BufReader::new(written.chain(StillRunning)));
    }
    Box::pin(written)
}

struct StillRunning;

impl AsyncRead for StillRunning {
    fn poll_read(
        self: Pin<&mut Self>,
        _context: &mut Context<'_>,
        _buffer: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        Poll::Pending
    }
}

fn job_state(name: &JobName, fake: &FakeJob) -> JobState {
    JobState {
        name: name.clone(),
        uid: FakeCluster::uid_of(name),
        labels: fake.job.metadata.labels.clone().unwrap_or_default(),
        phase: fake.phase,
        failed_reason: fake.failed_reason.clone(),
    }
}

#[cfg(test)]
mod tests;
