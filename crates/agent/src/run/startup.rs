//! Fonctions d'amorçage de l'agent : tampons disque et canal chiffré initial.

use super::*;

/// Répertoire des tampons disque, à côté du secret de liaison : le seul
/// endroit où l'agent sait pouvoir écrire, et celui qu'on monte en volume pour
/// un agent en conteneur.
pub(super) fn spool_dir(secret_path: &std::path::Path) -> std::path::PathBuf {
    secret_path.with_file_name("spool")
}

/// Borne du tampon disque, en octets.
pub(super) fn disk_limit(megabytes: u64) -> u64 {
    megabytes.saturating_mul(1024 * 1024)
}

/// Tampon disque des mesures de la machine. Impossible à ouvrir (droits,
/// disque en lecture seule) : l'agent continue avec la mémoire seule.
pub(super) fn open_sample_spool(config: &Config) -> Option<Spool> {
    if config.buffer_max_mb == 0 {
        return None;
    }
    let dir = spool_dir(&config.secret_path).join("samples");
    match Spool::open(&dir, disk_limit(config.buffer_max_mb)) {
        Ok(spool) => Some(spool),
        Err(error) => {
            warn!(
                path = %dir.display(),
                %error,
                "cannot open the disk catch-up buffer: measurements are kept in memory only"
            );
            None
        }
    }
}

/// Canal chiffré au démarrage : l'état rangé, confronté à la configuration.
///
/// Une empreinte donnée par la commande d'installation qui ne figure pas dans
/// l'état rangé, c'est une réinstallation vers un autre serveur (ou une autre
/// autorité) : on repart d'elle.
pub(super) fn initial_tls(config: &Config, stored: Option<TlsState>) -> Option<TlsState> {
    let configured_url = || {
        config.tls_url.clone().or_else(|| {
            dumbmonit_proto::agent_channel_url(
                &config.server_url,
                dumbmonit_proto::DEFAULT_AGENT_TLS_PORT,
            )
        })
    };
    match (stored, &config.ca_pin) {
        (Some(mut state), Some(pin)) if state.pins.contains(pin) => {
            if let Some(url) = config.tls_url.clone() {
                state.url = url;
            }
            Some(state)
        }
        (_, Some(pin)) => Some(TlsState::new(configured_url()?, vec![pin.clone()])),
        (Some(mut state), None) => {
            if let Some(url) = config.tls_url.clone() {
                state.url = url;
            }
            Some(state)
        }
        (None, None) => None,
    }
}

/// `server_url` en clair hors boucle locale (agent installé avant le tout-
/// HTTPS) : l'agent ne démarre que s'il a déjà un canal chiffré enrôlé
/// (certificat client et URL `https://` rangés), et ne parle plus qu'à lui —
/// ni jeton ni mesure ne partent en clair. Sinon, une erreur explicite plutôt
/// qu'un repli sur le clair.
pub(super) fn plain_url_channel(server_url: &str, tls: Option<&mut TlsState>) -> Result<()> {
    let Some(state) =
        tls.filter(|state| state.identity().is_some() && state.url.starts_with("https://"))
    else {
        anyhow::bail!(
            "the server URL '{server_url}' is plain http and no enrolled encrypted channel is \
             stored: run the installer again with the https:// command shown by DumbMonit"
        );
    };
    // Plus jamais l'ancien canal, même pour se réenrôler.
    state.locked = true;
    warn!(
        server_url,
        channel = %state.url,
        "server_url is plain http; using the stored encrypted channel only, re-run the installer \
         to switch to https"
    );
    Ok(())
}
