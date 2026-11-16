use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests;

const MAX_LENGTH: usize = 200;

macro_rules! parsed_text {
    ($name:ident, $error:ident) => {
        impl TryFrom<String> for $name {
            type Error = $error;

            fn try_from(raw: String) -> Result<Self, $error> {
                Self::parse(&raw)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

pub(crate) use parsed_text;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[serde(try_from = "String", into = "String")]
#[sqlx(transparent)]
pub struct Name(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NameError {
    #[error("a name cannot be empty")]
    Empty,
    #[error("a name has at most {MAX_LENGTH} characters")]
    TooLong,
}

impl Name {
    pub fn parse(raw: &str) -> Result<Self, NameError> {
        let name = raw.trim();
        if name.is_empty() {
            return Err(NameError::Empty);
        }
        if name.chars().count() > MAX_LENGTH {
            return Err(NameError::TooLong);
        }
        Ok(Self(name.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

parsed_text!(Name, NameError);
