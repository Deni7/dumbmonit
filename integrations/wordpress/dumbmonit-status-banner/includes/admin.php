<?php
/**
 * Settings › DumbMonit banner: the settings page (Settings API, manage_options),
 * the "Find status pages" search, and the notice when the page cannot be fetched.
 */

if (!defined('ABSPATH')) {
    exit;
}

const DMT_PAGE = 'dumbmonit-banner';

add_action('admin_menu', function () {
    add_options_page(
        __('DumbMonit status banner', 'dumbmonit-status-banner'),
        __('DumbMonit banner', 'dumbmonit-status-banner'),
        'manage_options',
        DMT_PAGE,
        'dmt_page'
    );
});

add_filter('plugin_action_links_' . plugin_basename(DMT_FILE), function ($links) {
    $url = admin_url('options-general.php?page=' . DMT_PAGE);
    array_unshift($links, '<a href="' . esc_url($url) . '">' . esc_html__('Settings') . '</a>');
    return $links;
});

add_action('admin_enqueue_scripts', function ($hook) {
    if ($hook !== 'settings_page_' . DMT_PAGE) {
        return;
    }
    wp_enqueue_script('dmt-admin', DMT_URL . 'assets/admin.js', [], DMT_VERSION, true);
    wp_localize_script('dmt-admin', 'dmtAdmin', [
        'ajaxUrl' => admin_url('admin-ajax.php'),
        'nonce' => wp_create_nonce('dmt_discover'),
        'i18n' => [
            'searching' => __('Searching…', 'dumbmonit-status-banner'),
            'pick' => __('Pick one of the pages found.', 'dumbmonit-status-banner'),
            'none' => __('No listed public page was found on this server.', 'dumbmonit-status-banner'),
            'choose' => __('Choose a page', 'dumbmonit-status-banner'),
            'failed' => __('The search failed. Try again.', 'dumbmonit-status-banner'),
        ],
    ]);
});

/** "Find status pages": lists the public pages of the address typed on the settings page. */
add_action('wp_ajax_dmt_discover', function () {
    check_ajax_referer('dmt_discover', 'nonce');
    if (!current_user_can('manage_options')) {
        wp_send_json_error(['message' => __('Sorry, you are not allowed to do that.')], 403);
    }
    $raw = isset($_POST['base']) && is_string($_POST['base']) ? wp_unslash($_POST['base']) : '';
    list($base) = dmt_split_address($raw);
    if ($base === '') {
        wp_send_json_error(['message' => __('Invalid DumbMonit address: expected http:// or https:// followed by the server name. It was not saved.', 'dumbmonit-status-banner')]);
    }
    $pages = dmt_discover($base);
    if (is_wp_error($pages)) {
        wp_send_json_error(['message' => $pages->get_error_message()]);
    }
    wp_send_json_success(['base' => $base, 'pages' => $pages]);
});

add_action('admin_notices', function () {
    if (!current_user_can('manage_options')) {
        return;
    }
    $settings = dmt_settings();
    if ($settings['base'] === '' || $settings['slug'] === '') {
        return;
    }
    $error = get_transient(DMT_ERR);
    if (is_string($error) && $error !== '') {
        printf(
            '<div class="notice notice-warning"><p>%s</p></div>',
            esc_html(sprintf(
                /* translators: %s: error detail */
                __('DumbMonit status banner: the status page cannot be fetched (%s). No banner is shown.', 'dumbmonit-status-banner'),
                $error
            ))
        );
    }
});

/** Name of a settings field: dumbmonit_banner[...]. */
function dmt_name(string $key): string
{
    return DMT_OPT . '[' . $key . ']';
}

/** A list of radio buttons in a fieldset. */
function dmt_radios(string $key, string $legend, array $choices, string $current): void
{
    echo '<fieldset><legend class="screen-reader-text">' . esc_html($legend) . '</legend>';
    foreach ($choices as $value => $label) {
        printf(
            '<label><input type="radio" name="%s" value="%s"%s> %s</label><br>',
            esc_attr(dmt_name($key)),
            esc_attr($value),
            checked($current, $value, false),
            esc_html($label)
        );
    }
    echo '</fieldset>';
}

function dmt_field_base(): void
{
    $settings = dmt_settings();
    printf(
        '<input id="dmt-base" class="regular-text code" type="url" name="%s" value="%s" placeholder="https://monit.example.com" autocomplete="off"> ',
        esc_attr(dmt_name('base')),
        esc_attr($settings['base'])
    );
    printf('<button type="button" class="button" id="dmt-find">%s</button>', esc_html__('Find status pages', 'dumbmonit-status-banner'));
    printf('<p class="description">%s</p>', esc_html__('Your DumbMonit server, for example https://monit.example.com. A private (LAN) address works.', 'dumbmonit-status-banner'));
}

