# UniFi Protect (pack communautaire)

Les caméras d'une console UniFi Protect, par l'API d'intégration d'UniFi OS :
lesquelles sont déconnectées. À ne pas confondre avec la surveillance UniFi
Network (bornes, commutateurs, passerelle).

!!! note "Pack communautaire"
    UniFi Protect n'est plus intégré à DumbMonit : il est livré en pack
    communautaire, [`packs/unifiprotect`](https://github.com/selfhostr/dumbmonit/tree/main/packs/unifiprotect),
    que chacun peut corriger sans attendre une version. Installez-le dans
    **Réglages → Packs d'intégration** (collez ou envoyez `pack.yaml`), puis
    ajoutez une surveillance de type **UniFi Protect**. Voir les
    [packs d'intégration](../packs/index.md).

    Le pack lit l'API d'intégration officielle avec une **clé d'API**, là où
    l'ancien type intégré se connectait avec un compte local. Les surveillances
    Protect existantes ont pris le type du pack ; une fois le pack installé,
    modifiez chacune et remplacez son nom d'utilisateur et son mot de passe par
    une clé d'API. L'API d'intégration ne publie ni l'état d'enregistrement des
    caméras ni la santé des disques de l'enregistreur : ils ne sont plus
    surveillés.

## Ports réseau à ouvrir

Du serveur DumbMonit (ou de son agent relais) vers l'équipement :

| Port | Protocole | Usage |
|---|---|---|
| 443 | TCP | UniFi OS (HTTPS) |

## Mise en place

1. Dans UniFi OS, **Settings → Control Plane → Integrations** : créez une clé
   d'API nommée DumbMonit et copiez-la (elle n'est montrée qu'une fois).
2. Dans DumbMonit, saisissez l'adresse de la console et collez la clé comme
   clé d'API. Activez *Accepter un certificat invérifiable* tant que la
   console garde son certificat d'origine.

La clé part dans l'en-tête `X-API-KEY`. L'API d'intégration n'a pas de clé en
lecture seule : gardez-la aussi secrète qu'un mot de passe d'administrateur.

## Ce qui est surveillé

Toutes les métriques sont préfixées `dumbmonit_unifiprotect_`.

| Métrique | Quoi | Labels |
|---|---|---|
| `camera_online` | 1 tant que la caméra est connectée, 0 sinon | `camera`, `camera_id` |

## Règles d'alerte

Installées avec le pack, sous l'uid `pack:unifiprotect:<nom>` :

| Règle | Se déclenche quand |
|---|---|
| `camera_offline` | une caméra reste déconnectée 5 minutes |
