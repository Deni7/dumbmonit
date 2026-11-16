# Cisco Meraki (pack communautaire)

Une organisation Cisco Meraki par l'API Dashboard : appareils hors ligne,
liaisons Internet en échec, licence hors conformité ou qui expire.

!!! note "Pack communautaire"
    Meraki n'est plus intégré à DumbMonit : il est livré en pack communautaire,
    [`packs/meraki`](https://github.com/selfhostr/dumbmonit/tree/main/packs/meraki),
    que chacun peut corriger sans attendre une version. Installez-le dans
    **Réglages → Packs d'intégration** (collez ou envoyez `pack.yaml`), puis
    ajoutez une surveillance de type **Cisco Meraki**. Voir les
    [packs d'intégration](../packs/index.md).

    Les surveillances Meraki existantes ont pris le type du pack avec leur clé
    d'API. Tant que le pack n'est pas installé, elles affichent *Install the
    community pack "meraki"*. L'option **Organization** attend désormais
    l'identifiant numérique de l'organisation (pas son nom), et le filtre de
    réseaux disparaît.

## Ports réseau à ouvrir

Du serveur DumbMonit vers Internet :

| Port | Protocole | Usage |
|---|---|---|
| 443 | TCP | API Dashboard Meraki (`api.meraki.com`, ou le tableau de bord du pays : `api.meraki.ca`, `api.meraki.cn`, `api.meraki.in`, `api.gov-meraki.com`) |

## Mise en place

1. Dans le tableau de bord Meraki, **Organization → Settings** : cochez *Enable
   access to the Cisco Meraki Dashboard API*.
2. Créez un administrateur d'organisation en lecture seule pour DumbMonit,
   connectez-vous avec lui et générez une clé d'API dans **My profile → API
   access**.
3. Trouvez l'identifiant de l'organisation : `curl -H "Authorization: Bearer <clé>" https://api.meraki.com/api/v1/organizations`.
4. Dans DumbMonit, saisissez `api.meraki.com` comme adresse, collez la clé et
   l'identifiant de l'organisation.

Un pack ne parle qu'à l'adresse de la surveillance. Si le tableau de bord
redirige votre organisation vers un autre serveur, l'erreur le nomme
(`n123.meraki.com`) : saisissez ce serveur comme adresse.

## Ce qui est surveillé

Toutes les métriques sont préfixées `dumbmonit_meraki_`.

| Métrique | Quoi | Labels |
|---|---|---|
| `device_online`, `device_offline`, `device_alerting` | 1 ou 0 par appareil ; un appareil dormant n'en a pas | `device`, `serial`, `model`, `product_type`, `network_id` |
| `device_last_report_timestamp_seconds` | dernier rapport de l'appareil | idem |
| `uplink_up`, `uplink_failed` | par liaison d'un routeur ; `not connected` (une prise vide) n'est pas une panne | `serial`, `network_id`, `interface` |
| `license_ok`, `license_expiration_timestamp_seconds` | licence co-terminée : état OK, date d'expiration | |
| `licenses_expired` | licences par appareil : celles qui ont expiré | |

## Règles d'alerte

Installées avec le pack, sous l'uid `pack:meraki:<nom>` :

| Règle | Se déclenche quand |
|---|---|
| `device_offline` | un appareil reste hors ligne 10 minutes |
| `uplink_failed` | une liaison configurée perd Internet pendant 5 minutes |
| `license_problem` | la licence est hors conformité, ou des licences par appareil ont expiré (1 heure) |
| `license_expiring` | la licence co-terminée expire dans moins de 30 jours |
