<?php
/**
 * The banner on the site: a bar at the top or bottom of every page, or inline
 * where the [dumbmonit_status] shortcode or the widget is placed.
 *
 * Every value printed is escaped. If DumbMonit cannot be reached, nothing is
 * printed and the page renders as usual.
 */

if (!defined('ABSPATH')) {
    exit;
}

/**
 * What the banner says, from the page's document and the services that count.
 *
 * @return array{state: string, tone: string, count: int, names: string[], title: string}
 */
function dmt_summary(array $doc, array $only): array
{
    $all = [];
    foreach ((array) ($doc['groups'] ?? []) as $group) {
        foreach ((array) ($group['items'] ?? []) as $item) {
            $all[] = $item;
        }
    }
    $chosen = $only ? array_values(array_filter($all, function ($item) use ($only) {
        return in_array($item['key'] ?? '', $only, true);
    })) : $all;
    // The ticked services are gone from the page: the whole page counts again.
    if ($only && !$chosen) {
        $chosen = $all;
        $only = [];
    }
    $names = [];
    $hard_down = false;
    $maintenance = false;
    foreach ($chosen as $item) {
        $state = $item['state'] ?? '';
        if ($state === 'down' || $state === 'degraded') {
            $names[] = dmt_str($item['label'] ?? '');
            $hard_down = $hard_down || $state === 'down';
        } elseif ($state === 'maintenance') {
            $maintenance = true;
        }
    }
    $overall = dmt_str($doc['overall'] ?? '');
    if (!$only && $overall === 'maintenance') {
        $state = 'maintenance';
    } elseif ($names) {
        $state = 'down';
    } elseif ($only && $maintenance) {
        $state = 'maintenance';
    } elseif (!$only && $overall !== '' && $overall !== 'operational') {
        $state = 'incident';
    } else {
        $state = 'operational';
    }
    $tones = ['operational' => 'ok', 'maintenance' => 'info', 'incident' => 'warn', 'down' => $hard_down ? 'bad' : 'warn'];
    return [
        'state' => $state,
        'tone' => $tones[$state],
        'count' => count($names),
        'names' => array_values(array_filter($names)),
        'title' => dmt_str($doc['title'] ?? ''),
    ];
}

/** The sentence of the banner: the admin's text for this state, else the translated default. */
function dmt_text(array $settings, array $summary): string
{
    $count = (int) $summary['count'];
    $custom = dmt_custom_text($settings, $summary['state']);
    if ($custom !== '') {
        return str_replace('{count}', number_format_i18n($count), $custom);
    }
    switch ($summary['state']) {
        case 'maintenance':
            return __('Maintenance in progress', 'dumbmonit-status-banner');
        case 'incident':
            return __('Incident in progress', 'dumbmonit-status-banner');
        case 'down':
            return sprintf(
                /* translators: %d: number of services down or degraded */
                _n('%d service is down', '%d services are down', $count, 'dumbmonit-status-banner'),
                $count
            );
        default:
            return __('All systems operational', 'dumbmonit-status-banner');
    }
}

