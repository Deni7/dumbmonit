# Keycloak (beta)

Si Keycloak peut vraiment connecter les gens : disponibilité avec le contrôle de la base, version, realms (un realm désactivé, c'est une population verrouillée dehors) et, avec un compte de service, sessions actives et utilisateurs par realm.

!!! note "Bêta"
    Écrit d'après la documentation d'API de l'éditeur et testé sur des réponses réalistes enregistrées, pas encore sur une instance réelle. Dites-nous ce que vous observez.

## Ports réseau à ouvrir

Du serveur DumbMonit (ou de son agent relais) vers l'équipement :

| Port | Protocole | Usage |
|---|---|---|
| 8080 | TCP | API (HTTP, par défaut) |
| 9000 | TCP | Contrôles de santé sur le port de gestion (Keycloak 25 et suivants) |
| 443 | TCP | Seulement avec le schéma https derrière un reverse proxy |

La panne à surveiller : **Keycloak debout, base de données tombée**. La page de connexion s'affiche encore, personne ne peut se connecter, et toutes les applications derrière Keycloak semblent cassées d'un coup. `/health/ready` est l'endroit où Keycloak le dit ; un `503` y est une mesure, pas une erreur de transport.

## Ce qu'il surveille

Toutes les métriques sont préfixées `dumbmonit_keycloak_`.

| Métrique | Quoi | Labels |
|---|---|---|
| `ready`, `live` | 1 tant que le contrôle de disponibilité / de vie indique UP |  |
| `health_check_up` | chaque contrôle de disponibilité (base de données…), 32 au plus | `check` |
| `admin_readable` | 1 quand l'API d'administration a accepté le jeton du compte de service, 0 quand elle l'a refusé (absent sans identifiants) |  |
| `version_info` | valeur 1 (seulement avec `manage-realm`, voir plus bas) | `version` |
| `uptime_seconds`, `memory_used_percent` | temps depuis le démarrage ; mémoire JVM utilisée (seulement avec `manage-realm`) |  |
| `realms_total`, `realms_disabled` | realms, et ceux désactivés |  |
| `realm_sessions`, `realm_offline_sessions`, `realm_users` | sessions actives, sessions hors ligne et utilisateurs de chaque realm lu ; pas de nombre de sessions pour un realm dont le compte ne voit pas les clients | `realm` |
| `sessions_active` | sessions actives, tous les realms lus |  |
| `scrape_errors` | appels qui ont échoué à la dernière sonde |  |

Les [règles intégrées](../alerting/rules.md#self-hosted-applications) qui s'appliquent : Keycloak not ready, Keycloak realm disabled, plus Appareil inaccessible.

## Laisser DumbMonit lire la santé et les realms de Keycloak

1. Lancez Keycloak avec les contrôles de santé activés, avec l'option ci-dessous. Depuis Keycloak 25, ils répondent sur le port de gestion 9000.

    ```
    KC_HEALTH_ENABLED=true
    ```

2. Facultatif, pour les realms, sessions et utilisateurs : dans le realm master, créez un client nommé comme suit, avec Client authentication et Service accounts roles activés, et Standard flow désactivé.

    ```
    dumbmonit
    ```

3. Sur l'onglet Client scopes de ce client, choisissez Add client scope et ajoutez en Default le scope nommé comme suit. Depuis Keycloak 26.8.2, l'API d'administration refuse les jetons qui ne l'ont pas.

    ```
    kc-admin-api
    ```

4. Sur l'onglet Service accounts roles de ce client, attribuez les rôles ci-dessous du client de chaque realm à lire (master-realm, ou homelab-realm pour un realm nommé homelab), puis copiez le secret de l'onglet Credentials.

5. Dans DumbMonit, saisissez l'adresse de Keycloak et l'identifiant et le secret du client, ou choisissez Aucun (contrôles de santé seulement).

### Droits requis

- **view-realm** : par realm : les realms et leur état
- **view-users** : par realm : le nombre d'utilisateurs
- **view-clients** : par realm : le nombre de sessions. Sans lui, Keycloak les cache, et DumbMonit signale une erreur au lieu d'un zéro
- **manage-realm (facultatif)** : sur master-realm seulement : version, durée de fonctionnement et mémoire. Un rôle puissant : ne le donnez que si vous en avez besoin

Dans le realm master, chaque realm a son propre client qui porte ses rôles d'administration : `master-realm` pour master lui-même, `homelab-realm` pour un realm nommé `homelab`, etc. Les rôles de `master-realm` seuls ne montrent que le realm master. Filtrez la fenêtre Assign role par clients pour les trouver.

Ce qui se passe quand il manque quelque chose :

- **Pas de scope `kc-admin-api`** (Keycloak 26.8.2 et suivants) : Keycloak délivre le jeton, puis son API d'administration répond 401 et journalise « Expected audience not available in the token ». `admin_readable` vaut 0, `scrape_errors` compte l'appel refusé et le journal du serveur nomme le scope à ajouter. Keycloak crée `kc-admin-api` dans chaque realm lors de la mise à jour et ne le donne qu'à ses propres clients (`admin-cli`, `security-admin-console`).
- **Pas de `view-clients`** : Keycloak répond aux statistiques de sessions par une liste vide, comme pour un realm où personne n'est connecté. DumbMonit vérifie si le compte voit les clients du realm : sinon, il ne publie aucun nombre de sessions pour ce realm et compte une erreur, plutôt qu'un faux zéro.
- **Pas de `manage-realm`** : depuis Keycloak 26.4, `/admin/serverinfo` ne donne la version, la durée de fonctionnement et la mémoire qu'à un compte du realm master qui a `manage-realm`. Sans lui, ces trois métriques sont simplement absentes ; tout le reste fonctionne.

!!! warning
    Keycloak 24 et antérieurs servent les contrôles de santé sur le port principal : réglez l'option de port de santé sur same. Les contrôles de santé doivent être activés sur le serveur, sinon DumbMonit signale un /health/ready absent.

## Identifiants

| Identifiant | Champs |
|---|---|
| Aucun (contrôles de santé seulement) | Rien n'est envoyé ; les contrôles de santé sont publics. |
| Identifiant / secret du client | Le client de compte de service. DumbMonit demande un jeton (`client_credentials`) sur `/realms/<realm>/protocol/openid-connect/token` à chaque sonde. |

Adresse : le nom d'hôte sur lequel répond Keycloak (`sso.lan`), `host:port`, ou une URL complète (`https://sso.example.com`, avec `/auth` sur les anciennes installations). Les contrôles de santé sont lus sur le même hôte, sur le port de santé. Options : protocole, port, port de santé (`health_port`, 9000 ; `same` pour Keycloak 24 et antérieurs), protocole de santé (`health_scheme`), realm du client (`auth_realm`, master), realms à lire (`realms`), vérification du certificat et délai de requête (15 s).
