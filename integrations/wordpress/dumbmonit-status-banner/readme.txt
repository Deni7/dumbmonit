=== DumbMonit Status Banner ===
Contributors: dumbmonit
Tags: status, uptime, monitoring, banner, multilingual
Requires at least: 6.5
Tested up to: 6.8
Requires PHP: 7.4
Stable tag: 2.0.0
License: MIT
License URI: https://opensource.org/license/mit

Shows the state of a DumbMonit status page on your site, in each visitor's language.

== Description ==

The plugin reads the public document of a published DumbMonit status page on the server (never from the visitor's browser), keeps it in a transient for the cache duration you choose, and prints a banner such as "2 services are down", or "All systems operational" if you choose to always show it.

* **Automatic language.** The banner follows the language of each visitor: the site language, or the current language of Polylang or WPML. That language is also sent to DumbMonit, so the page's translated service names come back. The plugin ships translations for German, Spanish, French, Italian, Portuguese (Portugal and Brazil), Russian, Chinese (Simplified), Japanese, Korean, Dutch, Polish and Turkish.
* **Find status pages.** Type the address of your DumbMonit server and click "Find status pages": the pages the server lists publicly appear in a list to pick from.
* **Placement.** A bar at the top or bottom of every page, or only where you place the `[dumbmonit_status]` shortcode or the "DumbMonit status" widget.
* **Choices.** Show always or only when something is wrong; count only some services; name the affected services; link to the status page; let visitors dismiss the banner (for a day, until something else goes wrong); colours that follow the status or your own light and dark colours; your own texts for each state (registered with Polylang and WPML for translation).
* **Safe.** Everything printed is escaped. If DumbMonit cannot be reached, nothing is shown and the page loads as usual; the failure is cached too, so an unreachable server never slows your pages down, and a notice tells the administrators.

== Installation ==

1. In DumbMonit, open the status page's settings and download the WordPress plugin, or copy the `dumbmonit-status-banner` folder to `wp-content/plugins/`. Activate it.
2. Go to Settings > DumbMonit banner.
3. Enter the address of your DumbMonit server (for example `https://monit.example.com`), click "Find status pages" and pick a page. You can also type the page address, the part after `/s/` in its link, or paste the link of the page.
4. Choose the placement and the other options, then save.

The DumbMonit server can be on a private (LAN) address.

== Shortcode ==

`[dumbmonit_status]` shows the banner inline. Every attribute is optional and overrides the settings:

* `page` – another page of the same DumbMonit server, by its address.
* `show` – `issues` (only when something is wrong) or `always`.
* `names`, `link`, `dismissible` – `yes` or `no`.

The server is always the one of the settings: a post author cannot make the site request another address.

== Frequently Asked Questions ==

= The banner is in the wrong language =

The banner uses WordPress's language for the page being viewed. With Polylang or WPML, it is the language of the current page. A page cache keeps one copy per address, so a multilingual site should keep languages on separate addresses (the usual setting of both plugins).

= Can I translate my own texts? =

Yes. With Polylang (Languages > Translations) or WPML (String Translation), your custom texts are listed under "DumbMonit status banner". Without them, a custom text is shown as written.

== Changelog ==

= 2.0.0 =
* Requires WordPress 6.5.
* The banner follows each visitor's language (site language, Polylang or WPML), and asks DumbMonit for the page's texts in that language.
* Translations included for 13 languages, as `.l10n.php` files with their `.po` sources.
* "Find status pages": lists the public pages of a DumbMonit server to pick from.
* New settings: placement (top, bottom, shortcode or widget only), services that count, names of affected services, link, dismissible banner, colours, custom texts per state, cache duration.
* `[dumbmonit_status]` shortcode and "DumbMonit status" widget.
* An unreachable server is remembered for a short while, so it no longer slows down page views.
* Settings from 1.x are read as they are (the status page URL becomes server address and page address).

= 1.1.0 =
* Works with a DumbMonit server on a private/LAN address.
* Admin notice when the status page cannot be fetched; error when the URL is invalid.
* Banner strings are translatable (text domain dumbmonit-status-banner).

= 1.0.0 =
* First release.
