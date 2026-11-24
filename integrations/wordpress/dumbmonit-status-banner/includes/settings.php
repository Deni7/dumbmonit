<?php
/**
 * Settings: defaults, reading (with the 1.x option upgraded on the fly),
 * validation of what the admin saves.
 */

if (!defined('ABSPATH')) {
    exit;
}

/** States of a page the banner can show, each with its own optional text. */
const DMT_STATES = ['operational', 'maintenance', 'incident', 'down'];

function dmt_defaults(): array
{
    return [
        // DumbMonit server, http(s)://host[:port][/path], without a trailing slash.
        'base' => '',
        // Address of the status page on that server (the part after /s/).
        'slug' => '',
        // top | bottom | inline (shortcode and widget only).
        'display' => 'top',
        // issues | always.
        'show' => 'issues',
        'names' => false,
        'link' => true,
        'dismissible' => true,
        // Keys of the services that count; empty: all of them.
        'components' => [],
        // status | custom.
        'colors' => 'status',
        'light_bg' => '#f1f5f9',
        'light_fg' => '#0f172a',
        'dark_bg' => '#1e293b',
        'dark_fg' => '#f1f5f9',
        'text_operational' => '',
        'text_maintenance' => '',
        'text_incident' => '',
        'text_down' => '',
        // Seconds the status is kept before DumbMonit is asked again.
        'ttl' => 60,
    ];
}

/** A scalar as a string; anything else as ''. */
function dmt_str($value): string
{
    return is_scalar($value) ? (string) $value : '';
}

/** http(s)://host[:port][/path], without credentials, query or fragment. */
function dmt_valid_base(string $url): bool
{
    return (bool) preg_match('#^https?://[^/\s?\#@]+(/[^\s?\#]*)?$#', $url)
        && substr($url, -1) !== '/';
}

function dmt_valid_slug(string $slug): bool
{
    return (bool) preg_match('/^[a-z0-9-]{2,40}$/', $slug);
}

function dmt_valid_key(string $key): bool
{
    return (bool) preg_match('/^[a-z0-9-]{1,80}$/', $key);
}

/** `#rgb` or `#rrggbb`, else the fallback. */
function dmt_hex(string $value, string $fallback): string
{
    return preg_match('/^#([0-9a-fA-F]{3}){1,2}$/', $value) ? $value : $fallback;
}

/**
 * Splits what the admin typed into the server address and, when the link of a
 * page was pasted (`…/s/<slug>` or `…/api/public/status/<slug>`), its address.
 *
 * @return array{0: string, 1: string} ['' , ''] when the address is not valid.
 */
function dmt_split_address(string $raw): array
{
    $url = untrailingslashit(esc_url_raw(trim($raw), ['http', 'https']));
    $slug = '';
    if (preg_match('#^(.+?)/(?:s|api/public/status)/([a-z0-9-]{2,40})(?:/.*)?$#', $url, $found)) {
        $url = $found[1];
        $slug = $found[2];
    } elseif (preg_match('#^(.+?)/api/public/status$#', $url, $found)) {
        $url = $found[1];
    }
    return dmt_valid_base($url) ? [$url, $slug] : ['', ''];
}

/** The saved settings, completed with the defaults. */
function dmt_settings(): array
{
    $stored = get_option(DMT_OPT, []);
    $stored = is_array($stored) ? $stored : [];
    // 1.x saved one `url` (https://host/api/public/status/<slug>) and `position`.
    if (empty($stored['base']) && isset($stored['url']) && is_string($stored['url'])) {
        list($base, $slug) = dmt_split_address($stored['url']);
        if ($base !== '') {
            $stored['base'] = $base;
            $stored['slug'] = $slug;
        }
    }
    if (!isset($stored['display']) && isset($stored['position'])) {
        $stored['display'] = $stored['position'] === 'bottom' ? 'bottom' : 'top';
    }
    unset($stored['url'], $stored['position']);
    $settings = wp_parse_args($stored, dmt_defaults());
    // Each value keeps the type of its default, whatever the database holds.
    foreach (dmt_defaults() as $key => $default) {
        if (is_string($default)) {
            $settings[$key] = dmt_str($settings[$key]);
        } elseif (is_bool($default)) {
            $settings[$key] = (bool) $settings[$key];
        }
    }
    $settings['components'] = is_array($settings['components'])
        ? array_values(array_filter(array_map('dmt_str', $settings['components']), 'dmt_valid_key'))
        : [];
    $settings['ttl'] = max(15, min(3600, (int) $settings['ttl']));
    return $settings;
}

