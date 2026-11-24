//! Lecture des variables d'environnement `DUMBMONIT_*`.

/// Lit `name` dans l'environnement du processus. Une valeur vide est traitée
/// comme absente, pour que `DUMBMONIT_X=` dans un fichier compose ne masque
/// pas une valeur par défaut.
pub fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.trim().is_empty())
}
