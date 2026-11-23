<?php
/**
 * Languages: the locale a visitor reads the site in, the language asked of
 * DumbMonit for the page's own texts, and the plugin's translations.
 *
 * Translations ship as `languages/dumbmonit-status-banner-<locale>.l10n.php`
 * (WordPress 6.5 reads these PHP files without compiling anything), with the
 * `.po` sources next to them for translators. A language pack installed from
 * translate.wordpress.org takes precedence.
 */

if (!defined('ABSPATH')) {
    exit;
}

/** Locales whose translations ship with the plugin; en_US is the source language. */
const DMT_BUNDLED_LOCALES = [
    'de_DE', 'es_ES', 'fr_FR', 'it_IT', 'pt_PT', 'pt_BR', 'ru_RU',
    'zh_CN', 'ja', 'ko_KR', 'nl_NL', 'pl_PL', 'tr_TR',
];

/**
 * Locale of the current request. On the site: the language of Polylang or WPML
 * when one of them is active, else the site's locale. In the admin: the user's
 * own language.
 */
function dmt_request_locale(): string
{
    if (!is_admin()) {
        if (function_exists('pll_current_language')) {
            $polylang = pll_current_language('locale');
            if (is_string($polylang) && $polylang !== '') {
                return $polylang;
            }
        }
        $code = apply_filters('wpml_current_language', null);
        if (is_string($code) && $code !== '' && $code !== 'all') {
            $languages = apply_filters('wpml_active_languages', null, ['skip_missing' => 0]);
            if (is_array($languages) && isset($languages[$code]['default_locale'])
                && is_string($languages[$code]['default_locale'])
                && $languages[$code]['default_locale'] !== '') {
                return $languages[$code]['default_locale'];
            }
            return $code;
        }
    }
    return determine_locale();
}

/**
 * The bundled translation for a locale: the exact one, else the same language
 * (fr_CA reads fr_FR, de_CH_informal reads de_DE). '' for English or a
 * language the plugin does not ship.
 */
function dmt_bundled_locale(string $locale): string
{
    $tag = strtolower(str_replace('-', '_', $locale));
    foreach (DMT_BUNDLED_LOCALES as $bundled) {
        if (strtolower($bundled) === $tag) {
            return $bundled;
        }
    }
    $primary = explode('_', $tag)[0];
    if ($primary === 'pt') {
        return strpos($tag, 'pt_br') === 0 ? 'pt_BR' : 'pt_PT';
    }
    foreach (DMT_BUNDLED_LOCALES as $bundled) {
        if (strtolower(explode('_', $bundled)[0]) === $primary) {
            return $bundled;
        }
    }
    return '';
}

/**
 * Language asked of DumbMonit (`?lang=`): the WordPress locale as a language
 * tag (fr_FR becomes fr-FR). The server maps it onto its own languages and
 * falls back to the page's default language.
 */
function dmt_server_lang(string $locale): string
{
    $tag = str_replace('_', '-', $locale);
    return preg_match('/^[A-Za-z]{2,3}(-[A-Za-z0-9]{1,8})*$/', $tag) ? $tag : 'en';
}

/**
 * Loads the plugin's strings in a locale (default: WordPress's). They are filed
 * under WordPress's current locale, so only this plugin's texts change: the
 * rest of the site keeps its translations.
 */
function dmt_load_translations(string $locale = ''): void
{
    static $loaded = null;
    $locale = $locale !== '' ? $locale : determine_locale();
    if ($loaded === $locale) {
        return;
    }
    $loaded = $locale;
    unload_textdomain('dumbmonit-status-banner');
    $files = [WP_LANG_DIR . '/plugins/dumbmonit-status-banner-' . $locale . '.mo'];
    $bundled = dmt_bundled_locale($locale);
    if ($bundled !== '') {
        $files[] = DMT_DIR . 'languages/dumbmonit-status-banner-' . $bundled . '.mo';
    }
    foreach ($files as $file) {
        // load_textdomain() tries the `.l10n.php` file next to this `.mo` path first.
        if (load_textdomain('dumbmonit-status-banner', $file)) {
            return;
        }
    }
}

add_action('init', function () {
    dmt_load_translations();
});