/** Validates the settings form. A refused field keeps its previous value. */
function dmt_sanitize($input): array
{
    $input = is_array($input) ? $input : [];
    $previous = dmt_settings();
    $defaults = dmt_defaults();
    $out = $defaults;

    $raw = trim(dmt_str($input['base'] ?? ''));
    list($base, $slug_from_link) = dmt_split_address($raw);
    if ($raw !== '' && $base === '') {
        add_settings_error(DMT_OPT, 'dmt_bad_base', __('Invalid DumbMonit address: expected http:// or https:// followed by the server name. It was not saved.', 'dumbmonit-status-banner'));
        $base = $previous['base'];
    }
    $out['base'] = $base;

    $slug = strtolower(trim(dmt_str($input['slug'] ?? '')));
    if ($slug === '') {
        $slug = $slug_from_link;
    }
    if ($slug !== '' && !dmt_valid_slug($slug)) {
        add_settings_error(DMT_OPT, 'dmt_bad_slug', __('Invalid page address: 2 to 40 lowercase letters, digits or hyphens. It was not saved.', 'dumbmonit-status-banner'));
        $slug = $previous['slug'];
    }
    $out['slug'] = $slug;

    $display = dmt_str($input['display'] ?? '');
    $out['display'] = in_array($display, ['top', 'bottom', 'inline'], true) ? $display : 'top';
    $out['show'] = dmt_str($input['show'] ?? '') === 'always' ? 'always' : 'issues';
    foreach (['names', 'link', 'dismissible'] as $flag) {
        $out[$flag] = !empty($input[$flag]);
    }

    $keys = isset($input['components']) && is_array($input['components']) ? $input['components'] : [];
    $out['components'] = array_values(array_unique(array_filter(array_map('dmt_str', $keys), 'dmt_valid_key')));

    $out['colors'] = dmt_str($input['colors'] ?? '') === 'custom' ? 'custom' : 'status';
    foreach (['light_bg', 'light_fg', 'dark_bg', 'dark_fg'] as $colour) {
        $hex = sanitize_hex_color(dmt_str($input[$colour] ?? ''));
        $out[$colour] = $hex ? $hex : $previous[$colour];
    }

    foreach (DMT_STATES as $state) {
        $text = sanitize_text_field(dmt_str($input['text_' . $state] ?? ''));
        $out['text_' . $state] = function_exists('mb_substr') ? mb_substr($text, 0, 200) : substr($text, 0, 200);
    }

    $ttl = absint($input['ttl'] ?? $defaults['ttl']);
    $out['ttl'] = max(15, min(3600, $ttl > 0 ? $ttl : $defaults['ttl']));

    // Checked now, so a typing mistake shows at once rather than as a missing banner.
    if ($out['base'] !== '' && $out['slug'] !== '') {
        dmt_forget_status($out['base'], $out['slug']);
        $status = dmt_fetch_status($out['base'], $out['slug'], dmt_server_lang(determine_locale()), $out['ttl']);
        if (is_wp_error($status)) {
            add_settings_error(DMT_OPT, 'dmt_unreachable', sprintf(
                /* translators: %s: error detail */
                __('Saved, but the status page could not be fetched: %s', 'dumbmonit-status-banner'),
                $status->get_error_message()
            ), 'warning');
        }
    }
    return $out;
}

/**
 * A custom text in the visitor's language: Polylang and WPML translate the
 * texts registered from the settings; without them, the text as written.
 */
function dmt_custom_text(array $settings, string $state): string
{
    $text = dmt_str($settings['text_' . $state] ?? '');
    if ($text === '') {
        return '';
    }
    if (function_exists('pll__')) {
        return (string) pll__($text);
    }
    return dmt_str(apply_filters('wpml_translate_single_string', $text, 'dumbmonit-status-banner', 'text_' . $state));
}

add_action('admin_init', function () {
    register_setting('dmt_group', DMT_OPT, [
        'type' => 'array',
        'sanitize_callback' => 'dmt_sanitize',
        'default' => [],
    ]);
    // Custom texts become translatable strings in Polylang and WPML.
    $settings = dmt_settings();
    foreach (DMT_STATES as $state) {
        $text = $settings['text_' . $state];
        if ($text === '') {
            continue;
        }
        if (function_exists('pll_register_string')) {
            pll_register_string('dmt_text_' . $state, $text, 'DumbMonit status banner');
        }
        do_action('wpml_register_single_string', 'dumbmonit-status-banner', 'text_' . $state, $text);
    }
});