function dmt_field_slug(): void
{
    $settings = dmt_settings();
    printf(
        '<input id="dmt-slug" class="regular-text code" type="text" name="%s" value="%s" placeholder="my-page" autocomplete="off" aria-describedby="dmt-find-note"> ',
        esc_attr(dmt_name('slug')),
        esc_attr($settings['slug'])
    );
    printf('<select id="dmt-pages" hidden aria-label="%s"></select>', esc_attr__('Choose a page', 'dumbmonit-status-banner'));
    echo '<p class="description" id="dmt-find-note" aria-live="polite">'
        . esc_html__('Pick a page found on the server, or type its address: the part after /s/ in its link.', 'dumbmonit-status-banner')
        . '</p>';
}

function dmt_field_components(): void
{
    $settings = dmt_settings();
    $doc = $settings['base'] !== '' && $settings['slug'] !== ''
        ? dmt_fetch_status($settings['base'], $settings['slug'], dmt_server_lang(determine_locale()), (int) $settings['ttl'])
        : null;
    if (!is_array($doc)) {
        // Unreachable: the ticked services are kept as they are.
        foreach ($settings['components'] as $key) {
            printf('<input type="hidden" name="%s[]" value="%s">', esc_attr(dmt_name('components')), esc_attr($key));
        }
        printf('<p class="description">%s</p>', esc_html__('Save a reachable page first to choose its services.', 'dumbmonit-status-banner'));
        return;
    }
    echo '<fieldset><legend class="screen-reader-text">' . esc_html__('Services', 'dumbmonit-status-banner') . '</legend>';
    foreach ($doc['groups'] as $group) {
        if ($group['name'] !== '') {
            echo '<p><strong>' . esc_html($group['name']) . '</strong></p>';
        }
        foreach ($group['items'] as $item) {
            if (!dmt_valid_key($item['key'])) {
                continue;
            }
            printf(
                '<label><input type="checkbox" name="%s[]" value="%s"%s> %s</label><br>',
                esc_attr(dmt_name('components')),
                esc_attr($item['key']),
                checked(in_array($item['key'], $settings['components'], true), true, false),
                esc_html($item['label'])
            );
        }
    }
    echo '</fieldset>';
    printf('<p class="description">%s</p>', esc_html__('Only the ticked services count; none ticked means all of them.', 'dumbmonit-status-banner'));
}

function dmt_field_display(): void
{
    dmt_radios('display', __('Placement', 'dumbmonit-status-banner'), [
        'top' => __('Bar at the top of every page', 'dumbmonit-status-banner'),
        'bottom' => __('Bar at the bottom of every page', 'dumbmonit-status-banner'),
        'inline' => __('Only where the [dumbmonit_status] shortcode or the widget is placed', 'dumbmonit-status-banner'),
    ], dmt_settings()['display']);
}

function dmt_field_show(): void
{
    dmt_radios('show', __('When to show', 'dumbmonit-status-banner'), [
        'issues' => __('Only when something is wrong', 'dumbmonit-status-banner'),
        'always' => __('Always', 'dumbmonit-status-banner'),
    ], dmt_settings()['show']);
}

function dmt_field_options(): void
{
    $settings = dmt_settings();
    $flags = [
        'names' => __('Name the affected services', 'dumbmonit-status-banner'),
        'link' => __('Link to the status page', 'dumbmonit-status-banner'),
        'dismissible' => __('Visitors can dismiss the banner', 'dumbmonit-status-banner'),
    ];
    echo '<fieldset><legend class="screen-reader-text">' . esc_html__('Options', 'dumbmonit-status-banner') . '</legend>';
    foreach ($flags as $key => $label) {
        printf(
            '<label><input type="checkbox" name="%s" value="1"%s> %s</label><br>',
            esc_attr(dmt_name($key)),
            checked((bool) $settings[$key], true, false),
            esc_html($label)
        );
    }
    echo '</fieldset>';
}

function dmt_field_ttl(): void
{
    printf(
        '<input id="dmt-ttl" class="small-text" type="number" min="15" max="3600" step="1" name="%s" value="%d">',
        esc_attr(dmt_name('ttl')),
        (int) dmt_settings()['ttl']
    );
    printf('<p class="description">%s</p>', esc_html__('Seconds before DumbMonit is asked again, 15 to 3600.', 'dumbmonit-status-banner'));
}

