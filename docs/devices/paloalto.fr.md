# Palo Alto PAN-OS (pack communautaire)

Un pare-feu Palo Alto Networks (PA-Series, VM-Series) par l'API XML de PAN-OS :
interfaces sans lien, pair HA perdu ou désynchronisé, table des sessions qui se
remplit, processeur et mémoire du plan de gestion, licences qui expirent.

!!! note "Pack communautaire"
    PAN-OS n'est plus intégré à DumbMonit : il est livré en pack communautaire,
    [`packs/paloalto`](https://github.com/selfhostr/dumbmonit/tree/main/packs/paloalto),
    que chacun peut corriger sans attendre une version. Installez-le dans
    **Réglages → Packs d'intégration** (collez ou envoyez `pack.yaml`), puis
    ajoutez une surveillance de type **Palo Alto PAN-OS**. Voir les
    [packs d'intégration](../packs/index.md).

    Les surveillances PAN-OS existantes ont pris le type du pack avec leur
    identifiant. Tant que le pack n'est pas installé, elles affichent *Install
    the community pack "paloalto"*. Le pack n'accepte qu'une **clé d'API** : une
    surveillance qui utilisait un nom et un mot de passe a besoin d'une clé
    générée pour ce compte (étape 3 ci-dessous). La dernière validation
    (commit) n'est plus surveillée.

    Les sauvegardes de configuration PAN-OS ([configuration réseau](netconfig.md))
    ne changent pas.

## Ports réseau à ouvrir

Du serveur DumbMonit (ou de son agent relais) vers l'équipement :

| Port | Protocole | Usage |
|---|---|---|
| 443 | TCP | Interface de gestion (HTTPS, API XML) |

## Mise en place

1. **Device → Admin Roles → Add** : un rôle dont l'onglet XML API n'ouvre que
   *Operational Requests* ; Web UI et REST API désactivés.
2. **Device → Administrators → Add** : un administrateur `dumbmonit` avec ce
   rôle et un long mot de passe aléatoire, puis **Commit**.
3. Générez sa clé d'API depuis l'hôte DumbMonit :
   `curl -k -X POST https://firewall.lan/api/ -d type=keygen -d user=dumbmonit --data-urlencode password=<mot de passe>`
4. Dans DumbMonit, saisissez l'adresse du pare-feu et collez la clé (entre
   `<key>` et `</key>`). Activez *Accepter un certificat invérifiable* tant que
   le pare-feu garde son certificat d'origine.

La clé part dans l'en-tête `X-PAN-KEY`, jamais dans l'URL.

## Ce qui est surveillé

Toutes les métriques sont préfixées `dumbmonit_paloalto_`. Seul
`show system info` est obligatoire ; les autres lectures sont facultatives.

| Métrique | Quoi | Labels |
|---|---|---|
| `version_info` | valeur 1 | `version`, `model`, `system_name` |
| `cpu_idle_percent`, `memory_total_kib`, `memory_free_kib`, `memory_cache_kib` | plan de gestion, d'après `top` | |
| `sessions`, `sessions_max`, `session_setup_rate`, `packets_per_second`, `throughput_kbps` | table des sessions et trafic | |
| `ha_enabled`, `ha_local_functional`, `ha_peer_up`, `ha_peer_connected`, `ha_in_sync` | grappe HA | |
| `license_expired`, `license_expiration_timestamp_seconds` | par licence ; *Never* n'a pas de date | `license` |
| `interface_link_up`, `interface_speed_mbps` | par port physique | `interface` |
| `interface_info` | valeur 1 par interface logique | `interface`, `zone` |

## Règles d'alerte

Installées avec le pack, sous l'uid `pack:paloalto:<nom>` :

| Règle | Se déclenche quand |
|---|---|
| `interface_down` | une interface rattachée à une zone reste sans lien 5 minutes |
| `ha_peer_lost` | le pair HA n'est ni actif ni passif, ou son lien est coupé (2 minutes) |
| `ha_out_of_sync` | la configuration HA reste désynchronisée 15 minutes |
| `ha_not_functional` | ce pare-feu est suspendu ou non fonctionnel dans sa grappe (5 minutes) |
| `session_table_full` | la table des sessions reste au-dessus de 90 % pendant 10 minutes |
| `cpu_high`, `memory_high` | le plan de gestion reste au-dessus de 90 % pendant 15 minutes |
| `license_expiring` | une licence expire dans moins de 30 jours, ou a expiré |
