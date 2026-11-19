use super::*;
use crate::db;
use crate::fixtures::add_user;

fn new_project(organization: &OrganizationId, name: &str) -> NewProject {
    NewProject {
        organization: organization.clone(),
        name: Name::parse(name).unwrap(),
        repo: Repo::parse("enojen/tenport").unwrap(),
    }
}

#[tokio::test]
async fn an_admin_adds_a_project_and_sees_it() {
    let (db, _dir) = db::temp().await;
    let admin = add_user(&db, "Wegobi", "enojen", Role::Admin).await;
    let added = add(
        &db,
        &admin.user,
        new_project(&admin.organization, "Tenport"),
    )
    .await
    .unwrap();
    assert_eq!(
        list_of_user(&db, &admin.user, None, 50).await.unwrap(),
        vec![added]
    );
}

#[tokio::test]
async fn a_member_cannot_add_a_project() {
    let (db, _dir) = db::temp().await;
    let admin = add_user(&db, "Wegobi", "enojen", Role::Admin).await;
    let member = add_user(&db, "Wegobi", "ali", Role::Member).await;
    let error = add(
        &db,
        &member.user,
        new_project(&admin.organization, "Tenport"),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(&error, ProjectsError::NotAdmin { organization } if *organization == admin.organization),
        "{error}"
    );
    assert!(
        list_of_user(&db, &admin.user, None, 50)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn a_user_sees_only_the_projects_of_their_organizations() {
    let (db, _dir) = db::temp().await;
    let wegobi = add_user(&db, "Wegobi", "enojen", Role::Admin).await;
    let other = add_user(&db, "Other", "ali", Role::Admin).await;
    add(
        &db,
        &wegobi.user,
        new_project(&wegobi.organization, "Tenport"),
    )
    .await
    .unwrap();
    let theirs = add(&db, &other.user, new_project(&other.organization, "Shop"))
        .await
        .unwrap();
    assert_eq!(
        list_of_user(&db, &other.user, None, 50).await.unwrap(),
        vec![theirs]
    );
}

#[test]
fn a_project_id_has_the_shape_of_the_tenbox_yaml_id() {
    let id = ProjectId::random();
    assert_eq!(id.as_str().len(), 6);
    assert!(
        id.as_str()
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
        "{id}"
    );
}

#[test]
fn a_repo_is_owner_and_name() {
    assert_eq!(
        Repo::parse(" enojen/tenport ").unwrap().as_str(),
        "enojen/tenport"
    );
    assert!(Repo::parse("Wegobi-Studio/my_repo.js").is_ok());
    for bad in [
        "",
        "tenport",
        "/tenport",
        "enojen/",
        "enojen/ten/port",
        "en jen/tenport",
        "https://github.com/enojen/tenport",
    ] {
        assert_eq!(Repo::parse(bad), Err(RepoError), "{bad:?}");
    }
}

#[test]
fn a_secret_value_is_never_empty_and_never_printed() {
    assert!(SecretValue::parse("").is_err());
    assert!(SecretValue::parse(&"x".repeat(4097)).is_err());
    let value = SecretValue::parse("lin_api_not_a_real_key").unwrap();
    assert_eq!(value.expose(), "lin_api_not_a_real_key");
    assert!(!format!("{value:?}").contains("lin_api"));
}

#[test]
fn the_secret_of_a_project_is_named_after_its_id() {
    let project = ProjectId::parse("abc123").unwrap();
    assert_eq!(project_secret(&project).as_str(), "project-abc123");
}
