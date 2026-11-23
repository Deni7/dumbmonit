//! Mise à jour automatique des agents d'avant deux correctifs : ce que le
//! serveur peut encore faire pour eux, et ce qu'il dit quand il ne peut rien.
//!
//! - **Clé de publication.** La première clé (0.15.0) n'a jamais rien signé :
//!   sa moitié privée est perdue. Depuis 0.20.1, les binaires sont signés par
//!   une nouvelle clé, que les agents antérieurs ne connaissent pas : ils
//!   rejettent la signature (« does not match any release key »), et un agent
//!   d'avant 0.15.0 n'a aucune clé. Quand « Accepter les builds non signés »
//!   est actif, le serveur leur propose donc le binaire **sans** signature :
//!   l'agent ne l'accepte alors que par le canal chiffré, serveur authentifié
//!   par l'autorité épinglée — exactement ce que ce réglage permet déjà pour
//!   un build non signé. Un agent qui connaît la clé reçoit toujours la
//!   signature, et la vérifie : rien ne change pour lui. Réglage inactif, la
//!   machine n'est pas sollicitée et l'interface dit pourquoi
//!   ([`UNKNOWN_KEY`]).
//! - **Windows.** Jusqu'à 0.20.1 compris, l'agent synchronisait la copie de son
//!   binaire par un descripteur en lecture seule ; `FlushFileBuffers` exige
//!   l'écriture, d'où « Accès refusé (os error 5) » à chaque tentative, quelle
//!   que soit l'offre : l'étape est sur le seul chemin d'installation Windows.
//!   Ces agents ne peuvent pas se mettre à jour eux-mêmes ; rien ne leur est
//!   proposé et l'interface demande une réinstallation, une fois
//!   ([`REINSTALL`]).

use dumbmonit_proto::{AgentUpdateOffer, is_older};

use crate::api::agent_files::ShippedBinary;

use super::selfupdate::{Gate, HostState, Settings, Verdict, evaluate};

/// Première version d'agent qui connaît la clé de publication actuelle.
pub const FIRST_CURRENT_KEY_VERSION: &str = "0.20.1";

/// Première version d'agent Windows qui va au bout de sa mise à jour. Le `-0`
/// compte les pré-versions de 0.20.2 (`0.20.2-dev.1`), qui ont le correctif.
pub const FIRST_WINDOWS_UPDATE_VERSION: &str = "0.20.2-0";

/// État : agent Windows qui ne peut pas se remplacer, à réinstaller une fois.
pub const REINSTALL: &str = "reinstall";
/// État : agent qui ne connaît pas la clé de publication, réglage « builds non
/// signés » inactif.
pub const UNKNOWN_KEY: &str = "unknown_key";

/// Vrai si un agent de cette version vérifie la signature des binaires de ce
/// build. Une version illisible est réputée la connaître : la signature est
/// alors envoyée, et vérifiée.
pub fn knows_current_key(agent_version: &str) -> bool {
    !is_older(agent_version, FIRST_CURRENT_KEY_VERSION)
}

/// Vrai pour un agent Windows d'avant le correctif de la copie synchronisée.
pub fn windows_cannot_update(platform: Option<&str>, agent_version: &str) -> bool {
    platform.is_some_and(|platform| platform.starts_with("windows"))
        && is_older(agent_version, FIRST_WINDOWS_UPDATE_VERSION)
}

/// Pourquoi cette machine ne peut pas installer ce binaire, quoi qu'on lui
/// propose ; `None` quand rien ne l'en empêche.
pub fn stuck(
    host: &HostState,
    binary: &ShippedBinary,
    settings: &Settings,
) -> Option<&'static str> {
    if windows_cannot_update(host.platform.as_deref(), &host.agent_version) {
        return Some(REINSTALL);
    }
    if binary.signature.is_some()
        && !settings.allow_unsigned
        && !knows_current_key(&host.agent_version)
    {
        return Some(UNKNOWN_KEY);
    }
    None
}

