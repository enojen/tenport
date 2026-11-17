use std::collections::BTreeMap;
use std::sync::Mutex;

use futures::FutureExt;
use futures::future::{BoxFuture, ready};
use futures::io::Cursor;
use k8s_openapi::api::batch::v1::Job;
use time::OffsetDateTime;

use super::Cluster;
use super::model::{
    ClusterError, JobName, JobPhase, JobState, LogRead, LogReader, PodName, PodState,
};

#[derive(Default)]
pub struct FakeCluster {
    jobs: Mutex<BTreeMap<JobName, FakeJob>>,
}

struct FakeJob {
    job: Job,
    created: OffsetDateTime,
    phase: JobPhase,
    pod: Option<FakePod>,
}

struct FakePod {
    state: PodState,
    log: String,
}

impl FakeCluster {
    pub fn jobs(&self) -> Vec<Job> {
        self.lock().values().map(|fake| fake.job.clone()).collect()
    }

    pub fn set_phase(&self, job: &JobName, phase: JobPhase) {
        if let Some(fake) = self.lock().get_mut(job) {
            fake.phase = phase;
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

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<JobName, FakeJob>> {
        self.jobs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn create(&self, job: Job) -> Result<JobName, ClusterError> {
        let name = JobName::new(job.metadata.name.clone().ok_or(ClusterError::NoName)?);
        let mut jobs = self.lock();
        if jobs.contains_key(&name) {
            return Err(ClusterError::Exists(name));
        }
        let fake = FakeJob {
            job,
            created: OffsetDateTime::now_utc(),
            phase: JobPhase::Running,
            pod: None,
        };
        jobs.insert(name.clone(), fake);
        Ok(name)
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

    fn log(&self, pod: &PodName) -> Result<LogReader, ClusterError> {
        let jobs = self.lock();
        let found = jobs
            .values()
            .filter_map(|fake| fake.pod.as_ref())
            .find(|fake| fake.state.name == *pod);
        let log = found
            .ok_or_else(|| ClusterError::NoPod(pod.clone()))?
            .log
            .clone();
        Ok(Box::pin(Cursor::new(log.into_bytes())))
    }

    fn delete(&self, job: &JobName) -> bool {
        self.lock().remove(job).is_some()
    }
}

impl Cluster for FakeCluster {
    fn create_job(&self, job: Job) -> BoxFuture<'_, Result<JobName, ClusterError>> {
        ready(self.create(job)).boxed()
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
        _read: LogRead,
    ) -> BoxFuture<'a, Result<LogReader, ClusterError>> {
        ready(self.log(pod)).boxed()
    }

    fn delete_job<'a>(&'a self, job: &'a JobName) -> BoxFuture<'a, Result<bool, ClusterError>> {
        ready(Ok(self.delete(job))).boxed()
    }
}

fn job_state(name: &JobName, fake: &FakeJob) -> JobState {
    JobState {
        name: name.clone(),
        labels: fake.job.metadata.labels.clone().unwrap_or_default(),
        created: fake.created,
        phase: fake.phase,
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
