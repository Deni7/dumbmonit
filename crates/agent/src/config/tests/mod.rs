//! Tests de la configuration de l'agent : jeux d'essai partagés.

use super::*;

mod channel;
mod collection;
mod sections;
mod validation;

fn env(pairs: &[(&str, &str)]) -> EnvSource {
    EnvSource::from_map(pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect())
}

fn file_with_url_and_token() -> FileConfig {
    FileConfig {
        server_url: Some("https://serveur:8080/".into()),
        token: Some("dmon_abc".into()),
        ..FileConfig::default()
    }
}

fn from_yaml(yaml: &str) -> FileConfig {
    FileConfig {
        server_url: Some("https://s:8080".into()),
        token: Some("dmon_abc".into()),
        ..dumbmonit_collectors::yaml::from_str(yaml).expect("YAML valide")
    }
}

fn with_yaml(yaml: &str) -> FileConfig {
    FileConfig {
        server_url: Some("https://s:8080".into()),
        token: Some("dmon_abc".into()),
        ..dumbmonit_collectors::yaml::from_str(yaml).expect("YAML valide")
    }
}
