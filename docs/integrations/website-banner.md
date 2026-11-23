# Website banner

Show a banner on your own website when services are down ("2 services are
down"), driven by a published [status page](../using/status-pages.md). When
everything is fine the banner stays hidden, unless you ask for it to be shown
("All systems operational").

Three ways to add it, from simplest to most controlled. Ready-to-copy files are
in the `integrations/` folder of the repository.

In DumbMonit, open the status page's settings (Status pages, Edit): the **Status banner for your website** section at the very bottom has the ready-filled HTML and PHP snippets with a Copy button, a link to this page, and a button to download the WordPress plugin as a `.zip` ready to upload.

## HTML snippet

Paste this before `</body>`, with your server address and the page's address:

```html
<script async
  src="https://monit.example.com/api/public/status/my-page/banner.js"
  data-position="top" data-show="issues" data-lang="en"></script>
```

| Attribute | Values | Default |
|---|---|---|
| `data-position` | `top`, `bottom` | `top` |
| `data-show` | `issues` (only when something is wrong), `always` | `issues` |
| `data-lang` | `en`, `fr`, `de`, `es`, `it`, `pt`, `pt-BR`, `ru`, `zh-Hans` | page language, else `en` |

The script has no dependencies, draws inside a Shadow DOM so your styles do not
touch it, follows the visitor's light or dark preference and can be dismissed
(until the browser tab is closed). It reads the page's public JSON, which the
server allows any website to fetch (`Access-Control-Allow-Origin: *`, no
cookies). Only the status page's public document is opened this way; the rest
of the API keeps its rules.

## WordPress

The plugin needs WordPress 6.5 or later and PHP 7.4 or later.

1. Download the `.zip` from the page's settings in DumbMonit and upload it in
   **Plugins > Add New > Upload Plugin**, or copy
   `integrations/wordpress/dumbmonit-status-banner` into `wp-content/plugins/`.
   Activate it.
2. Open **Settings > DumbMonit banner** and type the address of your DumbMonit
   server (`https://monit.example.com`).
3. Click **Find status pages** and pick a page in the list. The list comes from
   `GET /api/public/status`: the published pages anyone may read, except those
   whose **Show in the list of public pages** switch is off in DumbMonit. You
   can also type the page's address (the part after `/s/` in its link), or
   paste the link of the page into the first field.
4. Choose the other settings and save. The page is fetched right away; an
   error is shown if it cannot be.

| Setting | Choices |
|---|---|
| Placement | A bar at the top or at the bottom of every page, or only where the `[dumbmonit_status]` shortcode or the **DumbMonit status** widget is placed. |
| When to show | Only when something is wrong, or always ("All systems operational"). |
| Services | Tick the services that count for the banner; none ticked means all of them. |
| Options | Name the affected services; link to the status page; let visitors dismiss the banner (a cookie hides it for a day, until something else goes wrong). |
| Cache duration | 15 to 3600 seconds (default 60) before DumbMonit is asked again. |
| Colours | Follow the status (green, amber, red, blue, light and dark), or your own colours for light and dark mode. |
| Texts | Your own sentence for each state; `{count}` is replaced by the number of services down. An empty field keeps the translated default. |

### Languages

The banner follows each visitor's language with no setting: the WordPress
language of the page being viewed, or the current language of Polylang or
WPML. That language is sent to DumbMonit as `?lang=`, so the service names you
translated in the page's [languages](../using/status-pages.md#languages) come
back in it. The plugin's own texts ship in English, German, Spanish, French,
Italian, Portuguese (Portugal and Brazil), Russian, Chinese (Simplified),
Japanese, Korean, Dutch, Polish and Turkish, as `languages/*.l10n.php` files
read directly by WordPress, with their `.po` sources and a `.pot` template for
new translations. A regional variant uses its language (`fr_CA` reads
`fr_FR`). Your own texts are registered with Polylang (**Languages >
Translations**) and WPML (**String Translation**) so they can be translated
there too.

### Shortcode

`[dumbmonit_status]` prints the banner inline. Its attributes are optional and
override the settings: `page` (another page of the same server), `show`
(`issues` or `always`), and `names`, `link`, `dismissible` (`yes` or `no`):

```text
[dumbmonit_status page="shop" show="always" link="no"]
```

The server is always the one in the settings, so a post author cannot make
the site request another address.

### Safety

Everything printed is escaped. The plugin requests only the address saved by
an administrator (an `http` or `https` URL, re-checked before each request,
redirects disabled, 3-second timeout); a private (LAN) address works. If
DumbMonit cannot be reached, no banner is shown and the page loads normally;
the failure is cached for a short while so it never slows page views down,
and a warning is shown in the WordPress admin. The settings page is limited to
administrators (`manage_options`) and its forms carry WordPress nonces.

Settings saved by version 1.x are read as they are: the old status page URL
becomes the server address and the page address.

## PHP include

For any PHP site, without a framework:

```php
require __DIR__ . '/banner.php';
dumbmonit_banner('https://monit.example.com/api/public/status/my-page',
    ['position' => 'top', 'show' => 'issues', 'ttl' => 60]);
```

The helper accepts `http` and `https` URLs only, uses a 3-second timeout, does
not follow redirects, keeps a file cache in the temporary directory and prints
nothing if the server cannot be reached (the failure is written to the PHP error
log). Texts are in English by default; pass `'lang' => 'fr'` or a `'strings'`
array to change them. A private (LAN) address works. See `integrations/php/example.php`.
