mod model;
mod store;

#[cfg(test)]
mod tests;

use sqlx::SqliteConnection;

use crate::db::Db;
use crate::events::{self, Actor, Event, EventsError, Stamp};
use crate::name::Name;

pub use model::{
    AccountsEvent, Email, EmailError, GithubLogin, GithubLoginError, MemberAdded, Membership,
    OrgId, OrganizationAdded, Role, Token, TokenAdded, TokenError, TokenHash, TokenId, User,
    UserAdded, UserId,
};

const UNIQUE_GITHUB: &str = "users.github";

#[derive(Debug, thiserror::Error)]
pub enum AccountsError {
    #[error("a user with the GitHub login {github} exists")]
    UserExists { github: GithubLogin },
    #[error(transparent)]
    Events(#[from] EventsError),
    #[error("cannot read accounts: {0}")]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, Clone)]
pub struct NewUser {
    pub name: Name,
    pub email: Option<Email>,
    pub github: GithubLogin,
}

#[derive(Debug, Clone)]
pub struct Registration {
    pub org: Name,
    pub user: NewUser,
    pub role: Role,
    pub token_name: Name,
}

#[derive(Debug)]
pub struct Registered {
    pub org: OrgId,
    pub user: UserId,
    pub token: Token,
}

pub async fn register(
    db: &Db,
    actor: Actor,
    registration: Registration,
) -> Result<Registered, AccountsError> {
    let mut events = Vec::new();
    let org = find_or_new_organization(db, &registration.org, &mut events).await?;
    let user = UserId::random();
    let token = Token::random();
    events.push(user_added(&user, &registration.user));
    events.push(Event::Accounts(AccountsEvent::MemberAdded(MemberAdded {
        org: org.clone(),
        user: user.clone(),
        role: registration.role,
    })));
    events.push(Event::Accounts(AccountsEvent::TokenAdded(TokenAdded {
        id: TokenId::random(),
        user: user.clone(),
        name: registration.token_name,
        hash: token.hash(),
    })));
    events::append_all(db, actor, events)
        .await
        .map_err(|error| registration_error(error, &registration.user.github))?;
    Ok(Registered { org, user, token })
}

pub async fn authenticate(db: &Db, token: &Token) -> Result<Option<User>, AccountsError> {
    let mut conn = db.acquire().await?;
    Ok(store::find_user_by_token_hash(&mut conn, &token.hash()).await?)
}

pub async fn memberships(db: &Db, user: &UserId) -> Result<Vec<Membership>, AccountsError> {
    let mut conn = db.acquire().await?;
    Ok(store::list_memberships_of_user(&mut conn, user).await?)
}

pub(crate) async fn apply(
    conn: &mut SqliteConnection,
    event: &AccountsEvent,
    stamp: &Stamp<'_>,
) -> Result<(), sqlx::Error> {
    match event {
        AccountsEvent::OrganizationAdded(added) => {
            store::insert_organization(conn, added, stamp).await
        }
        AccountsEvent::UserAdded(added) => store::insert_user(conn, added, stamp).await,
        AccountsEvent::MemberAdded(added) => store::insert_membership(conn, added, stamp).await,
        AccountsEvent::TokenAdded(added) => store::insert_token(conn, added, stamp).await,
    }
}

async fn find_or_new_organization(
    db: &Db,
    name: &Name,
    events: &mut Vec<Event>,
) -> Result<OrgId, AccountsError> {
    let mut conn = db.acquire().await?;
    if let Some(id) = store::find_organization_id_by_name(&mut conn, name).await? {
        return Ok(id);
    }
    let id = OrgId::random();
    events.push(Event::Accounts(AccountsEvent::OrganizationAdded(
        OrganizationAdded {
            id: id.clone(),
            name: name.clone(),
        },
    )));
    Ok(id)
}

fn user_added(id: &UserId, user: &NewUser) -> Event {
    Event::Accounts(AccountsEvent::UserAdded(UserAdded {
        id: id.clone(),
        name: user.name.clone(),
        email: user.email.clone(),
        github: user.github.clone(),
    }))
}

fn registration_error(error: EventsError, github: &GithubLogin) -> AccountsError {
    if error.violates_unique(UNIQUE_GITHUB) {
        AccountsError::UserExists {
            github: github.clone(),
        }
    } else {
        AccountsError::Events(error)
    }
}

#[cfg(test)]
pub(crate) async fn organization_id(db: &Db, name: &str) -> Option<OrgId> {
    let mut conn = db.acquire().await.unwrap();
    let name = Name::parse(name).unwrap();
    store::find_organization_id_by_name(&mut conn, &name)
        .await
        .unwrap()
}
