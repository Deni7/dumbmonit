//! Rôles personnalisés : le catalogue fixe des permissions, et les routes que
//! chacune ouvre.
//!
//! Un administrateur compose un rôle ([`super::custom_roles`]) en cochant des
//! permissions de ce catalogue ; il n'en invente pas. Chaque permission est une
//! liste fermée de routes (méthode et gabarit sans `/api`, `*` = un segment non
//! vide, comme [`super::middleware::OPERATOR_ROUTES`]). Le garde d'API
//! ([`super::middleware::require_session`]) ne laisse passer une écriture d'un
//! compte à rôle personnalisé que si l'une de ses permissions la cite : refus
//! par défaut, une route ajoutée demain reste à l'administrateur tant qu'on ne
//! l'a pas inscrite ici. Il dépose alors [`Delegated`] dans la requête, ce qui
//! fait accepter le compte aux extracteurs `AdminIdentity` et
//! `OperatorIdentity` — pour cette route-là seulement.
//!
//! Un compte à rôle personnalisé est, pour tout le reste, un lecteur : son rôle
//! livré reste `viewer`, et les vérifications « est-ce un administrateur ? »
//! des gestionnaires (secrets, comptes, sauvegardes, SSO) lui répondent non.
//! Ne se délèguent jamais : comptes, rôles, équipes, jetons, SSO, sauvegardes,
//! réglages de l'instance, secrets (serveur de courriel, jetons d'agent),
//! actions sur les machines (agents, conteneurs, remédiation).

use axum::http::Method;
use serde::{Deserialize, Serialize};

use crate::auth::middleware::OPERATOR_ROUTES;
use crate::auth::teams::{self, Level};
use crate::auth::users::{Role, User};

/// Une permission du catalogue. Sérialisée par son nom (`monitors.manage`),
/// tel qu'il est stocké et exposé.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Permission {
    /// Lire tout ce qu'un lecteur lit. Toujours compris dans un rôle personnalisé.
    #[serde(rename = "view")]
    View,
    /// Traiter les alertes : ce que fait aujourd'hui un opérateur.
    #[serde(rename = "alerts.operate")]
    AlertsOperate,
    /// Créer, modifier, supprimer les moniteurs, leurs modèles et leurs règles.
    #[serde(rename = "monitors.manage")]
    MonitorsManage,
    /// Canaux, politique de notification, modèles de message.
    #[serde(rename = "notifications.manage")]
    NotificationsManage,
    /// Pages de statut et incidents publiés.
    #[serde(rename = "status_pages.manage")]
    StatusPagesManage,
    /// Fenêtres de maintenance (silences).
    #[serde(rename = "maintenance.manage")]
    MaintenanceManage,
    /// Lire les rapports périodiques et leurs PDF.
    #[serde(rename = "reports.view")]
    ReportsView,
}

/// Traitement des alertes : exactement les écritures d'un opérateur.
const ALERTS_OPERATE: &[(&str, &str)] = OPERATOR_ROUTES;

const MONITORS_MANAGE: &[(&str, &str)] = &[
    ("POST", "/targets"),
    ("POST", "/targets/test"),
    ("POST", "/targets/import"),
    ("POST", "/targets/import/kuma"),
    ("POST", "/targets/import/kuma-db"),
    ("POST", "/targets/import/uptimerobot"),
    ("POST", "/targets/import/gatus"),
    ("POST", "/targets/import/healthchecks"),
    ("POST", "/targets/reorder"),
    ("POST", "/targets/folders/rename"),
    ("PATCH", "/targets/bulk"),
    ("PUT", "/targets/*"),
    ("DELETE", "/targets/*"),
    ("POST", "/targets/*/probe"),
    ("POST", "/targets/*/discover"),
    ("POST", "/targets/*/release"),
    ("POST", "/monitor-templates"),
    ("PUT", "/monitor-templates/*"),
    ("DELETE", "/monitor-templates/*"),
    ("POST", "/alerts/rules"),
    ("POST", "/alerts/rules/backtest"),
    ("PUT", "/alerts/rules/*"),
    ("DELETE", "/alerts/rules/*"),
    ("POST", "/alerts/rules/*/enable"),
    ("POST", "/alerts/rules/*/reset"),
    ("POST", "/slos"),
    ("PUT", "/slos/*"),
    ("DELETE", "/slos/*"),
];

