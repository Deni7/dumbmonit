//! Fichiers du bandeau d'état pour sites tiers (`integrations/`), proposés en
//! téléchargement dans les réglages d'une page de statut.
//!
//! * `GET /integrations/wordpress.zip` — le plugin WordPress, installable par
//!   Extensions → Ajouter → Téléverser (un dossier racine au nom du plugin).
//! * `GET /integrations/banner.php` — l'aide PHP à inclure dans un site.
//!
//! Les fichiers sont embarqués dans le binaire. Le zip est écrit à la main, sans
//! compression (« stored ») : quelques dizaines de kio, aucune dépendance de plus.
//! Rien de secret ici : ce sont les mêmes fichiers que dans le dépôt.

use axum::Router;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::routing::get;

use crate::state::AppState;

/// Dossier racine du plugin dans l'archive : WordPress en tire le nom du plugin.
const PLUGIN_DIR: &str = "dumbmonit-status-banner";

/// Un fichier du plugin : son chemin dans le dossier du plugin, et son contenu.
macro_rules! plugin_file {
    ($name:literal) => {
        (
            $name,
            include_bytes!(concat!(
                "../../../../integrations/wordpress/dumbmonit-status-banner/",
                $name
            )),
        )
    };
}

/// Une langue livrée : la source `.po` des traducteurs et le `.l10n.php` que
/// lit WordPress 6.5.
macro_rules! plugin_locale {
    ($locale:literal) => {
        [
            (
                concat!("languages/dumbmonit-status-banner-", $locale, ".po"),
                include_bytes!(concat!(
                    "../../../../integrations/wordpress/dumbmonit-status-banner/",
                    "languages/dumbmonit-status-banner-",
                    $locale,
                    ".po"
                )),
            ),
            (
                concat!("languages/dumbmonit-status-banner-", $locale, ".l10n.php"),
                include_bytes!(concat!(
                    "../../../../integrations/wordpress/dumbmonit-status-banner/",
                    "languages/dumbmonit-status-banner-",
                    $locale,
                    ".l10n.php"
                )),
            ),
        ]
    };
}

const PLUGIN_LOCALES: &[[(&str, &[u8]); 2]] = &[
    plugin_locale!("de_DE"),
    plugin_locale!("es_ES"),
    plugin_locale!("fr_FR"),
    plugin_locale!("it_IT"),
    plugin_locale!("ja"),
    plugin_locale!("ko_KR"),
    plugin_locale!("nl_NL"),
    plugin_locale!("pl_PL"),
    plugin_locale!("pt_BR"),
    plugin_locale!("pt_PT"),
    plugin_locale!("ru_RU"),
    plugin_locale!("tr_TR"),
    plugin_locale!("zh_CN"),
];

const PLUGIN_FILES: &[(&str, &[u8])] = &[
    plugin_file!("dumbmonit-status-banner.php"),
    plugin_file!("uninstall.php"),
    plugin_file!("readme.txt"),
    plugin_file!("includes/i18n.php"),
    plugin_file!("includes/settings.php"),
    plugin_file!("includes/client.php"),
    plugin_file!("includes/render.php"),
    plugin_file!("includes/admin.php"),
    plugin_file!("includes/widget.php"),
    plugin_file!("assets/admin.js"),
    plugin_file!("languages/dumbmonit-status-banner.pot"),
];

/// Tous les fichiers du plugin, langues comprises.
fn plugin_files() -> impl Iterator<Item = &'static (&'static str, &'static [u8])> {
    PLUGIN_FILES.iter().chain(PLUGIN_LOCALES.iter().flatten())
}

const BANNER_PHP: &str = include_str!("../../../../integrations/php/banner.php");

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/integrations/wordpress.zip", get(wordpress_zip))
        .route("/integrations/banner.php", get(banner_php))
}

/// Le plugin, rangé sous son dossier racine.
fn plugin_zip() -> Vec<u8> {
    let files: Vec<(String, &[u8])> =
        plugin_files().map(|(name, body)| (format!("{PLUGIN_DIR}/{name}"), *body)).collect();
    stored_zip(&files)
}

async fn wordpress_zip() -> Response {
    attachment("application/zip", "dumbmonit-status-banner.zip", plugin_zip())
}

async fn banner_php() -> Response {
    attachment("text/x-php; charset=utf-8", "banner.php", BANNER_PHP.as_bytes().to_vec())
}

fn attachment(content_type: &'static str, name: &str, body: Vec<u8>) -> Response {
    (
        [
            (header::CONTENT_TYPE, content_type.to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{name}\"")),
            (header::CACHE_CONTROL, "no-cache".to_string()),
        ],
        body,
    )
        .into_response()
}

/// CRC-32 (IEEE), tel que l'exige le format zip.
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

