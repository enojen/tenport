use std::collections::BTreeMap;
use std::pin::Pin;
use std::sync::Mutex;
use std::task::{Context, Poll};
use std::time::Duration;

use futures::FutureExt;
use futures::future::{BoxFuture, ready};
use futures::io::{AsyncRead, AsyncReadExt, BufReader, Cursor};
use k8s_openapi::api::batch::v1::Job;
use time::OffsetDateTime;

use super::Cluster;
use super::model::{
    ClusterError, CreatedJob, JobName, JobPhase, JobState, JobUid, LogRead, LogReader, PodName,
    PodState, SecretName,
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
    created: OffsetDateTime,
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

    fn create_owned_secret(&self, secret: &SecretName, owner: &CreatedJob, key: &str, value: &str) {
        self.write_key(secret, key, value);
        self.secret_owners
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(secret.clone(), owner.clone());
    }

    fn lock_secrets(&self) -> std::sync::MutexGuard<'_, BTreeMap<SecretName, SecretData>> {
        self.secrets
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn write_key(&self, secret: &SecretName, key: &str, value: &str) {
        self.lock_secrets()
            .entry(secret.clone())
            .or_default()
            .insert(key.to_owned(), value.to_owned());
    }

    fn delete_key(&self, secret: &SecretName, key: &str) -> bool {
        self.lock_secrets()
            .get_mut(secret)
            .is_some_and(|data| data.remove(key).is_some())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<JobName, FakeJob>> {
        self.jobs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn create(&self, job: Job) -> Result<CreatedJob, ClusterError> {
        let name = JobName::new(job.metadata.name.clone().ok_or(ClusterError::NoName)?);
        let mut jobs = self.lock();
        if jobs.contains_key(&name) {
            return Err(ClusterError::Exists(name));
        }
        let fake = FakeJob {
            job,
            created: OffsetDateTime::now_utc(),
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

    fn list(&self, selector: &str) -> Vec<JobState> {
        let wanted = parse_selector(selector);
        self.lock()
            .iter()
            .map(|(name, fake)| job_state(name, fake))
            .filter(|state| {
                wanted
                    .iter()
                    .all(|(key, value)| state.labels.get(*key) == Some(&(*value).to_owned()))
            })
            .collect()
    }

    fn find(&self, job: &JobName) -> Option<PodState> {
        self.lock()
            .get(job)?
            .pod
            .as_ref()
            .map(|pod| pod.state.clone())
    }

    fn open_log(&self, pod: &PodName, read: LogRead) -> Result<LogReader, ClusterError> {
        let jobs = self.lock();
        let found = jobs.values().find_map(|fake| {
            let found = fake.pod.as_ref().filter(|found| found.state.name == *pod)?;
            Some((fake.phase, found.log.clone()))
        });
        let (phase, log) = found.ok_or_else(|| ClusterError::NoPod(pod.clone()))?;
        let written = Cursor::new(log.into_bytes());
        if read.follow && phase == JobPhase::Running {
            return Ok(Box::pin(BufReader::new(written.chain(StillRunning))));
        }
        Ok(Box::pin(written))
    }

    fn delete(&self, job: &JobName) -> bool {
        self.lock().remove(job).is_some()
    }
}

impl Cluster for FakeCluster {
    fn create_job(&self, job: Job) -> BoxFuture<'_, Result<CreatedJob, ClusterError>> {
        async move {
            tokio::time::sleep(self.create_time).await;
            self.create(job)
        }
        .boxed()
    }

    fn list_jobs<'a>(
        &'a self,
        selector: &'a str,
    ) -> BoxFuture<'a, Result<Vec<JobState>, ClusterError>> {
        ready(Ok(self.list(selector))).boxed()
    }

    fn find_pod<'a>(
        &'a self,
        job: &'a JobName,
    ) -> BoxFuture<'a, Result<Option<PodState>, ClusterError>> {
        ready(Ok(self.find(job))).boxed()
    }

    fn read_log<'a>(
        &'a self,
        pod: &'a PodName,
        read: LogRead,
    ) -> BoxFuture<'a, Result<LogReader, ClusterError>> {
        ready(self.open_log(pod, read)).boxed()
    }

    fn delete_job<'a>(&'a self, job: &'a JobName) -> BoxFuture<'a, Result<bool, ClusterError>> {
        ready(Ok(self.delete(job))).boxed()
    }

    fn write_secret_key<'a>(
        &'a self,
        secret: &'a SecretName,
        key: &'a str,
        value: &'a str,
    ) -> BoxFuture<'a, Result<(), ClusterError>> {
        self.write_key(secret, key, value);
        ready(Ok(())).boxed()
    }

    fn delete_secret_key<'a>(
        &'a self,
        secret: &'a SecretName,
        key: &'a str,
    ) -> BoxFuture<'a, Result<bool, ClusterError>> {
        ready(Ok(self.delete_key(secret, key))).boxed()
    }

    fn create_job_secret<'a>(
        &'a self,
        secret: &'a SecretName,
        owner: &'a CreatedJob,
        key: &'a str,
        value: &'a str,
    ) -> BoxFuture<'a, Result<(), ClusterError>> {
        self.create_owned_secret(secret, owner, key, value);
        ready(Ok(())).boxed()
    }
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
        created: fake.created,
        phase: fake.phase,
        failed_reason: fake.failed_reason.clone(),
    }
}

fn parse_selector(selector: &str) -> Vec<(&str, &str)> {
    selector
        .split(',')
        .filter_map(|pair| pair.split_once('='))
        .collect()
}

#[cfg(test)]
mod tests;
