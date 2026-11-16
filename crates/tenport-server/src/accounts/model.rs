use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use utoipa::ToSchema;

use crate::id::short_id;
use crate::name::{Name, parsed_text};

const TOKEN_PREFIX: &str = "tbx_";
const TOKEN_BYTES: usize = 32;
const GITHUB_LOGIN_MAX: usize = 39;

short_id!(OrgId, "o-");
short_id!(UserId, "u-");
short_id!(TokenId, "t-");

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[serde(try_from = "String", into = "String")]
#[sqlx(transparent)]
pub struct GithubLogin(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a GitHub login has 1 to {GITHUB_LOGIN_MAX} letters, digits and single hyphens inside")]
pub struct GithubLoginError;

impl GithubLogin {
    pub fn parse(raw: &str) -> Result<Self, GithubLoginError> {
        let login = raw.trim().to_ascii_lowercase();
        if is_github_login(&login) {
            Ok(Self(login))
        } else {
            Err(GithubLoginError)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn is_github_login(login: &str) -> bool {
    (1..=GITHUB_LOGIN_MAX).contains(&login.len())
        && login.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        && !login.starts_with('-')
        && !login.ends_with('-')
        && !login.contains("--")
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[serde(try_from = "String", into = "String")]
#[sqlx(transparent)]
pub struct Email(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("an email has one @ with text on both sides")]
pub struct EmailError;

impl Email {
    pub fn parse(raw: &str) -> Result<Self, EmailError> {
        let email = raw.trim();
        match email.split_once('@') {
            Some((local, domain))
                if !local.is_empty() && !domain.is_empty() && !domain.contains('@') =>
            {
                Ok(Self(email.to_owned()))
            }
            _ => Err(EmailError),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

parsed_text!(GithubLogin, GithubLoginError);
parsed_text!(Email, EmailError);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, ToSchema)]
#[serde(rename_all = "lowercase")]
#[sqlx(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Member,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Member => "member",
        }
    }
}

pub struct Token(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a token is {TOKEN_PREFIX} and {} lowercase hex characters", TOKEN_BYTES * 2)]
pub struct TokenError;

impl Token {
    pub fn random() -> Self {
        let bytes: [u8; TOKEN_BYTES] = rand::random();
        Self(format!("{TOKEN_PREFIX}{}", hex::encode(bytes)))
    }

    pub fn parse(raw: &str) -> Result<Self, TokenError> {
        let hex_part = raw.strip_prefix(TOKEN_PREFIX).ok_or(TokenError)?;
        let well_formed = hex_part.len() == TOKEN_BYTES * 2
            && hex_part
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase());
        if well_formed {
            Ok(Self(raw.to_owned()))
        } else {
            Err(TokenError)
        }
    }

    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn hash(&self) -> TokenHash {
        TokenHash(hex::encode(Sha256::digest(self.0.as_bytes())))
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(..)")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(transparent)]
#[sqlx(transparent)]
pub struct TokenHash(String);

impl TokenHash {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub id: UserId,
    pub name: Name,
    pub email: Option<Email>,
    pub github: GithubLogin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Membership {
    pub org: OrgId,
    pub org_name: Name,
    pub role: Role,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AccountsEvent {
    #[serde(rename = "organization.added")]
    OrganizationAdded(OrganizationAdded),
    #[serde(rename = "user.added")]
    UserAdded(UserAdded),
    #[serde(rename = "member.added")]
    MemberAdded(MemberAdded),
    #[serde(rename = "token.added")]
    TokenAdded(TokenAdded),
}

impl AccountsEvent {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::OrganizationAdded(_) => "organization.added",
            Self::UserAdded(_) => "user.added",
            Self::MemberAdded(_) => "member.added",
            Self::TokenAdded(_) => "token.added",
        }
    }

    pub fn subject(&self) -> String {
        match self {
            Self::OrganizationAdded(added) => format!("organization:{}", added.id),
            Self::UserAdded(added) => format!("user:{}", added.id),
            Self::MemberAdded(added) => format!("user:{}", added.user),
            Self::TokenAdded(added) => format!("user:{}", added.user),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationAdded {
    pub id: OrgId,
    pub name: Name,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserAdded {
    pub id: UserId,
    pub name: Name,
    pub email: Option<Email>,
    pub github: GithubLogin,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberAdded {
    pub org: OrgId,
    pub user: UserId,
    pub role: Role,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenAdded {
    pub id: TokenId,
    pub user: UserId,
    pub name: Name,
    pub hash: TokenHash,
}