function dmt_field_colors(): void
{
    $settings = dmt_settings();
    dmt_radios('colors', __('Colours', 'dumbmonit-status-banner'), [
        'status' => __('Follow the status: green, amber, red or blue', 'dumbmonit-status-banner'),
        'custom' => __('Custom colours', 'dumbmonit-status-banner'),
    ], $settings['colors']);
    $colours = [
        'light_bg' => __('Background, light mode', 'dumbmonit-status-banner'),
        'light_fg' => __('Text, light mode', 'dumbmonit-status-banner'),
        'dark_bg' => __('Background, dark mode', 'dumbmonit-status-banner'),
        'dark_fg' => __('Text, dark mode', 'dumbmonit-status-banner'),
    ];
    echo '<p>';
    foreach ($colours as $key => $label) {
        printf(
            '<label style="display:inline-block;margin:8px 16px 0 0"><input type="color" name="%s" value="%s"> %s</label>',
            esc_attr(dmt_name($key)),
            esc_attr(dmt_hex(dmt_str($settings[$key]), dmt_defaults()[$key])),
            esc_html($label)
        );
    }
    echo '</p>';
}

function dmt_field_texts(): void
{
    $settings = dmt_settings();
    // Each field is labelled and filled in grey with the default text it replaces.
    $defaults = [
        'operational' => __('All systems operational', 'dumbmonit-status-banner'),
        'maintenance' => __('Maintenance in progress', 'dumbmonit-status-banner'),
        'incident' => __('Incident in progress', 'dumbmonit-status-banner'),
        /* translators: %d: number of services down or degraded */
        'down' => sprintf(_n('%d service is down', '%d services are down', 2, 'dumbmonit-status-banner'), 2),
    ];
    foreach ($defaults as $state => $label) {
        printf(
            '<p><label for="dmt-text-%1$s">%2$s</label><br><input id="dmt-text-%1$s" class="regular-text" type="text" maxlength="200" name="%3$s" value="%4$s" placeholder="%2$s"></p>',
            esc_attr($state),
            esc_attr($label),
            esc_attr(dmt_name('text_' . $state)),
            esc_attr($settings['text_' . $state])
        );
    }
    printf('<p class="description">%s</p>', esc_html__('Leave a field empty to use the default text, translated for each visitor. {count} is replaced by the number of services down.', 'dumbmonit-status-banner'));
}

add_action('admin_init', function () {
    add_settings_section('dmt_page', __('Status page', 'dumbmonit-status-banner'), '__return_false', DMT_PAGE);
    add_settings_field('dmt_base', __('DumbMonit address', 'dumbmonit-status-banner'), 'dmt_field_base', DMT_PAGE, 'dmt_page', ['label_for' => 'dmt-base']);
    add_settings_field('dmt_slug', __('Page address', 'dumbmonit-status-banner'), 'dmt_field_slug', DMT_PAGE, 'dmt_page', ['label_for' => 'dmt-slug']);
    add_settings_field('dmt_components', __('Services', 'dumbmonit-status-banner'), 'dmt_field_components', DMT_PAGE, 'dmt_page');

    add_settings_section('dmt_display', __('Display', 'dumbmonit-status-banner'), '__return_false', DMT_PAGE);
    add_settings_field('dmt_where', __('Placement', 'dumbmonit-status-banner'), 'dmt_field_display', DMT_PAGE, 'dmt_display');
    add_settings_field('dmt_show', __('When to show', 'dumbmonit-status-banner'), 'dmt_field_show', DMT_PAGE, 'dmt_display');
    add_settings_field('dmt_options', __('Options', 'dumbmonit-status-banner'), 'dmt_field_options', DMT_PAGE, 'dmt_display');
    add_settings_field('dmt_ttl', __('Cache duration', 'dumbmonit-status-banner'), 'dmt_field_ttl', DMT_PAGE, 'dmt_display', ['label_for' => 'dmt-ttl']);

    add_settings_section('dmt_look', __('Colours', 'dumbmonit-status-banner'), '__return_false', DMT_PAGE);
    add_settings_field('dmt_colors', __('Colours', 'dumbmonit-status-banner'), 'dmt_field_colors', DMT_PAGE, 'dmt_look');

    add_settings_section('dmt_words', __('Texts', 'dumbmonit-status-banner'), '__return_false', DMT_PAGE);
    add_settings_field('dmt_texts', __('Texts', 'dumbmonit-status-banner'), 'dmt_field_texts', DMT_PAGE, 'dmt_words');
});

function dmt_page(): void
{
    if (!current_user_can('manage_options')) {
        return;
    }
    echo '<div class="wrap"><h1>' . esc_html__('DumbMonit status banner', 'dumbmonit-status-banner') . '</h1>';
    echo '<form method="post" action="options.php">';
    settings_fields('dmt_group');
    do_settings_sections(DMT_PAGE);
    submit_button();
    echo '</form></div>';
}