/** Styles of the banner, printed once per page. */
function dmt_style_once(array $settings): string
{
    static $done = false;
    if ($done) {
        return '';
    }
    $done = true;
    $css = '.dmt-banner{box-sizing:border-box;display:flex;flex-wrap:wrap;gap:4px 12px;align-items:center;'
        . 'justify-content:center;padding:10px 16px;font:14px/1.4 system-ui,sans-serif;'
        . 'background:var(--dmt-bg);color:var(--dmt-fg)}'
        . '.dmt-top,.dmt-bottom{position:fixed;left:0;right:0;z-index:99999}.dmt-top{top:0}.dmt-bottom{bottom:0}'
        . 'body.admin-bar .dmt-top{top:32px}@media screen and (max-width:782px){body.admin-bar .dmt-top{top:46px}}'
        . '.dmt-inline{border-radius:6px;margin:0 0 1em}'
        . '.dmt-banner a{color:inherit;text-decoration:underline}'
        . '.dmt-close{all:unset;cursor:pointer;min-width:24px;min-height:24px;padding:0 8px;font-size:20px;'
        . 'line-height:24px;text-align:center}.dmt-close:focus-visible{outline:2px solid currentColor}';
    if ($settings['colors'] === 'custom') {
        $defaults = dmt_defaults();
        $css .= sprintf(
            '.dmt-banner{--dmt-bg:%s;--dmt-fg:%s}@media (prefers-color-scheme:dark){.dmt-banner{--dmt-bg:%s;--dmt-fg:%s}}',
            dmt_hex(dmt_str($settings['light_bg']), $defaults['light_bg']),
            dmt_hex(dmt_str($settings['light_fg']), $defaults['light_fg']),
            dmt_hex(dmt_str($settings['dark_bg']), $defaults['dark_bg']),
            dmt_hex(dmt_str($settings['dark_fg']), $defaults['dark_fg'])
        );
    } else {
        $css .= '.dmt-ok{--dmt-bg:#e6f4ea;--dmt-fg:#14532d}.dmt-warn{--dmt-bg:#fef3c7;--dmt-fg:#78350f}'
            . '.dmt-bad{--dmt-bg:#fee2e2;--dmt-fg:#7f1d1d}.dmt-info{--dmt-bg:#dbeafe;--dmt-fg:#1e3a8a}'
            . '@media (prefers-color-scheme:dark){.dmt-ok{--dmt-bg:#14532d;--dmt-fg:#dcfce7}'
            . '.dmt-warn{--dmt-bg:#78350f;--dmt-fg:#fef3c7}.dmt-bad{--dmt-bg:#7f1d1d;--dmt-fg:#fee2e2}'
            . '.dmt-info{--dmt-bg:#1e3a8a;--dmt-fg:#dbeafe}}';
    }
    return '<style id="dmt-banner-css">' . $css . '</style>';
}

/**
 * Hides the banner for a day once dismissed. The cookie holds a fingerprint of
 * what the banner says: a new problem shows again. Also checked in the browser,
 * for sites behind a page cache.
 */
function dmt_dismiss_script(string $id): string
{
    $js = '(function(){var b=document.getElementById(' . wp_json_encode($id) . ');if(!b)return;'
        . 'var s=b.getAttribute("data-dmt");'
        . 'if(document.cookie.split("; ").indexOf("dmt_dismissed="+s)>=0){b.remove();return;}'
        . 'var x=b.querySelector(".dmt-close");if(x)x.addEventListener("click",function(){'
        . 'document.cookie="dmt_dismissed="+s+"; path=/; max-age=86400; SameSite=Lax";b.remove();});})();';
    return wp_get_inline_script_tag($js);
}

/**
 * The banner's HTML, or '' when there is nothing to show (all is well and the
 * banner shows only problems, dismissed, not set up, or DumbMonit unreachable).
 *
 * @param string $mode top | bottom | inline
 */
