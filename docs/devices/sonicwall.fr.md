# SonicWall SonicOS (pack communautaire)

Un pare-feu SonicWall par l'API de SonicOS 7 : le lien de chaque interface
rattachée à une zone. Volontairement minimal : seuls les chemins d'API que
SonicWall publie sont lus.

!!! note "Pack communautaire"
    SonicWall n'est plus intégré à DumbMonit : il est livré en pack
    communautaire,
    [`packs/sonicwall`](https://github.com/selfhostr/dumbmonit/tree/main/packs/sonicwall),
    écrit sans équipement pour l'essayer, que chacun peut corriger sans
    attendre une version. Installez-le dans **Réglages → Packs d'intégration**
    (collez ou envoyez `pack.yaml`), puis ajoutez une surveillance de type
    **SonicWall SonicOS**. Voir les [packs d'intégration](../packs/index.md).

    Les surveillances SonicWall existantes ont pris le type du pack avec leur
    nom d'utilisateur et leur mot de passe. Tant que le pack n'est pas installé,
    elles affichent *Install the community pack "sonicwall"*.

## Ports réseau à ouvrir

Du serveur DumbMonit (ou de son agent relais) vers l'équipement :

| Port | Protocole | Usage |
|---|---|---|
| 443 | TCP | Interface de gestion (HTTPS, API SonicOS) |

## Mise en place

1. **Device → Settings → Administration → Audit/SonicOS API** : activez l'API
   SonicOS et *RFC-2617 HTTP Basic Access authentication*.
2. **Device → Users → Local Users & Groups → Add User** : `dumbmonit`, un long
   mot de passe aléatoire, membre de *SonicWall Read-Only Admins*.
3. Autorisez la gestion HTTPS sur l'interface par laquelle DumbMonit joint le
   pare-feu.
4. Dans DumbMonit, saisissez l'adresse du pare-feu avec ce nom d'utilisateur et
   ce mot de passe.

Chaque sonde ouvre une session d'API (`POST /api/sonicos/auth`), lit, puis la
referme toujours (`DELETE`) : SonicOS n'admet qu'une session d'API à la fois.

## Ce qui est surveillé

Toutes les métriques sont préfixées `dumbmonit_sonicwall_`.

| Métrique | Quoi | Labels |
|---|---|---|
| `version_info` | valeur 1 | `version`, `model` |
| `interface_link_up` | 1 quand l'interface a un lien, 0 pour *No link* | `interface`, `zone` |

## Règles d'alerte

Installées avec le pack, sous l'uid `pack:sonicwall:<nom>` :

| Règle | Se déclenche quand |
|---|---|
| `interface_down` | une interface rattachée à une zone (hors *Unassigned*) reste sans lien 5 minutes |
