use super::*;
use crate::db;

#[tokio::test]
async fn adds_an_organization_with_a_short_id() {
    let (db, _dir) = db::temp().await;
    let id = add_organization(&db, Actor::Server, " Wegobi ")
        .await
        .unwrap();
    assert!(
        id.as_str().starts_with("o-") && id.as_str().len() == 8,
        "{id}"
    );
    let mut conn = db.acquire().await.unwrap();
    let organization = store::organization_by_name(&mut conn, "Wegobi")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(organization.id, id.as_str());
    assert_eq!(organization.name, "Wegobi");
    assert_eq!(organization.created_by, "server");
}

#[tokio::test]
async fn refuses_an_empty_name() {
    let (db, _dir) = db::temp().await;
    let error = add_organization(&db, Actor::Server, "  ")
        .await
        .unwrap_err();
    assert!(matches!(error, AccountsError::EmptyName), "{error}");
}

#[tokio::test]
async fn refuses_a_name_that_exists() {
    let (db, _dir) = db::temp().await;
    add_organization(&db, Actor::Server, "Wegobi")
        .await
        .unwrap();
    let error = add_organization(&db, Actor::Server, "Wegobi")
        .await
        .unwrap_err();
    assert!(
        matches!(&error, AccountsError::OrganizationExists { name } if name == "Wegobi"),
        "{error}"
    );
}
