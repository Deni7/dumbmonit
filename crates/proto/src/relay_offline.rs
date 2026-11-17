//! Relais hors ligne : le planning des sondes que le relais garde pour lui, et
//! le rattrapage des résultats obtenus pendant une coupure du lien.
//!
//! En temps normal, c'est le serveur qui planifie : il dépose chaque sonde
//! échue dans l'attente longue du relais ([`crate::RELAY_PATH`]). Quand le lien
//! tombe, le relais ne reçoit plus rien ; avec une copie du planning
//! ([`RelayPlan`]), il continue de sonder seul, range les résultats dans un
//! tampon disque et les renvoie par lots ([`RelayBacklog`]) dès que le serveur
//! répond de nouveau — avec leurs horodatages d'origine.

use serde::{Deserialize, Serialize};

use crate::{ProbeOutcome, Target, TargetId};

/// `GET` : le planning des sondes de ce relais.
pub const RELAY_PLAN_PATH: &str = "/api/agent/relay/plan";

/// `POST` : un lot de résultats obtenus hors ligne.
pub const RELAY_BACKLOG_PATH: &str = "/api/agent/relay/backlog";

/// Résultats au plus par lot de rattrapage. Une sonde d'hyperviseur peut
/// porter des milliers d'échantillons : un lot reste ainsi sous la limite de
/// corps du canal des agents (16 Mo) dans tous les cas ordinaires.
pub const MAX_BACKLOG_RESULTS: usize = 200;

/// Planning des sondes confiées à un relais, tel que le serveur le lui remet.
///
/// Les cibles voyagent entières, identifiants compris, exactement comme dans
/// une sonde [`crate::ProbeJob`] : c'est le relais qui se présentera aux
/// équipements quand le serveur ne sera plus là pour le lui demander. Le
/// relais range ce planning chiffré sur son disque.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayPlan {
    pub probes: Vec<PlannedProbe>,
    /// Instant où le serveur l'a établi, en millisecondes.
    pub issued_at_ms: i64,
}

/// Une cible du planning : quoi sonder, à quel rythme (`target.interval`) et
/// avec quel délai.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedProbe {
    pub target: Target,
    /// Délai maximal de la sonde, en secondes, le même que sur le serveur.
    pub timeout_secs: u64,
}

/// Un lot de résultats obtenus pendant une coupure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayBacklog {
    /// Identité du tampon du relais, tirée au hasard à sa création. Les
    /// numéros de séquence ne valent que pour un même tampon : un relais
    /// réinstallé repart de 1, le serveur l'apprend par ce changement.
    pub spool_id: String,
    /// Résultats, par numéro de séquence croissant.
    pub results: Vec<BacklogResult>,
    /// Résultats perdus depuis la création du tampon (cumul) : tampon plein,
    /// ou enregistrement illisible après une coupure de courant.
    #[serde(default)]
    pub dropped: u64,
}

/// Un résultat de sonde mis de côté.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacklogResult {
    /// Numéro de séquence dans le tampon : c'est sur lui que le serveur
    /// écarte un lot renvoyé deux fois (accusé de réception perdu).
    pub seq: u64,
    pub target_id: TargetId,
    /// Début de la sonde, en millisecondes. Les échantillons portent leurs
    /// propres horodatages ; celui-ci sert au journal.
    pub started_at_ms: i64,
    pub outcome: ProbeOutcome,
}

/// Réponse du serveur à un lot de rattrapage.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BacklogAck {
    /// Plus grand numéro de séquence désormais connu du serveur : le relais
    /// peut oublier tout ce qui va jusque-là.
    pub acked_seq: u64,
    /// Résultats rangés.
    pub accepted: usize,
    /// Résultats déjà reçus (lot renvoyé), écartés.
    pub duplicates: usize,
    /// Résultats écartés : équipement supprimé, ou qui ne passe plus par ce
    /// relais.
    pub ignored: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_backlog_without_a_loss_counter_still_reads() {
        let backlog: RelayBacklog =
            serde_json::from_str(r#"{"spool_id":"ab","results":[]}"#).expect("lot");
        assert_eq!(backlog.dropped, 0);
        assert!(backlog.results.is_empty());
    }
}
