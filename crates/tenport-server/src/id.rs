const LENGTH: usize = 6;
const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";

macro_rules! short_id {
    ($name:ident, $prefix:literal) => {
        #[derive(
            Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, sqlx::Type,
        )]
        #[serde(transparent)]
        #[sqlx(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn random() -> Self {
                Self(format!("{}{}", $prefix, $crate::id::random_chars()))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

pub(crate) use short_id;

pub fn random_chars() -> String {
    (0..LENGTH)
        .map(|_| char::from(ALPHABET[rand::random_range(0..ALPHABET.len())]))
        .collect()
}