/// Le serveur de courriel (`/notify/mail-server`) n'y est pas : il porte un
/// mot de passe d'instance.
const NOTIFICATIONS_MANAGE: &[(&str, &str)] = &[
    ("POST", "/notify/channels"),
    ("PUT", "/notify/channels/*"),
    ("DELETE", "/notify/channels/*"),
    ("POST", "/notify/channels/*/test"),
    ("POST", "/notify/template-preview"),
    ("PUT", "/notify/policy"),
    ("POST", "/notify/match-preview"),
];

/// Ni l'accès protégé d'une page (son mot de passe), ni ses abonnés (des
/// données personnelles), ni leurs canaux : ceux-là restent à l'administrateur.
const STATUS_PAGES_MANAGE: &[(&str, &str)] = &[
    ("POST", "/status-pages"),
    ("PUT", "/status-pages/*"),
    ("DELETE", "/status-pages/*"),
    ("PUT", "/status-pages/*/items"),
    ("PUT", "/status-pages/*/languages"),
    ("PUT", "/status-pages/*/logo"),
    ("DELETE", "/status-pages/*/logo"),
    ("POST", "/incidents"),
    ("PUT", "/incidents/*"),
    ("DELETE", "/incidents/*"),
    ("POST", "/incidents/*/updates"),
    ("PUT", "/incidents/*/postmortem"),
    ("DELETE", "/incidents/*/postmortem"),
];

const MAINTENANCE_MANAGE: &[(&str, &str)] = &[
    ("POST", "/alerts/silences"),
    ("PUT", "/alerts/silences/*"),
    ("DELETE", "/alerts/silences/*"),
];

/// Des lectures : les plannings de rapports sont réservés à l'administrateur.
const REPORTS_VIEW: &[(&str, &str)] = &[
    ("GET", "/reports/schedules"),
    ("GET", "/reports/schedules/*"),
    ("GET", "/reports/schedules/*/pdf"),
];

impl Permission {
    /// Le catalogue entier, dans l'ordre de présentation.
    pub const ALL: [Self; 7] = [
        Self::View,
        Self::AlertsOperate,
        Self::MonitorsManage,
        Self::NotificationsManage,
        Self::StatusPagesManage,
        Self::MaintenanceManage,
        Self::ReportsView,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::View => "view",
            Self::AlertsOperate => "alerts.operate",
            Self::MonitorsManage => "monitors.manage",
            Self::NotificationsManage => "notifications.manage",
            Self::StatusPagesManage => "status_pages.manage",
            Self::MaintenanceManage => "maintenance.manage",
            Self::ReportsView => "reports.view",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|permission| permission.as_str() == value)
    }

    /// Nom court, en anglais : l'interface traduit les permissions qu'elle
    /// connaît et affiche celui-ci pour les autres.
    pub fn label(self) -> &'static str {
        match self {
            Self::View => "View",
            Self::AlertsOperate => "Handle alerts",
            Self::MonitorsManage => "Manage monitors",
            Self::NotificationsManage => "Manage notifications",
            Self::StatusPagesManage => "Manage status pages",
            Self::MaintenanceManage => "Manage maintenance",
            Self::ReportsView => "View reports",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::View => "Read everything a viewer reads.",
            Self::AlertsOperate => "Acknowledge, ignore and silence alerts.",
            Self::MonitorsManage => "Edit monitors, templates, alert rules and SLOs.",
            Self::NotificationsManage => "Edit channels, the policy and message templates.",
            Self::StatusPagesManage => "Edit status pages and publish incidents.",
            Self::MaintenanceManage => "Schedule and edit maintenance windows.",
            Self::ReportsView => "Read scheduled reports and their PDF.",
        }
    }

    /// Les routes que la permission ouvre, en plus des lectures d'un lecteur.
    pub fn routes(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::View => &[],
            Self::AlertsOperate => ALERTS_OPERATE,
            Self::MonitorsManage => MONITORS_MANAGE,
            Self::NotificationsManage => NOTIFICATIONS_MANAGE,
            Self::StatusPagesManage => STATUS_PAGES_MANAGE,
            Self::MaintenanceManage => MAINTENANCE_MANAGE,
            Self::ReportsView => REPORTS_VIEW,
        }
    }

    /// Vrai si `method path` est l'une des routes de la permission.
    pub fn allows(self, method: &Method, path: &str) -> bool {
        let path = path.strip_prefix("/api").unwrap_or(path);
        let path = path.strip_suffix('/').filter(|p| !p.is_empty()).unwrap_or(path);
        self.routes().iter().any(|(wanted, pattern)| {
            (*wanted == method.as_str() || (*wanted == "GET" && *method == Method::HEAD))
                && teams::path_matches(pattern, path)
        })
    }
}

