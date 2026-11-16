use super::*;
use crate::accounts::{GithubLogin, NewUser, Registered, Registration};
use crate::db;

async fn register(db: &Db, org: &str, github: &str, role: Role) -> Registered {
    let registration = Registration {
        org: Name::parse(org).unwrap(),
        user: NewUser {
            name: Name::parse(github).unwrap(),
            email: None,
            github: GithubLogin::parse(github).unwrap(),
        },
        role,
        token_name: Name::parse("laptop").unwrap(),
    };
    accounts::register(db, Actor::Server, registration)
        .await
        .unwrap()
}

fn new_project(org: &OrgId, name: &str) -> NewProject {
    NewProject {
        org: org.clone(),
        name: Name::parse(name).unwrap(),
        repo: Repo::parse("enojen/tenport").unwrap(),
    }
}

#[tokio::test]
async fn an_admin_adds_a_project_and_sees_it() {
    let (db, _dir) = db::temp().await;
    let admin = register(&db, "Wegobi", "enojen", Role::Admin).await;
    let added = add(&db, &admin.user, new_project(&admin.org, "Tenport"))
        .await
        .unwrap();
    assert_eq!(list_of_user(&db, &admin.user).await.unwrap(), vec![added]);
}

#[tokio::test]
async fn a_member_cannot_add_a_project() {
    let (db, _dir) = db::temp().await;
    let admin = register(&db, "Wegobi", "enojen", Role::Admin).await;
    let member = register(&db, "Wegobi", "ali", Role::Member).await;
    let error = add(&db, &member.user, new_project(&admin.org, "Tenport"))
        .await
        .unwrap_err();
    assert!(
        matches!(&error, ProjectsError::NotAdmin { org } if *org == admin.org),
        "{error}"
    );
    assert!(list_of_user(&db, &admin.user).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_user_sees_only_the_projects_of_their_organizations() {
    let (db, _dir) = db::temp().await;
    let wegobi = register(&db, "Wegobi", "enojen", Role::Admin).await;
    let other = register(&db, "Other", "ali", Role::Admin).await;
    add(&db, &wegobi.user, new_project(&wegobi.org, "Tenport"))
        .await
        .unwrap();
    let theirs = add(&db, &other.user, new_project(&other.org, "Shop"))
        .await
        .unwrap();
    assert_eq!(list_of_user(&db, &other.user).await.unwrap(), vec![theirs]);
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
