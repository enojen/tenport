use super::*;

fn config(pairs: &[(&str, &str)]) -> Result<Config, ConfigError> {
    Config::from_lookup(|name| {
        pairs
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| (*value).to_owned())
    })
}

#[test]
fn listens_on_localhost_when_no_address_is_set() {
    let config = config(&[(DATABASE, "/data/tenport.db")]).unwrap();
    assert_eq!(config.listen, "127.0.0.1:8080".parse().unwrap());
    assert_eq!(config.database, PathBuf::from("/data/tenport.db"));
}

#[test]
fn reads_the_address_that_is_set() {
    let config = config(&[(LISTEN, "0.0.0.0:3000"), (DATABASE, "tenport.db")]).unwrap();
    assert_eq!(config.listen, "0.0.0.0:3000".parse().unwrap());
}

#[test]
fn refuses_a_missing_database() {
    assert_eq!(config(&[]), Err(ConfigError::Missing { name: DATABASE }));
}

#[test]
fn refuses_an_address_without_a_port() {
    assert_eq!(
        config(&[(LISTEN, "0.0.0.0"), (DATABASE, "tenport.db")]),
        Err(ConfigError::BadAddress {
            name: LISTEN,
            value: "0.0.0.0".to_owned()
        })
    );
}
