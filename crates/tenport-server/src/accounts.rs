mod model;
mod store;

#[cfg(test)]
mod tests;

use sqlx::SqliteConnection;

use crate::db::Db;
use crate::events::{self, Actor, ApplyError, Event, EventsError, Stamp};
use crate::name::Name;

pub use model::{
    AccountsEvent, Email, EmailError, GithubLogin, GithubLoginError, MemberAdded, Membership,
    OrganizationAdded, OrganizationId, Role, Token, TokenAdded, TokenError, TokenHash, TokenId,
    User, UserAdded, UserId,
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

#[derive(Debug)]
pub struct NewUser {
    pub organization_name: Name,
    pub name: Name,
    pub email: Option<Email>,
    pub github: GithubLogin,
    pub role: Role,
    pub token_name: Name,
}

#[derive(Debug, serde::Serialize)]
pub struct AddedUser {
    pub user: UserId,
    pub organization: OrganizationId,
    pub role: Role,
    #[serde(serialize_with = "expose_token")]
    pub token: Token,
}

pub async fn add_user(db: &Db, actor: Actor, new: NewUser) -> Result<AddedUser, AccountsError> {
    let NewUser {
        organization_name,
        name,
        email,
        github,
        role,
        token_name,
    } = new;
    let mut events = Vec::new();
    let organization = match find_organization_id(db, &organization_name).await? {
        Some(id) => id,
        None => add_organization(&mut events, organization_name),
    };
    let user = UserId::random();
    let token = Token::random();
    events.push(Event::Accounts(AccountsEvent::UserAdded(UserAdded {
        id: user.clone(),
        name,
        email,
        github: github.clone(),
    })));
    events.push(Event::Accounts(AccountsEvent::MemberAdded(MemberAdded {
        organization: organization.clone(),
        user: user.clone(),
        role,
    })));
    events.push(Event::Accounts(AccountsEvent::TokenAdded(TokenAdded {
        id: TokenId::random(),
        user: user.clone(),
        name: token_name,
        hash: token.hash(),
    })));
    events::append_all(db, actor, events)
        .await
        .map_err(|error| add_user_error(error, github))?;
    Ok(AddedUser {
        user,
        organization,
        role,
        token,
    })
}

pub async fn authenticate(db: &Db, token: &Token) -> Result<Option<User>, AccountsError> {
    let mut conn = db.acquire().await?;
    Ok(store::find_user_by_token_hash(&mut conn, &token.hash()).await?)
}

pub async fn memberships(db: &Db, user: &UserId) -> Result<Vec<Membership>, AccountsError> {
    let mut conn = db.acquire().await?;
    Ok(store::list_memberships_of_user(&mut conn, user).await?)
}

pub async fn find_role(
    db: &Db,
    organization: &OrganizationId,
    user: &UserId,
) -> Result<Option<Role>, AccountsError> {
    let mut conn = db.acquire().await?;
    Ok(store::find_role_of_user(&mut conn, organization, user).await?)
}

pub(crate) async fn apply(
    conn: &mut SqliteConnection,
    event: &AccountsEvent,
    stamp: &Stamp<'_>,
) -> Result<(), ApplyError> {
    match event {
        AccountsEvent::OrganizationAdded(added) => {
            store::insert_organization(conn, added, stamp).await?
        }
        AccountsEvent::UserAdded(added) => store::insert_user(conn, added, stamp).await?,
        AccountsEvent::MemberAdded(added) => store::insert_membership(conn, added, stamp).await?,
        AccountsEvent::TokenAdded(added) => store::insert_token(conn, added, stamp).await?,
    }
    Ok(())
}

async fn find_organization_id(
    db: &Db,
    name: &Name,
) -> Result<Option<OrganizationId>, AccountsError> {
    let mut conn = db.acquire().await?;
    Ok(store::find_organization_id_by_name(&mut conn, name).await?)
}

fn add_organization(events: &mut Vec<Event>, name: Name) -> OrganizationId {
    let id = OrganizationId::random();
    events.push(Event::Accounts(AccountsEvent::OrganizationAdded(
        OrganizationAdded {
            id: id.clone(),
            name,
        },
    )));
    id
}

fn add_user_error(error: EventsError, github: GithubLogin) -> AccountsError {
    if error.violates_unique(UNIQUE_GITHUB) {
        AccountsError::UserExists { github }
    } else {
        AccountsError::Events(error)
    }
}

fn expose_token<S: serde::Serializer>(token: &Token, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(token.expose())
}

#[cfg(test)]
pub(crate) async fn organization_id(db: &Db, name: &str) -> Option<OrganizationId> {
    let mut conn = db.acquire().await.unwrap();
    let name = Name::parse(name).unwrap();
    store::find_organization_id_by_name(&mut conn, &name)
        .await
        .unwrap()
}