/// États qu'une impossibilité de [`stuck`] remplace : ceux d'une machine qui
/// recevrait l'offre tôt ou tard. Les autres (à jour, désactivée, conteneur,
/// build non signé…) disent déjà l'essentiel.
fn superseded(status: &str) -> bool {
    matches!(
        status,
        "pending"
            | "updating"
            | "failed"
            | "refused"
            | "manual"
            | "waiting_window"
            | "waiting_canary"
            | "waiting_wave"
            | "paused"
            | "halted"
    )
}

/// [`evaluate`], plus les machines qui ne peuvent pas aller au bout : pas
/// d'offre (pas même « Update now »), et l'état qui dit quoi faire.
pub fn verdict(
    settings: &Settings,
    host: &HostState,
    binary: Option<&ShippedBinary>,
    server_version: &str,
    gate: &Gate,
    now_ms: i64,
) -> Verdict {
    let verdict = evaluate(settings, host, binary, server_version, gate, now_ms);
    match binary.and_then(|binary| stuck(host, binary, settings)) {
        Some(status) if superseded(verdict.status) => {
            Verdict { status, reason: None, offer: false, force: false, allow_downgrade: false }
        }
        _ => verdict,
    }
}

/// L'offre pour cette machine. La signature n'est retirée que pour un agent
/// qui ne connaît pas la clé, et seulement quand les builds non signés sont
/// acceptés.
pub fn offer(
    binary: &ShippedBinary,
    host: &HostState,
    settings: &Settings,
    verdict: &Verdict,
    version: &str,
    build: Option<&str>,
) -> AgentUpdateOffer {
    let unsigned = settings.allow_unsigned && !knows_current_key(&host.agent_version);
    AgentUpdateOffer {
        version: version.to_string(),
        build: build.map(str::to_string),
        file: binary.file.clone(),
        sha256: binary.sha256.clone(),
        size: Some(binary.size),
        signature: if unsigned { None } else { binary.signature.clone() },
        allow_unsigned: settings.allow_unsigned,
        allow_downgrade: verdict.allow_downgrade,
        force: verdict.force,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::collectors::agent::selfupdate::Outcome;

    const NOW: i64 = 1_800_000_000_000;
    const SERVER: &str = "0.20.2";

    fn binary(signed: bool) -> ShippedBinary {
        ShippedBinary {
            file: "dumbmonit-agent-linux-x86_64".into(),
            sha256: "b".repeat(64),
            size: 10,
            signature: if signed { Some("c2ln".to_string()) } else { None },
        }
    }

    fn host(platform: &str, version: &str) -> HostState {
        HostState {
            agent_version: version.into(),
            sha256: Some("a".repeat(64)),
            platform: Some(platform.into()),
            ability: Some("ready".into()),
            ..HostState::default()
        }
    }

    fn unsigned_on() -> Settings {
        Settings { allow_unsigned: true, ..Settings::default() }
    }

    fn judge(settings: &Settings, host: &HostState, signed: bool) -> Verdict {
        verdict(settings, host, Some(&binary(signed)), SERVER, &Gate::Open, NOW)
    }

    #[test]
    fn only_agents_from_0_20_1_know_the_current_key() {
        for old in ["0.14.3", "0.15.0", "0.19.0", "0.20.0", "0.20.0-dev.2", "0.20.1-rc.1"] {
            assert!(!knows_current_key(old), "{old}");
        }
        for new in ["0.20.1", "0.20.2", "0.21.0", "1.0.0"] {
            assert!(knows_current_key(new), "{new}");
        }
        // Illisible : la signature est envoyée, l'agent la vérifie.
        assert!(knows_current_key("dev"));
    }

    #[test]
    fn only_windows_agents_before_the_fix_cannot_update() {
        for old in ["0.14.3", "0.15.0", "0.20.0", "0.20.1"] {
            assert!(windows_cannot_update(Some("windows-x86_64"), old), "{old}");
            assert!(!windows_cannot_update(Some("linux-x86_64"), old), "{old}");
        }
        for new in ["0.20.2-dev.1", "0.20.2", "0.21.0"] {
            assert!(!windows_cannot_update(Some("windows-x86_64"), new), "{new}");
        }
        assert!(!windows_cannot_update(None, "0.14.3"));
        assert!(!windows_cannot_update(Some("windows-x86_64"), "dev"));
    }

    #[test]
    fn an_agent_that_knows_the_key_always_gets_the_signature() {
        for settings in [Settings::default(), unsigned_on()] {
            let agent = host("linux-x86_64", "0.20.1");
            let verdict = judge(&settings, &agent, true);
            assert!(verdict.offer, "{verdict:?}");
            let made = offer(&binary(true), &agent, &settings, &verdict, SERVER, None);
            assert_eq!(made.signature.as_deref(), Some("c2ln"));
            assert_eq!(made.allow_unsigned, settings.allow_unsigned);
        }
    }

    #[test]
    fn an_agent_before_the_key_is_offered_the_build_unsigned_only_when_allowed() {
        let agent = host("linux-x86_64", "0.15.0");
        // Réglage inactif : rien n'est proposé, l'état dit pourquoi.
        let held = judge(&Settings::default(), &agent, true);
        assert_eq!((held.status, held.offer), (UNKNOWN_KEY, false));
        // Même « Update now » : l'agent rejetterait la signature.
        let requested = HostState { requested_ms: Some(NOW - 1), ..agent.clone() };
        assert_eq!(judge(&Settings::default(), &requested, true).status, UNKNOWN_KEY);

        // Réglage actif : offre sans signature, l'agent l'installe par le
        // canal chiffré.
        let settings = unsigned_on();
        let verdict = judge(&settings, &agent, true);
        assert!(verdict.offer && verdict.status == "pending", "{verdict:?}");
        let made = offer(&binary(true), &agent, &settings, &verdict, SERVER, Some("abc"));
        assert_eq!(made.signature, None);
        assert!(made.allow_unsigned);
        assert_eq!(made.sha256, "b".repeat(64));
        assert_eq!(made.version, SERVER);
        assert_eq!(made.build.as_deref(), Some("abc"));
        // Le refus n'a de sens que pour un build signé : non signé, c'est
        // l'état « unsigned » habituel.
        assert_eq!(judge(&Settings::default(), &agent, false).status, "unsigned");
    }

    #[test]
    fn an_old_windows_agent_is_told_to_reinstall_and_offered_nothing() {
        let agent = host("windows-x86_64", "0.14.3");
        for settings in [Settings::default(), unsigned_on()] {
            let held = judge(&settings, &agent, true);
            assert_eq!((held.status, held.offer, held.force), (REINSTALL, false, false));
        }
        // Après l'échec rapporté, ou sur demande explicite : toujours rien.
        let failed = HostState {
            outcome: Some(Outcome {
                outcome: "failed".into(),
                sha256: "b".repeat(64),
                version: Some(SERVER.into()),
                reason: Some("syncing the copy: os error 5".into()),
                at_ms: NOW - 10,
                at: None,
            }),
            requested_ms: Some(NOW - 5),
            ..agent.clone()
        };
        assert_eq!(judge(&unsigned_on(), &failed, true).status, REINSTALL);
        // Déjà à jour (réinstallé à la main) ou mises à jour coupées : l'état
        // habituel.
        let current = HostState { sha256: Some("b".repeat(64)), ..agent.clone() };
        assert_eq!(judge(&Settings::default(), &current, true).status, "up_to_date");
        let off = HostState { mode: Some(false), ..agent };
        assert_eq!(judge(&Settings::default(), &off, true).status, "disabled");
        // Un agent Windows d'après le correctif suit le cas général.
        let fixed = host("windows-x86_64", "0.20.2");
        assert!(judge(&Settings::default(), &fixed, true).offer);
    }
}
