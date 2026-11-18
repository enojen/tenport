use crate::accounts::{self, AddedUser, GithubLogin, NewUser, Role};
use crate::db::Db;
use crate::events::Actor;
use crate::name::Name;
use crate::projects::{self, NewProject, ProjectId, Repo};

pub async fn add_user(db: &Db, organization: &str, github: &str, role: Role) -> AddedUser {
    let new_user = NewUser {
        organization_name: Name::parse(organization).unwrap(),
        name: Name::parse(github).unwrap(),
        email: None,
        github: GithubLogin::parse(github).unwrap(),
        role,
        token_name: Name::parse("laptop").unwrap(),
    };
    accounts::add_user(db, Actor::Server, new_user)
        .await
        .unwrap()
}

pub async fn project_of(db: &Db, owner: &AddedUser) -> ProjectId {
    let new = NewProject {
        organization: owner.organization.clone(),
        name: Name::parse("Tenport").unwrap(),
        repo: Repo::parse("enojen/tenport").unwrap(),
    };
    projects::add(db, &owner.user, new).await.unwrap().id
}

pub async fn organization_exists(db: &Db, name: &str) -> bool {
    let name = Name::parse(name).unwrap();
    accounts::find_organization_id(db, &name)
        .await
        .unwrap()
        .is_some()
}
