<?php
if (!defined('WP_UNINSTALL_PLUGIN')) {
    exit;
}

delete_option('dumbmonit_banner');

// Cached statuses (dmt_s_<hash>) and the last error (dmt_last_error).
global $wpdb;
$wpdb->query($wpdb->prepare(
    "DELETE FROM {$wpdb->options} WHERE option_name LIKE %s OR option_name LIKE %s",
    $wpdb->esc_like('_transient_dmt_') . '%',
    $wpdb->esc_like('_transient_timeout_dmt_') . '%'
));
