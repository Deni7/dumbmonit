<?php
/**
 * Requests to DumbMonit: the public document of a page and the list of public
 * pages. Server side only, nothing is loaded from the visitor's browser.
 *
 * Only the http(s) address the admin saved (or typed on the settings page) is
 * ever requested, re-checked before each request, with redirects disabled so
 * the request cannot be bounced to another host. `wp_remote_get`, not the
 * "safe" variant that refuses LAN addresses: DumbMonit is often on a private IP.
 */

if (!defined('ABSPATH')) {
    exit;
}

function dmt_api_url(string $base, string $path = ''): string
{
    return untrailingslashit($base) . '/api/public/status' . $path;
}

/**
 * GET a JSON document from DumbMonit.
 *
 * @return array|WP_Error
 */
function dmt_get_json(string $url, string $lang = '')
{
    $headers = ['Accept' => 'application/json'];
    if ($lang !== '') {
        $headers['Accept-Language'] = $lang;
    }
    $response = wp_remote_get($url, [
        'timeout' => 3,
        'redirection' => 0,
        'limit_response_size' => 2 * MB_IN_BYTES,
        'headers' => $headers,
    ]);
    if (is_wp_error($response)) {
        return $response;
    }
    $code = (int) wp_remote_retrieve_response_code($response);
    if ($code !== 200) {
        return new WP_Error('dmt_http', sprintf('HTTP %d', $code), ['status' => $code]);
    }
    $data = json_decode(wp_remote_retrieve_body($response), true);
    if (!is_array($data)) {
        return new WP_Error('dmt_json', __('The server did not answer with a DumbMonit document.', 'dumbmonit-status-banner'));
    }
    return $data;
}

/** What the banner needs from the page's document; the rest (history, incidents) is not kept. */
function dmt_slim(array $data): array
{
    $groups = [];
    foreach ((array) ($data['groups'] ?? []) as $group) {
        if (!is_array($group)) {
            continue;
        }
        $items = [];
        foreach ((array) ($group['items'] ?? []) as $item) {
            if (!is_array($item)) {
                continue;
            }
            $items[] = [
                'key' => dmt_str($item['key'] ?? ''),
                'label' => dmt_str($item['label'] ?? ''),
                'state' => dmt_str($item['state'] ?? ''),
            ];
        }
        $groups[] = ['name' => dmt_str($group['name'] ?? ''), 'items' => $items];
    }
    $page = isset($data['page']) && is_array($data['page']) ? $data['page'] : [];
    return [
        'overall' => dmt_str($data['overall'] ?? ''),
        'title' => dmt_str($page['title'] ?? ''),
        'groups' => $groups,
    ];
}

function dmt_cache_key(string $base, string $slug, string $lang): string
{
    return 'dmt_s_' . md5($base . '|' . $slug . '|' . strtolower($lang));
}

/** Forgets the cached status of a page, in the languages this site uses. */
function dmt_forget_status(string $base, string $slug): void
{
    $langs = ['en', dmt_server_lang(determine_locale()), dmt_server_lang(get_locale())];
    if (function_exists('pll_languages_list')) {
        $langs = array_merge($langs, array_map('dmt_server_lang', array_map('dmt_str', (array) pll_languages_list(['fields' => 'locale']))));
    }
    foreach (array_unique($langs) as $lang) {
        delete_transient(dmt_cache_key($base, $slug, $lang));
    }
    delete_transient(DMT_ERR);
}

/**
 * The page's document in a language, cached for `$ttl` seconds. A failure is
 * cached too (shorter), so an unreachable server does not slow every page view
 * down by the request timeout; the error is kept for the admin notice.
 *
 * @return array|WP_Error
 */
function dmt_fetch_status(string $base, string $slug, string $lang, int $ttl)
{
    if (!dmt_valid_base($base) || !dmt_valid_slug($slug)) {
        return new WP_Error('dmt_config', 'not configured');
    }
    $key = dmt_cache_key($base, $slug, $lang);
    $cached = get_transient($key);
    if (is_array($cached)) {
        if (isset($cached['error'])) {
            return new WP_Error('dmt_cached', dmt_str($cached['error']));
        }
        if (isset($cached['doc']) && is_array($cached['doc'])) {
            return $cached['doc'];
        }
    }
    $url = add_query_arg('lang', rawurlencode($lang), dmt_api_url($base, '/' . rawurlencode($slug)));
    $data = dmt_get_json($url, $lang);
    if (is_wp_error($data)) {
        $message = $data->get_error_message();
        set_transient($key, ['error' => $message], max(15, min($ttl, 300)));
        set_transient(DMT_ERR, $message, 10 * MINUTE_IN_SECONDS);
        return $data;
    }
    $doc = dmt_slim($data);
    set_transient($key, ['doc' => $doc], max(15, $ttl));
    delete_transient(DMT_ERR);
    return $doc;
}

/**
 * The public pages a DumbMonit server lists (`GET /api/public/status`).
 *
 * @return array|WP_Error List of ['slug' => …, 'title' => …, 'languages' => […]].
 */
function dmt_discover(string $base)
{
    if (!dmt_valid_base($base)) {
        return new WP_Error('dmt_config', __('Invalid DumbMonit address: expected http:// or https:// followed by the server name. It was not saved.', 'dumbmonit-status-banner'));
    }
    $data = dmt_get_json(dmt_api_url($base));
    if (is_wp_error($data)) {
        $detail = $data->get_error_data();
        $status = is_array($detail) && isset($detail['status']) ? (int) $detail['status'] : 0;
        if ($status === 404 || $status === 405) {
            return new WP_Error('dmt_no_directory', __('This server does not list its pages (it may be older than this plugin). Type the page address instead.', 'dumbmonit-status-banner'));
        }
        if ($data->get_error_code() === 'dmt_json') {
            return $data;
        }
        return new WP_Error('dmt_unreachable', sprintf(
            /* translators: %s: error detail */
            __('The server could not be reached: %s', 'dumbmonit-status-banner'),
            $data->get_error_message()
        ));
    }
    $pages = [];
    foreach ((array) ($data['pages'] ?? []) as $page) {
        if (!is_array($page)) {
            continue;
        }
        $slug = dmt_str($page['slug'] ?? '');
        if (!dmt_valid_slug($slug)) {
            continue;
        }
        $title = dmt_str($page['title'] ?? '');
        $languages = array_values(array_filter(array_map('dmt_str', (array) ($page['languages'] ?? []))));
        $pages[] = ['slug' => $slug, 'title' => $title !== '' ? $title : $slug, 'languages' => $languages];
    }
    return $pages;
}
