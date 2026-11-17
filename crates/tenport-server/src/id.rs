const LENGTH: usize = 6;
const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{what} looks like {prefix}k3x9qa: six lower-case letters or digits")]
pub struct IdError {
    what: &'static str,
    prefix: &'static str,
}

macro_rules! short_id {
    ($name:ident, $prefix:literal, $what:literal) => {
        #[derive(
            Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, sqlx::Type,
        )]
        #[serde(try_from = "String", into = "String")]
        #[sqlx(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn random() -> Self {
                Self(format!("{}{}", $prefix, $crate::id::random_chars()))
            }

            pub fn parse(raw: &str) -> Result<Self, $crate::id::IdError> {
                if $crate::id::is_short_id(raw, $prefix) {
                    Ok(Self(raw.to_owned()))
                } else {
                    Err($crate::id::IdError::new($what, $prefix))
                }
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        $crate::name::parsed_text!($name, $crate::id::IdError);
    };
}

pub(crate) use short_id;

pub fn random_chars() -> String {
    (0..LENGTH)
        .map(|_| char::from(ALPHABET[rand::random_range(0..ALPHABET.len())]))
        .collect()
}

impl IdError {
    pub fn new(what: &'static str, prefix: &'static str) -> Self {
        Self { what, prefix }
    }
}

pub fn is_short_id(raw: &str, prefix: &str) -> bool {
    raw.strip_prefix(prefix).is_some_and(|chars| {
        chars.len() == LENGTH && chars.bytes().all(|byte| ALPHABET.contains(&byte))
    })
}
