use time::OffsetDateTime;

use super::{Runs, RunsError, TENPORT_JOBS};
use crate::cluster::{JobState, JobUid};
use crate::db::Db;
use crate::github::GITHUB_HOSTS;
use crate::projects::{self, ProjectId};

const PROJECT_ID_LABEL: &str = "project-id";
const TOKEN_REUSE_MARGIN: time::Duration = time::Duration::minutes(5);

impl Runs {
    pub async fn run_credential(
        &self,
        db: &Db,
        placeholder: &str,
        host: &str,
    ) -> Result<String, RunsError> {
        let none = || RunsError::NoRunCredential {
            host: host.to_owned(),
        };
        if !GITHUB_HOSTS.contains(&host) {
            return Err(none());
        }
        let open: Vec<JobState> = self
            .cluster
            .list_jobs(TENPORT_JOBS)
            .await?
            .into_iter()
            .filter(|job| job.is_running())
            .collect();
        self.forget_tokens_of_ended_jobs(&open);
        let job = open
            .iter()
            .find(|job| self.run_credential_key.placeholder(&job.uid) == placeholder)
            .ok_or_else(none)?;
        let now = OffsetDateTime::now_utc();
        if let Some(token) = self.reusable_token(&job.uid, now) {
            return Ok(token);
        }
        let project = job
            .labels
            .get(PROJECT_ID_LABEL)
            .and_then(|id| ProjectId::parse(id).ok())
            .ok_or_else(none)?;
        let project = projects::find(db, &project).await?.ok_or_else(none)?;
        let token = self.github.repo_token(&project.repo, now).await?;
        let value = token.token.clone();
        lock(&self.repo_tokens).insert(job.uid.clone(), token);
        Ok(value)
    }

    fn reusable_token(&self, job: &JobUid, now: OffsetDateTime) -> Option<String> {
        lock(&self.repo_tokens)
            .get(job)
            .filter(|token| token.expires - now > TOKEN_REUSE_MARGIN)
            .map(|token| token.token.clone())
    }

    fn forget_tokens_of_ended_jobs(&self, open: &[JobState]) {
        lock(&self.repo_tokens).retain(|uid, _| open.iter().any(|job| job.uid == *uid));
    }
}

fn lock<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