/// Remet un jeu de permissions en forme : `view` toujours compris, chaque
/// permission une fois, dans l'ordre du catalogue.
pub fn normalize(permissions: &[Permission]) -> Vec<Permission> {
    let mut out: Vec<Permission> = permissions.to_vec();
    out.push(Permission::View);
    out.sort_unstable();
    out.dedup();
    out
}

/// Relit le jeu stocké (tableau JSON de noms). Un nom inconnu — écrit par une
/// version plus récente, ou à la main — est ignoré : il n'ouvre rien.
pub fn parse_stored(raw: &str) -> Vec<Permission> {
    let names: Vec<String> = serde_json::from_str(raw).unwrap_or_default();
    let known: Vec<Permission> =
        names.iter().filter_map(|name| Permission::parse(name.as_str())).collect();
    normalize(&known)
}

/// Ce que le compte peut faire, dans les termes du catalogue : pour que
/// l'interface masque ce qu'il ne peut pas faire. Le serveur l'applique de
/// toute façon, route par route.
pub fn effective(user: &User) -> Vec<Permission> {
    match user.role {
        Role::Admin => Permission::ALL.to_vec(),
        Role::Operator => {
            vec![Permission::View, Permission::AlertsOperate, Permission::MaintenanceManage]
        }
        Role::Wall => Vec::new(),
        Role::Viewer => match &user.custom_role {
            Some(role) => role.permissions.clone(),
            None => vec![Permission::View],
        },
    }
}

/// La permission du rôle personnalisé du compte qui ouvre `method path`, s'il
/// y en a une. Toujours `None` pour un rôle livré : un rôle personnalisé ne
/// s'applique qu'à un compte dont le rôle livré est `viewer`.
pub fn granted(user: &User, method: &Method, path: &str) -> Option<Permission> {
    if user.role != Role::Viewer {
        return None;
    }
    let role = user.custom_role.as_ref()?;
    role.permissions.iter().copied().find(|permission| permission.allows(method, path))
}

/// Déposé par le garde quand une permission du rôle personnalisé ouvre la
/// route de la requête : les extracteurs d'administrateur et d'opérateur
/// l'acceptent alors, pour cette route seulement.
#[derive(Debug, Clone, Copy)]
pub struct Delegated(pub Permission);

/// Plafond des droits d'équipe ([`teams::Level`]) d'un compte : celui de son
/// rôle livré, ou celui que ses permissions justifient.
pub fn team_ceiling(user: &User) -> Level {
    match &user.custom_role {
        Some(role) if user.role == Role::Viewer => {
            let has = |permission| role.permissions.contains(&permission);
            if has(Permission::MonitorsManage) {
                Level::Manage
            } else if has(Permission::AlertsOperate) || has(Permission::MaintenanceManage) {
                Level::Operate
            } else {
                Level::Read
            }
        }
        _ => Level::ceiling(user.role),
    }
}

