use std::collections::HashMap;

use crate::accounts::{
    self, AccountsError, Email, GithubLogin, NewUser, OrgId, Registration, Role, UserId,
};
use crate::db::Db;
use crate::events::Actor;
use crate::name::Name;

#[cfg(test)]
mod tests;

pub const USAGE: &str = "usage: tenport
       tenport user add --name <name> --github <login> --org <org> [--email <email>] [--role admin|member]";

const TOKEN_NAME: &str = "user add";

#[derive(Debug, Clone)]
pub enum Command {
    Serve,
    UserAdd(Registration),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CommandError {
    #[error("unknown command: {0}")]
    Unknown(String),
    #[error("{flag} needs a value")]
    NoValue { flag: String },
    #[error("unknown flag: {0}")]
    UnknownFlag(String),
    #[error("{0} is given twice")]
    Twice(String),
    #[error("user add needs {0}")]
    Missing(&'static str),
    #[error("{flag}: {reason}")]
    BadValue { flag: &'static str, reason: String },
}

pub fn parse(args: &[String]) -> Result<Command, CommandError> {
    match args {
        [] => Ok(Command::Serve),
        [user, add, flags @ ..] if user == "user" && add == "add" => user_add(flags),
        _ => Err(CommandError::Unknown(args.join(" "))),
    }
}

fn user_add(args: &[String]) -> Result<Command, CommandError> {
    let mut flags = Flags::read(args, &["--name", "--github", "--org", "--email", "--role"])?;
    let name = flags.required("--name", Name::parse)?;
    let github = flags.required("--github", GithubLogin::parse)?;
    let org = flags.required("--org", Name::parse)?;
    let email = flags.optional("--email", Email::parse)?;
    let role = flags.optional("--role", role)?.unwrap_or(Role::Member);
    Ok(Command::UserAdd(Registration {
        org,
        user: NewUser {
            name,
            email,
            github,
        },
        role,
        token_name: Name::parse(TOKEN_NAME).expect("the token name is a name"),
    }))
}

fn role(raw: &str) -> Result<Role, &'static str> {
    match raw {
        "admin" => Ok(Role::Admin),
        "member" => Ok(Role::Member),
        _ => Err("admin or member"),
    }
}

struct Flags<'a>(HashMap<&'static str, &'a str>);

impl<'a> Flags<'a> {
    fn read(args: &'a [String], known: &[&'static str]) -> Result<Self, CommandError> {
        let mut values = HashMap::new();
        let mut rest = args.iter();
        while let Some(flag) = rest.next() {
            let known = known
                .iter()
                .find(|name| **name == flag)
                .ok_or_else(|| CommandError::UnknownFlag(flag.clone()))?;
            let value = rest
                .next()
                .ok_or_else(|| CommandError::NoValue { flag: flag.clone() })?;
            if values.insert(*known, value.as_str()).is_some() {
                return Err(CommandError::Twice(flag.clone()));
            }
        }
        Ok(Self(values))
    }

    fn optional<T, E: ToString>(
        &mut self,
        flag: &'static str,
        parse: impl Fn(&str) -> Result<T, E>,
    ) -> Result<Option<T>, CommandError> {
        self.0
            .remove(flag)
            .map(|raw| {
                parse(raw).map_err(|error| CommandError::BadValue {
                    flag,
                    reason: error.to_string(),
                })
            })
            .transpose()
    }

    fn required<T, E: ToString>(
        &mut self,
        flag: &'static str,
        parse: impl Fn(&str) -> Result<T, E>,
    ) -> Result<T, CommandError> {
        self.optional(flag, parse)?
            .ok_or(CommandError::Missing(flag))
    }
}

#[derive(Debug, serde::Serialize)]
pub struct AddedUser {
    pub user: UserId,
    pub organization: OrgId,
    pub role: Role,
    pub token: String,
}

pub async fn add_user(db: &Db, registration: Registration) -> Result<AddedUser, AccountsError> {
    let role = registration.role;
    let added = accounts::register(db, Actor::Server, registration).await?;
    Ok(AddedUser {
        user: added.user,
        organization: added.org,
        role,
        token: added.token.expose().to_owned(),
    })
}
