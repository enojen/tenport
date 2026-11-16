use serde_json::Value;

use super::*;
use crate::accounts::{AccountsEvent, OrgId, OrganizationAdded};
use crate::db;
use crate::name::Name;

fn organization_added(name: &str) -> Event {
    Event::Accounts(AccountsEvent::OrganizationAdded(OrganizationAdded {
        id: OrgId::random(),
        name: Name::parse(name).unwrap(),
    }))
}

fn entry(event: Event) -> Entry {
    Entry {
        id: EventId::now(),
        at: OffsetDateTime::now_utc(),
        actor: Actor::Server,
        event,
    }
}

async fn events_in(db: &Db) -> i64 {
    store::count_events(&mut db.acquire().await.unwrap())
        .await
        .unwrap()
}

#[tokio::test]
async fn writes_the_event_and_its_table_together() {
    let (db, _dir) = db::temp().await;
    let event = organization_added("Wegobi");
    let id = append(&db, Actor::Server, event.clone()).await.unwrap();
    let mut conn = db.acquire().await.unwrap();
    let row = store::find_event_by_id(&mut conn, &id.to_string())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.kind, "organization.added");
    assert_eq!(row.subject, event.subject());
    assert_eq!(row.actor, "server");
    assert!(accounts::organization_id(&db, "Wegobi").await.is_some());
}

#[tokio::test]
async fn the_body_names_its_own_type() {
    let event = organization_added("Wegobi");
    let body: Value = serde_json::to_value(&event).unwrap();
    assert_eq!(body["type"], event.kind());
    assert_eq!(serde_json::from_value::<Event>(body).unwrap(), event);
}

#[test]
fn an_unknown_type_is_refused() {
    let body = serde_json::json!({ "type": "organization.renamed", "id": "o-abcdef" });
    assert!(serde_json::from_value::<Event>(body).is_err());
}

#[tokio::test]
async fn the_same_event_twice_is_appended_once() {
    let (db, _dir) = db::temp().await;
    let entry = entry(organization_added("Wegobi"));
    assert_eq!(append_entry(&db, &entry).await.unwrap(), Appended::New);
    assert_eq!(
        append_entry(&db, &entry).await.unwrap(),
        Appended::Duplicate
    );
    assert_eq!(events_in(&db).await, 1);
}

#[tokio::test]
async fn the_same_id_with_other_content_is_a_conflict() {
    let (db, _dir) = db::temp().await;
    let first = entry(organization_added("Wegobi"));
    append_entry(&db, &first).await.unwrap();
    let second = Entry {
        event: organization_added("Other"),
        ..first.clone()
    };
    let error = append_entry(&db, &second).await.unwrap_err();
    assert!(
        matches!(error, EventsError::Conflict { id } if id == first.id),
        "{error}"
    );
    assert!(accounts::organization_id(&db, "Other").await.is_none());
}

#[tokio::test]
async fn a_failed_table_write_leaves_no_event() {
    let (db, _dir) = db::temp().await;
    append(&db, Actor::Server, organization_added("Wegobi"))
        .await
        .unwrap();
    let error = append(&db, Actor::Server, organization_added("Wegobi"))
        .await
        .unwrap_err();
    assert!(error.violates_unique("organizations.name"), "{error}");
    assert_eq!(events_in(&db).await, 1);
}

#[test]
fn ids_follow_the_time_they_were_made() {
    let first = EventId::now();
    let second = EventId::now();
    assert!(first < second);
}

#[tokio::test]
async fn a_batch_is_written_whole_or_not_at_all() {
    let (db, _dir) = db::temp().await;
    let error = append_all(
        &db,
        Actor::Server,
        vec![organization_added("Wegobi"), organization_added("Wegobi")],
    )
    .await
    .unwrap_err();
    assert!(error.violates_unique("organizations.name"), "{error}");
    assert_eq!(events_in(&db).await, 0);
    assert!(accounts::organization_id(&db, "Wegobi").await.is_none());
}