/// Le compte a un rôle personnalisé qui change quelque chose (au-delà de la
/// lecture) : la politique « second facteur obligatoire » le vise comme un
/// opérateur.
pub fn writes_anything(user: &User) -> bool {
    user.role == Role::Viewer
        && user.custom_role.as_ref().is_some_and(|role| {
            role.permissions
                .iter()
                .any(|permission| !matches!(permission, Permission::View | Permission::ReportsView))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::custom_roles::Assigned;

    fn user(role: Role, permissions: Option<&[Permission]>) -> User {
        User {
            id: 1,
            username: "jane".into(),
            display_name: String::new(),
            role,
            password_hash: Some("hash".into()),
            oidc_subject: None,
            oidc_issuer: None,
            created_at: String::new(),
            last_login_at: None,
            disabled: false,
            totp_secret: None,
            totp_enabled: false,
            ldap_id: None,
            ldap_dn: None,
            disabled_by_ldap: false,
            saml_subject: None,
            saml_issuer: None,
            email: None,
            custom_role: permissions.map(|permissions| Assigned {
                id: 7,
                name: "Monitor editors".into(),
                permissions: normalize(permissions),
            }),
        }
    }

    #[test]
    fn names_round_trip_and_unknown_names_open_nothing() {
        for permission in Permission::ALL {
            assert_eq!(Permission::parse(permission.as_str()), Some(permission));
            assert_eq!(
                serde_json::to_value(permission).unwrap(),
                serde_json::json!(permission.as_str())
            );
        }
        assert_eq!(Permission::parse("users.manage"), None);
        assert_eq!(
            parse_stored(r#"["monitors.manage","users.manage","backup.restore"]"#),
            vec![Permission::View, Permission::MonitorsManage]
        );
        assert_eq!(parse_stored("not json"), vec![Permission::View]);
    }

    #[test]
    fn a_custom_role_always_includes_view() {
        assert_eq!(normalize(&[]), vec![Permission::View]);
        assert_eq!(
            normalize(&[Permission::ReportsView, Permission::View, Permission::ReportsView]),
            vec![Permission::View, Permission::ReportsView]
        );
    }

    #[test]
    fn monitor_editors_cannot_ack_alerts_or_reach_admin_routes() {
        let editor = user(Role::Viewer, Some(&[Permission::MonitorsManage]));
        let yes = |m: Method, p: &str| assert!(granted(&editor, &m, p).is_some(), "{m} {p}");
        let no = |m: Method, p: &str| assert!(granted(&editor, &m, p).is_none(), "{m} {p}");
        yes(Method::POST, "/targets");
        yes(Method::PUT, "/api/targets/3");
        yes(Method::DELETE, "/targets/3/");
        yes(Method::PATCH, "/targets/bulk");
        yes(Method::POST, "/monitor-templates");
        yes(Method::PUT, "/alerts/rules/2");

        // Alertes : traitement réservé à `alerts.operate`.
        no(Method::POST, "/alerts/abc123/ack");
        no(Method::DELETE, "/alerts/abc123/ack");
        no(Method::POST, "/alerts/silences");
        no(Method::PUT, "/alerts/rules/2/overrides/7");
        // Rien de l'administration.
        no(Method::POST, "/users");
        no(Method::PUT, "/users/1");
        no(Method::POST, "/roles");
        no(Method::PUT, "/roles/1");
        no(Method::POST, "/tokens");
        no(Method::POST, "/backup/restore");
        no(Method::PUT, "/instance/settings");
        no(Method::PUT, "/notify/mail-server");
        no(Method::POST, "/teams");
        no(Method::PUT, "/auth/oidc");
        // Ni les gestes sur les machines, ni les secrets d'un moniteur.
        no(Method::POST, "/targets/3/agent/uninstall");
        no(Method::POST, "/targets/3/containers/web/restart");
        no(Method::POST, "/targets/3/push/regenerate");
        // Un gabarit ne déborde pas d'un segment.
        no(Method::PUT, "/targets/3/agent/relay");
        no(Method::PUT, "/targets//");
        no(Method::POST, "/targets/3/../../users");
        // Les lectures réservées restent fermées.
        no(Method::GET, "/reports/schedules");
        no(Method::GET, "/users");
    }

    #[test]
    fn only_a_viewer_base_role_takes_its_custom_permissions() {
        let all = &Permission::ALL[..];
        // Un rôle personnalisé laissé sur un compte admin, opérateur ou mural ne
        // change rien : seul le rôle livré compte.
        for role in [Role::Operator, Role::Wall] {
            let stray = user(role, Some(all));
            assert!(granted(&stray, &Method::POST, "/targets").is_none(), "{role:?}");
        }
        let wall = user(Role::Wall, Some(all));
        assert!(effective(&wall).is_empty());
        assert_eq!(team_ceiling(&wall), Level::Read);
        let viewer = user(Role::Viewer, None);
        assert_eq!(effective(&viewer), vec![Permission::View]);
        assert!(granted(&viewer, &Method::POST, "/alerts/abc/ack").is_none());
        assert!(!writes_anything(&viewer));
    }

    #[test]
    fn each_permission_opens_its_own_routes() {
        let operator = user(Role::Viewer, Some(&[Permission::AlertsOperate]));
        assert_eq!(
            granted(&operator, &Method::POST, "/alerts/abc/ack"),
            Some(Permission::AlertsOperate)
        );
        assert!(granted(&operator, &Method::POST, "/targets").is_none());
        assert_eq!(team_ceiling(&operator), Level::Operate);
        assert!(writes_anything(&operator));

        let maintenance = user(Role::Viewer, Some(&[Permission::MaintenanceManage]));
        assert!(granted(&maintenance, &Method::POST, "/alerts/silences").is_some());
        assert!(granted(&maintenance, &Method::DELETE, "/alerts/silences/4").is_some());
        assert!(granted(&maintenance, &Method::POST, "/alerts/abc/ack").is_none());

        let reports = user(Role::Viewer, Some(&[Permission::ReportsView]));
        assert!(granted(&reports, &Method::GET, "/reports/schedules").is_some());
        assert!(granted(&reports, &Method::HEAD, "/reports/schedules/3/pdf").is_some());
        assert!(granted(&reports, &Method::POST, "/reports/schedules").is_none());
        assert!(granted(&reports, &Method::DELETE, "/reports/schedules/3").is_none());
        assert!(!writes_anything(&reports));
        assert_eq!(team_ceiling(&reports), Level::Read);

        let pages = user(Role::Viewer, Some(&[Permission::StatusPagesManage]));
        assert!(granted(&pages, &Method::PUT, "/status-pages/2/items").is_some());
        assert!(granted(&pages, &Method::POST, "/incidents/2/updates").is_some());
        assert!(granted(&pages, &Method::PUT, "/status-pages/2/access").is_none());
        assert!(granted(&pages, &Method::DELETE, "/status-pages/2/subscribers/9").is_none());

        let notify = user(Role::Viewer, Some(&[Permission::NotificationsManage]));
        assert!(granted(&notify, &Method::POST, "/notify/channels").is_some());
        assert!(granted(&notify, &Method::PUT, "/notify/policy").is_some());
        assert!(granted(&notify, &Method::PUT, "/notify/mail-server").is_none());

        let editor = user(Role::Viewer, Some(&[Permission::MonitorsManage]));
        assert_eq!(team_ceiling(&editor), Level::Manage);
    }

    #[test]
    fn built_in_roles_keep_their_meaning() {
        assert_eq!(effective(&user(Role::Admin, None)), Permission::ALL.to_vec());
        assert_eq!(
            effective(&user(Role::Operator, None)),
            vec![Permission::View, Permission::AlertsOperate, Permission::MaintenanceManage]
        );
        assert_eq!(team_ceiling(&user(Role::Operator, None)), Level::Manage);
        assert_eq!(team_ceiling(&user(Role::Viewer, None)), Level::Read);
    }
}
