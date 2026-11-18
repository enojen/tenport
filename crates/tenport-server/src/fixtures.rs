use crate::accounts::{self, GithubLogin, NewUser, Registered, Registration, Role};
use crate::db::Db;
use crate::events::Actor;
use crate::name::Name;
use crate::projects::{self, NewProject, ProjectId, Repo};

pub async fn register(db: &Db, organization: &str, github: &str, role: Role) -> Registered {
    let registration = Registration {
        organization: Name::parse(organization).unwrap(),
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

pub async fn project_of(db: &Db, owner: &Registered) -> ProjectId {
    let new = NewProject {
        organization: owner.organization.clone(),
        name: Name::parse("Tenport").unwrap(),
        repo: Repo::parse("enojen/tenport").unwrap(),
    };
    projects::add(db, &owner.user, new).await.unwrap().id
}
