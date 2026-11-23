<?php
/**
 * Plugin Name: DumbMonit Status Banner
 * Description: Shows the state of a DumbMonit status page on your site, in each visitor's language.
 * Version: 2.0.0
 * Requires at least: 6.5
 * Requires PHP: 7.4
 * License: MIT
 * License URI: https://opensource.org/license/mit
 * Text Domain: dumbmonit-status-banner
 * Domain Path: /languages
 */

if (!defined('ABSPATH')) {
    exit;
}

define('DMT_VERSION', '2.0.0');
define('DMT_FILE', __FILE__);
define('DMT_DIR', plugin_dir_path(__FILE__));
define('DMT_URL', plugin_dir_url(__FILE__));

/** The plugin's settings, one option (the same name as in 1.x). */
const DMT_OPT = 'dumbmonit_banner';
/** Last fetch error, for the admin notice. */
const DMT_ERR = 'dmt_last_error';

require_once DMT_DIR . 'includes/i18n.php';
require_once DMT_DIR . 'includes/settings.php';
require_once DMT_DIR . 'includes/client.php';
require_once DMT_DIR . 'includes/render.php';

if (is_admin()) {
    require_once DMT_DIR . 'includes/admin.php';
}

add_action('widgets_init', function () {
    require_once DMT_DIR . 'includes/widget.php';
    register_widget('DMT_Status_Widget');
});
