<?php
/**
 * The banner as a widget (also usable through the Legacy Widget block). It
 * uses the plugin's settings, shown inline in the widget area.
 */

if (!defined('ABSPATH')) {
    exit;
}

class DMT_Status_Widget extends WP_Widget
{
    public function __construct()
    {
        parent::__construct('dmt_status', __('DumbMonit status', 'dumbmonit-status-banner'), [
            'description' => __('The status of your DumbMonit page, with the settings of the banner.', 'dumbmonit-status-banner'),
        ]);
    }

    public function widget($args, $instance)
    {
        $html = dmt_banner_html(dmt_settings(), 'inline');
        if ($html === '') {
            return;
        }
        $before = isset($args['before_widget']) ? (string) $args['before_widget'] : '';
        $after = isset($args['after_widget']) ? (string) $args['after_widget'] : '';
        // The wrappers come from the theme; the banner is built from escaped parts.
        echo $before . $html . $after; // phpcs:ignore WordPress.Security.EscapeOutput.OutputNotEscaped
    }

    public function form($instance)
    {
        echo '<p>' . esc_html__('This widget uses the settings in Settings › DumbMonit banner.', 'dumbmonit-status-banner') . '</p>';
        // No field of its own: no Save button.
        return 'noform';
    }
}
