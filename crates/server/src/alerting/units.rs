//! Unités des seuils de règle : ce que l'interface et les messages en font.
//!
//! L'unité d'une règle (`Rule::unit`) est une donnée, pas un libellé libre :
//!
//! - `s`, `min`, `h` et `d` disent « ce seuil est une durée », stockée dans cette
//!   unité ; l'éditeur de règle la montre et la fait saisir en minutes, heures ou
//!   jours, les messages l'écrivent « 2 d » plutôt que « 172800 s » ;
//! - `bool` dit « la série vaut 0 ou 1 » : il n'y a pas de seuil à régler,
//!   l'éditeur le dit au lieu de proposer un « > 0 » qui n'apprend rien ;
//! - le reste (`%`, `°C`, `GiB`…) n'est qu'un suffixe d'affichage.
//!
//! Le pendant côté interface est `web/src/lib/components/alerts/rules/units.ts`.

/// Unité d'une règle « tout ou rien » : la série lue vaut 0 ou 1.
pub const FLAG_UNIT: &str = "bool";

/// Vrai pour l'unité d'une règle « tout ou rien ».
pub fn is_flag(unit: &str) -> bool {
    unit == FLAG_UNIT
}

/// Secondes contenues dans une unité de durée ; `None` si l'unité n'en est pas une.
pub fn duration_base_secs(unit: &str) -> Option<f64> {
    match unit {
        "s" => Some(1.0),
        "min" => Some(60.0),
        "h" => Some(3600.0),
        "d" => Some(86_400.0),
        _ => None,
    }
}

/// Une durée en secondes, à la granularité utile : « 45 s », « 10 min »,
/// « 36 h », « 2.5 d ». Mêmes paliers que `formatDuration` de l'interface.
pub fn human_seconds(secs: f64) -> String {
    let sign = if secs < 0.0 { "-" } else { "" };
    let abs = secs.abs();
    let (amount, suffix) = if abs < 60.0 {
        (abs, "s")
    } else if abs < 3600.0 {
        (abs / 60.0, "min")
    } else if abs < 2.0 * 86_400.0 {
        (abs / 3600.0, "h")
    } else {
        (abs / 86_400.0, "d")
    };
    let rounded = (amount * 10.0).round() / 10.0;
    let text = if rounded.fract().abs() < 1e-9 {
        format!("{}", rounded as i64)
    } else {
        format!("{rounded:.1}")
    };
    format!("{sign}{text} {suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notify::message::format_value;

    #[test]
    fn les_unites_de_duree_sont_reconnues() {
        assert_eq!(duration_base_secs("s"), Some(1.0));
        assert_eq!(duration_base_secs("min"), Some(60.0));
        assert_eq!(duration_base_secs("h"), Some(3600.0));
        assert_eq!(duration_base_secs("d"), Some(86_400.0));
        assert_eq!(duration_base_secs("%"), None);
        assert_eq!(duration_base_secs("ms"), None);
        assert_eq!(duration_base_secs(""), None);
    }

    #[test]
    fn seul_bool_est_un_drapeau() {
        assert!(is_flag("bool"));
        assert!(!is_flag(""));
        assert!(!is_flag("s"));
    }

    #[test]
    fn les_durees_se_lisent_a_la_bonne_echelle() {
        assert_eq!(human_seconds(45.0), "45 s");
        assert_eq!(human_seconds(0.5), "0.5 s");
        assert_eq!(human_seconds(600.0), "10 min");
        assert_eq!(human_seconds(36.0 * 3600.0), "36 h");
        assert_eq!(human_seconds(2.0 * 86_400.0), "2 d");
        assert_eq!(human_seconds(2.5 * 86_400.0), "2.5 d");
        assert_eq!(human_seconds(-3.0 * 86_400.0), "-3 d");
    }

    #[test]
    fn les_messages_ecrivent_les_durees_et_taisent_les_drapeaux() {
        // Un seuil en secondes ne s'écrit plus « 172800 s ».
        assert_eq!(format_value(172_800.0, "s"), "2 d");
        assert_eq!(format_value(30.0, "s"), "30 s");
        // Un seuil en jours ou en heures garde son échelle.
        assert_eq!(format_value(14.0, "d"), "14 d");
        assert_eq!(format_value(4.0, "h"), "4 h");
        // Un drapeau n'a pas de suffixe : « 1 bool » ne veut rien dire.
        assert_eq!(format_value(1.0, "bool"), "1");
        assert_eq!(format_value(95.0, "%"), "95 %");
    }
}
