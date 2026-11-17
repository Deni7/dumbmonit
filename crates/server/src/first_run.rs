//! Chronométrage du premier lancement.
//!
//! L'objectif produit est chiffré : de `docker compose up -d` à la première
//! alerte reçue, moins de cinq minutes. Pour le vérifier sur une vraie
//! installation plutôt que de le supposer, l'instance note l'heure de chaque
//! étape franchie pour la première fois :
//!
//! 1. premier démarrage sans compte (`first_boot_at`) ;
//! 2. création du premier administrateur (`setup_at`) ;
//! 3. premier moniteur (`first_target_at`) ;
//! 4. premier canal de notification (`first_channel_at`) ;
//! 5. premier message réellement parti, test compris (`first_notification_at`) ;
//! 6. première alerte réelle livrée par le moteur (`first_alert_at`).
//!
//! Rien ne sort de l'instance : les horodatages restent dans la table
//! `settings` et ne se lisent que par `GET /api/onboarding`, affichés dans
//! Réglages → À propos.
//!
//! Une ligne par étape (`first_run.<étape>`), écrite par `INSERT OR IGNORE` :
//! la première valeur gagne, sans verrou ni relecture, même si deux requêtes
//! arrivent ensemble. Les étapes 3 à 6 ne sont notées que si la création du
//! compte l'a été : sur une instance mise à jour depuis une version qui ne
//! chronométrait pas, le « premier moniteur » ajouté demain ne mesurerait rien.
//!
//! Ces repères décrivent une installation, pas un réglage : ils n'entrent pas
//! dans la sauvegarde, et une restauration sur une instance neuve ne les
//! réécrit pas.

use serde::Serialize;
use sqlx::{Row, SqlitePool};

/// Préfixe des clés dans `settings`.
const PREFIX: &str = "first_run.";

/// Une étape du premier lancement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Milestone {
    FirstBoot,
    Setup,
    FirstTarget,
    FirstChannel,
    FirstNotification,
    FirstAlert,
}

impl Milestone {
    pub const ALL: [Milestone; 6] = [
        Milestone::FirstBoot,
        Milestone::Setup,
        Milestone::FirstTarget,
        Milestone::FirstChannel,
        Milestone::FirstNotification,
        Milestone::FirstAlert,
    ];

    /// Nom du champ, dans la réponse de l'API comme dans la clé stockée.
    pub fn field(self) -> &'static str {
        match self {
            Self::FirstBoot => "first_boot_at",
            Self::Setup => "setup_at",
            Self::FirstTarget => "first_target_at",
            Self::FirstChannel => "first_channel_at",
            Self::FirstNotification => "first_notification_at",
            Self::FirstAlert => "first_alert_at",
        }
    }

    fn key(self) -> String {
        format!("{PREFIX}{}", self.field())
    }

    /// Les deux premières étapes ouvrent la mesure ; les suivantes n'ont de sens
    /// que si elle a été ouverte.
    fn needs_setup(self) -> bool {
        !matches!(self, Self::FirstBoot | Self::Setup)
    }
}

/// Les horodatages connus, UTC sans suffixe (`crate::db::timestamp`).
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct Timings {
    pub first_boot_at: Option<String>,
    pub setup_at: Option<String>,
    pub first_target_at: Option<String>,
    pub first_channel_at: Option<String>,
    pub first_notification_at: Option<String>,
    pub first_alert_at: Option<String>,
}

impl Timings {
    fn slot(&mut self, milestone: Milestone) -> &mut Option<String> {
        match milestone {
            Milestone::FirstBoot => &mut self.first_boot_at,
            Milestone::Setup => &mut self.setup_at,
            Milestone::FirstTarget => &mut self.first_target_at,
            Milestone::FirstChannel => &mut self.first_channel_at,
            Milestone::FirstNotification => &mut self.first_notification_at,
            Milestone::FirstAlert => &mut self.first_alert_at,
        }
    }
}

/// Note l'étape si elle ne l'a jamais été. Ne fait jamais échouer l'appelant :
/// une mesure manquée ne vaut pas une création de moniteur refusée.
pub async fn record(pool: &SqlitePool, milestone: Milestone) {
    if let Err(error) = try_record(pool, milestone, &crate::db::now()).await {
        tracing::debug!(?error, step = milestone.field(), "first-run step not recorded");
    }
}

async fn try_record(pool: &SqlitePool, milestone: Milestone, at: &str) -> sqlx::Result<()> {
    // Même encodage que `auth::settings::set` : une chaîne JSON, en octets.
    let value = serde_json::to_vec(at).unwrap_or_default();
    if milestone.needs_setup() {
        sqlx::query(
            "INSERT OR IGNORE INTO settings (key, value)
             SELECT ?, ? WHERE EXISTS (SELECT 1 FROM settings WHERE key = ?)",
        )
        .bind(milestone.key())
        .bind(value)
        .bind(Milestone::Setup.key())
        .execute(pool)
        .await?;
    } else {
        sqlx::query("INSERT OR IGNORE INTO settings (key, value) VALUES (?, ?)")
            .bind(milestone.key())
            .bind(value)
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Lit les étapes franchies. Une valeur illisible compte comme absente.
pub async fn load(pool: &SqlitePool) -> sqlx::Result<Timings> {
    let rows = sqlx::query("SELECT key, value FROM settings WHERE key LIKE 'first_run.%'")
        .fetch_all(pool)
        .await?;
    let mut timings = Timings::default();
    for row in rows {
        let key: String = row.try_get("key")?;
        let value: Vec<u8> = row.try_get("value")?;
        let Some(milestone) = Milestone::ALL.into_iter().find(|m| m.key() == key) else {
            continue;
        };
        *timings.slot(milestone) = serde_json::from_slice::<String>(&value).ok();
    }
    Ok(timings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_milestone_has_its_own_key() {
        let keys: std::collections::HashSet<_> = Milestone::ALL.iter().map(|m| m.key()).collect();
        assert_eq!(keys.len(), Milestone::ALL.len());
        assert_eq!(Milestone::FirstAlert.key(), "first_run.first_alert_at");
    }

    #[test]
    fn only_the_boot_and_the_setup_open_the_measure() {
        assert!(!Milestone::FirstBoot.needs_setup());
        assert!(!Milestone::Setup.needs_setup());
        assert!(Milestone::FirstTarget.needs_setup());
        assert!(Milestone::FirstAlert.needs_setup());
    }

    #[tokio::test]
    async fn the_first_value_wins_and_nothing_is_measured_before_the_setup() {
        let dir = tempfile::tempdir().expect("répertoire temporaire");
        let pool = crate::db::open(&dir.path().join("t.db")).await.expect("base");

        // Instance mise à jour : pas de création de compte notée, pas de mesure.
        try_record(&pool, Milestone::FirstTarget, "2026-11-16 10:00:00").await.unwrap();
        assert_eq!(load(&pool).await.unwrap(), Timings::default());

        try_record(&pool, Milestone::Setup, "2026-11-16 10:01:00").await.unwrap();
        try_record(&pool, Milestone::FirstTarget, "2026-11-16 10:02:00").await.unwrap();
        try_record(&pool, Milestone::FirstTarget, "2026-11-16 10:09:00").await.unwrap();
        try_record(&pool, Milestone::Setup, "2026-11-16 10:09:00").await.unwrap();

        let timings = load(&pool).await.unwrap();
        assert_eq!(timings.setup_at.as_deref(), Some("2026-11-16 10:01:00"));
        assert_eq!(timings.first_target_at.as_deref(), Some("2026-11-16 10:02:00"));
        assert_eq!(timings.first_channel_at, None);
    }
}