/// Archive zip sans compression. Date fixe (1er janvier 1980) : deux appels
/// donnent exactement les mêmes octets.
fn stored_zip(files: &[(String, &[u8])]) -> Vec<u8> {
    const DOS_DATE: u16 = 0x0021;
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, body) in files {
        let offset = out.len() as u32;
        let crc = crc32(body);
        let size = body.len() as u32;
        let name_len = name.len() as u16;
        // En-tête local.
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes()); // version nécessaire
        out.extend_from_slice(&0x0800u16.to_le_bytes()); // noms en UTF-8
        out.extend_from_slice(&0u16.to_le_bytes()); // méthode : stored
        out.extend_from_slice(&0u16.to_le_bytes()); // heure
        out.extend_from_slice(&DOS_DATE.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&name_len.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // extra
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(body);
        // Entrée du répertoire central.
        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&0x031Eu16.to_le_bytes()); // créé sous Unix, v3.0
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&0x0800u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&DOS_DATE.to_le_bytes());
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&name_len.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes()); // extra
        central.extend_from_slice(&0u16.to_le_bytes()); // commentaire
        central.extend_from_slice(&0u16.to_le_bytes()); // disque
        central.extend_from_slice(&0u16.to_le_bytes()); // attributs internes
        central.extend_from_slice(&(0o100_644u32 << 16).to_le_bytes()); // fichier 0644
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let central_offset = out.len() as u32;
    let count = files.len() as u16;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&central_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u16_at(b: &[u8], i: usize) -> usize {
        u16::from_le_bytes([b[i], b[i + 1]]) as usize
    }
    fn u32_at(b: &[u8], i: usize) -> usize {
        u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) as usize
    }

    /// Relit l'archive par son répertoire central, comme le ferait un
    /// décompresseur : nom, contenu, CRC.
    fn read_zip(zip: &[u8]) -> Vec<(String, Vec<u8>)> {
        let eocd = zip.len() - 22;
        assert_eq!(u32_at(zip, eocd), 0x0605_4b50, "fin de répertoire central");
        let count = u16_at(zip, eocd + 10);
        let mut pos = u32_at(zip, eocd + 16);
        let mut out = Vec::new();
        for _ in 0..count {
            assert_eq!(u32_at(zip, pos), 0x0201_4b50);
            let crc = u32_at(zip, pos + 16);
            let size = u32_at(zip, pos + 24);
            let name_len = u16_at(zip, pos + 28);
            let local = u32_at(zip, pos + 42);
            let name = String::from_utf8(zip[pos + 46..pos + 46 + name_len].to_vec()).unwrap();
            assert_eq!(u32_at(zip, local), 0x0403_4b50);
            let start = local + 30 + u16_at(zip, local + 26) + u16_at(zip, local + 28);
            let body = zip[start..start + size].to_vec();
            assert_eq!(crc32(&body) as usize, crc, "CRC de {name}");
            out.push((name, body));
            pos += 46 + name_len;
        }
        out
    }

    #[test]
    fn crc32_valeur_connue() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn le_zip_contient_le_plugin_sous_son_dossier() {
        let entries = read_zip(&plugin_zip());
        assert_eq!(entries.len(), plugin_files().count());
        let main = entries
            .iter()
            .find(|(name, _)| name == "dumbmonit-status-banner/dumbmonit-status-banner.php")
            .expect("fichier principal du plugin");
        assert!(String::from_utf8_lossy(&main.1).contains("Plugin Name:"));
        assert!(entries.iter().all(|(name, _)| name.starts_with("dumbmonit-status-banner/")));
    }

    #[test]
    fn le_zip_embarque_chaque_fichier_charge_par_le_plugin() {
        let names: Vec<&str> = plugin_files().map(|(name, _)| *name).collect();
        let main = String::from_utf8_lossy(PLUGIN_FILES[0].1);
        // `require_once DMT_DIR . 'includes/….php'` : chacun doit être dans l'archive.
        let required: Vec<&str> =
            main.split("DMT_DIR . '").skip(1).filter_map(|rest| rest.split('\'').next()).collect();
        assert!(required.len() >= 5, "{required:?}");
        for path in required {
            assert!(names.contains(&path), "{path} manque dans l'archive");
        }
        let admin = String::from_utf8_lossy(
            plugin_files().find(|(name, _)| *name == "includes/admin.php").expect("admin.php").1,
        )
        .into_owned();
        assert!(admin.contains("assets/admin.js") && names.contains(&"assets/admin.js"));
        // Chaque fichier PHP, traductions comprises, commence par la balise PHP.
        for (name, body) in plugin_files().filter(|(name, _)| name.ends_with(".php")) {
            assert!(body.starts_with(b"<?php"), "{name}");
        }
        assert_eq!(names.iter().filter(|name| name.ends_with(".l10n.php")).count(), 13);
    }
}
