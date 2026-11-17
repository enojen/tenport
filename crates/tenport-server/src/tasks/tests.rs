use super::*;
use crate::accounts::Role;
use crate::db;
use crate::fixtures::{project_of, register};
use crate::name::Name;
use crate::tracker::TrackerUrl;

fn started(project: &ProjectId, task: &str) -> Reported {
    reported(TasksEvent::TaskStarted(TaskStarted {
        project: project.clone(),
        task: TaskId::parse(task).unwrap(),
        title: Some(Name::parse("Add projects").unwrap()),
        url: Some(TrackerUrl::parse("https://github.com/enojen/tenport/issues/7").unwrap()),
    }))
}

fn closed(project: &ProjectId, task: &str) -> Reported {
    reported(TasksEvent::TaskClosed(TaskClosed {
        project: project.clone(),
        task: TaskId::parse(task).unwrap(),
    }))
}

fn reported(event: TasksEvent) -> Reported {
    Reported {
        id: EventId::now(),
        at: OffsetDateTime::now_utc(),
        event,
    }
}

async fn open_tasks(db: &Db) -> i64 {
    store::count_open_tasks(&mut db.acquire().await.unwrap())
        .await
        .unwrap()
}

#[tokio::test]
async fn a_started_task_is_open_until_it_is_closed() {
    let (db, _dir) = db::temp().await;
    let owner = register(&db, "Wegobi", "enojen", Role::Admin).await;
    let project = project_of(&db, &owner).await;
    record_events(&db, &owner.user, vec![started(&project, "TEN-1")])
        .await
        .unwrap();
    assert_eq!(open_tasks(&db).await, 1);
    record_events(&db, &owner.user, vec![closed(&project, "TEN-1")])
        .await
        .unwrap();
    assert_eq!(open_tasks(&db).await, 0);
}

#[tokio::test]
async fn the_same_events_sent_again_are_skipped() {
    let (db, _dir) = db::temp().await;
    let owner = register(&db, "Wegobi", "enojen", Role::Admin).await;
    let project = project_of(&db, &owner).await;
    let batch = vec![started(&project, "TEN-1"), closed(&project, "TEN-1")];
    let first = record_events(&db, &owner.user, batch.clone())
        .await
        .unwrap();
    let again = record_events(&db, &owner.user, batch).await.unwrap();
    assert_eq!(
        first,
        Recorded {
            stored: 2,
            skipped: 0
        }
    );
    assert_eq!(
        again,
        Recorded {
            stored: 0,
            skipped: 2
        }
    );
}

#[tokio::test]
async fn closing_a_task_that_never_started_is_refused() {
    let (db, _dir) = db::temp().await;
    let owner = register(&db, "Wegobi", "enojen", Role::Admin).await;
    let project = project_of(&db, &owner).await;
    let error = record_events(&db, &owner.user, vec![closed(&project, "TEN-1")])
        .await
        .unwrap_err();
    assert!(
        matches!(
            error,
            TasksError::Events(EventsError::Refused {
                reason: "the task is not open",
                ..
            })
        ),
        "{error}"
    );
}

#[tokio::test]
async fn an_open_task_is_not_started_again() {
    let (db, _dir) = db::temp().await;
    let owner = register(&db, "Wegobi", "enojen", Role::Admin).await;
    let project = project_of(&db, &owner).await;
    record_events(&db, &owner.user, vec![started(&project, "TEN-1")])
        .await
        .unwrap();
    let error = record_events(&db, &owner.user, vec![started(&project, "TEN-1")])
        .await
        .unwrap_err();
    assert!(
        matches!(
            error,
            TasksError::Events(EventsError::Refused {
                reason: "the task is open already",
                ..
            })
        ),
        "{error}"
    );
}

#[tokio::test]
async fn a_closed_task_starts_again() {
    let (db, _dir) = db::temp().await;
    let owner = register(&db, "Wegobi", "enojen", Role::Admin).await;
    let project = project_of(&db, &owner).await;
    let first_round = vec![started(&project, "TEN-1"), closed(&project, "TEN-1")];
    record_events(&db, &owner.user, first_round).await.unwrap();
    record_events(&db, &owner.user, vec![started(&project, "TEN-1")])
        .await
        .unwrap();
    assert_eq!(open_tasks(&db).await, 1);
    record_events(&db, &owner.user, vec![closed(&project, "TEN-1")])
        .await
        .unwrap();
    assert_eq!(open_tasks(&db).await, 0);
}

#[tokio::test]
async fn a_refused_event_leaves_the_whole_batch_out() {
    let (db, _dir) = db::temp().await;
    let owner = register(&db, "Wegobi", "enojen", Role::Admin).await;
    let project = project_of(&db, &owner).await;
    let batch = vec![started(&project, "TEN-1"), closed(&project, "TEN-2")];
    record_events(&db, &owner.user, batch).await.unwrap_err();
    assert_eq!(open_tasks(&db).await, 0);
}

#[tokio::test]
async fn only_a_member_of_the_organization_reports_its_tasks() {
    let (db, _dir) = db::temp().await;
    let owner = register(&db, "Wegobi", "enojen", Role::Admin).await;
    let stranger = register(&db, "Other", "ali", Role::Admin).await;
    let project = project_of(&db, &owner).await;
    let error = record_events(&db, &stranger.user, vec![started(&project, "TEN-1")])
        .await
        .unwrap_err();
    assert!(
        matches!(&error, TasksError::Projects(ProjectsError::NotMember { project: refused }) if *refused == project),
        "{error}"
    );
    assert_eq!(open_tasks(&db).await, 0);
}

#[test]
fn a_task_id_follows_the_tenbox_rule() {
    for good in ["TEN-301", "42", "a_b-c"] {
        assert!(TaskId::parse(good).is_ok(), "{good:?}");
    }
    for bad in ["", "-a", "_a", "a b", "TEN/1", "#12"] {
        assert_eq!(TaskId::parse(bad), Err(TaskIdError), "{bad:?}");
    }
}

#[test]
fn an_event_without_a_title_or_url_still_reads() {
    let body = serde_json::json!({ "type": "task.started", "project": "k7f3qa", "task": "TEN-1" });
    let event: TasksEvent = serde_json::from_value(body).unwrap();
    assert!(matches!(
        event,
        TasksEvent::TaskStarted(TaskStarted {
            title: None,
            url: None,
            ..
        })
    ));
}