function dmt_banner_html(array $settings, string $mode): string
{
    if (!dmt_valid_base($settings['base']) || !dmt_valid_slug($settings['slug'])) {
        return '';
    }
    $locale = dmt_request_locale();
    dmt_load_translations($locale);
    $doc = dmt_fetch_status($settings['base'], $settings['slug'], dmt_server_lang($locale), (int) $settings['ttl']);
    if (is_wp_error($doc)) {
        return '';
    }
    $summary = dmt_summary($doc, (array) $settings['components']);
    if ($summary['state'] === 'operational' && $settings['show'] !== 'always') {
        return '';
    }
    $text = dmt_text($settings, $summary);
    $fingerprint = substr(md5($settings['slug'] . '|' . $summary['state'] . '|' . implode(',', $summary['names'])), 0, 12);
    if ($settings['dismissible'] && isset($_COOKIE['dmt_dismissed']) && $_COOKIE['dmt_dismissed'] === $fingerprint) {
        return '';
    }

    $id = 'dmt-banner-' . wp_unique_id();
    $html = dmt_style_once($settings);
    $html .= sprintf(
        '<div id="%s" class="%s" role="status" data-dmt="%s">',
        esc_attr($id),
        esc_attr('dmt-banner dmt-' . $summary['tone'] . ' dmt-' . $mode),
        esc_attr($fingerprint)
    );
    $html .= '<span class="dmt-text">' . esc_html($text) . '</span>';
    if ($settings['names'] && $summary['names']) {
        $html .= '<span class="dmt-names">' . esc_html(sprintf(
            /* translators: %s: list of service names */
            __('Affected: %s', 'dumbmonit-status-banner'),
            wp_sprintf('%l', $summary['names'])
        )) . '</span>';
    }
    if ($settings['link']) {
        $url = untrailingslashit($settings['base']) . '/s/' . rawurlencode($settings['slug']);
        $title = $summary['title'] !== '' ? ' title="' . esc_attr($summary['title']) . '"' : '';
        $html .= '<a class="dmt-link" href="' . esc_url($url) . '" target="_blank" rel="noopener"' . $title . '>'
            . esc_html__('Details', 'dumbmonit-status-banner') . '</a>';
    }
    if ($settings['dismissible']) {
        $html .= '<button type="button" class="dmt-close" aria-label="' . esc_attr__('Dismiss', 'dumbmonit-status-banner') . '">&times;</button>';
    }
    $html .= '</div>';
    if ($settings['dismissible']) {
        $html .= dmt_dismiss_script($id);
    }
    return $html;
}

/** The bar on every page of the site, when the placement is top or bottom. */
function dmt_auto_render(): void
{
    static $done = false;
    if ($done || is_admin() || wp_doing_ajax() || is_feed() || is_embed()) {
        return;
    }
    $settings = dmt_settings();
    if ($settings['display'] !== 'top' && $settings['display'] !== 'bottom') {
        return;
    }
    $done = true;
    // Built from escaped parts in dmt_banner_html().
    echo dmt_banner_html($settings, $settings['display']); // phpcs:ignore WordPress.Security.EscapeOutput.OutputNotEscaped
}

add_action('wp_body_open', 'dmt_auto_render');
add_action('wp_footer', 'dmt_auto_render'); // themes without wp_body_open; runs once

/** yes/no attribute of the shortcode, else the setting. */
function dmt_flag($value, bool $fallback): bool
{
    $value = strtolower(trim(dmt_str($value)));
    if (in_array($value, ['1', 'yes', 'true', 'on'], true)) {
        return true;
    }
    if (in_array($value, ['0', 'no', 'false', 'off'], true)) {
        return false;
    }
    return $fallback;
}

/**
 * [dumbmonit_status page="other-page" show="always" names="yes" link="no" dismissible="no"]
 *
 * Every attribute is optional and overrides the setting. The server stays the
 * one saved in the settings: a post author cannot make the site request
 * another address.
 */
function dmt_shortcode($atts): string
{
    $settings = dmt_settings();
    $atts = shortcode_atts([
        'page' => $settings['slug'],
        'show' => $settings['show'],
        'names' => '',
        'link' => '',
        'dismissible' => '',
    ], is_array($atts) ? $atts : [], 'dumbmonit_status');

    $slug = strtolower(trim(dmt_str($atts['page'])));
    if (!dmt_valid_slug($slug)) {
        return '';
    }
    if ($slug !== $settings['slug']) {
        // The ticked services belong to the page of the settings.
        $settings['components'] = [];
        $settings['slug'] = $slug;
    }
    $show = dmt_str($atts['show']);
    if ($show === 'always' || $show === 'issues') {
        $settings['show'] = $show;
    }
    foreach (['names', 'link', 'dismissible'] as $flag) {
        $settings[$flag] = dmt_flag($atts[$flag], (bool) $settings[$flag]);
    }
    return dmt_banner_html($settings, 'inline');
}

add_shortcode('dumbmonit_status', 'dmt_shortcode');
