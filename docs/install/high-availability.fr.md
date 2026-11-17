# Haute disponibilité (actif/passif)

Un second serveur DumbMonit peut attendre sur une autre machine et prendre le
relais quand le premier a disparu. C'est désactivé par défaut : sans
`DUMBMONIT_HA_DIR`, rien de ce qui suit ne se produit, et un serveur seul reste
le choix le plus simple pour un homelab. Ne l'activez que si une heure sans
surveillance est un vrai problème.

**Un seul serveur est actif à la fois, toujours.** Le serveur en attente n'ouvre
ni sa base ni aucun port et ne sonde rien tant qu'il n'est pas devenu actif.

## Ce qu'il faut

- **Deux machines** avec **la même version de DumbMonit**. Mettez-les à jour
  ensemble : une base écrite par une version plus récente ne démarre pas sur
  une plus ancienne.
- **Un VictoriaMetrics externe** que les deux serveurs joignent
  (`DUMBMONIT_VM_URL`, voir
  [VictoriaMetrics externe](../reference/external-victoriametrics.md)), sur une
  troisième machine ou en grappe. Avec celui qui est embarqué, chaque serveur
  garde ses propres métriques et l'historique ne suit pas une bascule (le
  serveur le dit dans son journal).
- **Un répertoire partagé** où les deux serveurs écrivent : un partage NFS ou
  SMB, ou tout volume monté par les deux hôtes. Il contient le bail et les
  copies de la base (quelques mégaoctets), rien d'autre. Le serveur tourne sous
  l'uid 65532 (ou `PUID`) : donnez-lui le droit d'y écrire
  (`chown 65532:65532` sur le partage, ou `anonuid`/`all_squash` en NFS).
  Gardez-le sur un stockage fiable : tant qu'il est injoignable, rien ne peut
  changer de mains. Protégez-le comme `/data` : il contient une copie de la base
  (configuration, empreintes des mots de passe ; les identifiants restent
  chiffrés par le secret d'instance, qui n'y est jamais écrit).
- **Le même secret d'instance des deux côtés** : soit `DUMBMONIT_SECRET` avec
  la même valeur, soit une copie de `/data/secret.key` du premier serveur posée
  dans le volume du second **avant son premier démarrage**. Le serveur en
  attente vérifie chaque copie de la base avec son propre secret et refuse de
  prendre le relais s'il ne peut pas la déchiffrer.
- **Des agents qui connaissent les deux adresses** : `server_urls` dans leur
  `agent.yaml` (voir [Serveurs de secours](../devices/agent.md#backup-servers)).

## Comment ça marche

- **Le bail.** Un petit fichier du répertoire partagé dit quel serveur est
  actif. L'actif le renouvelle toutes les 10 secondes, et le relit d'abord : si
  un autre serveur le tient, il s'arrête aussitôt (code de sortie 75) et le
  conteneur le redémarre en serveur en attente.
- **La réplique.** Toutes les 30 secondes (`DUMBMONIT_HA_SNAPSHOT_SECS`), quand
  la base a changé, l'actif en écrit une copie cohérente (`VACUUM INTO`, comme
  les [sauvegardes locales planifiées](backup.md)) dans le répertoire partagé,
  sous son propre bail seulement. Le serveur en attente recopie chaque nouvelle
  copie, vérifie son SHA-256, son intégrité et que son secret la déchiffre, et
  la garde prête.
- **La prise de relais.** Le serveur en attente devient actif quand vous le
  promouvez, ou de lui-même après `DUMBMONIT_HA_TAKEOVER_MINUTES` sans
  battement. Il prend le bail, attend 15 secondes et le relit (si deux serveurs
  essaient en même temps, un seul gagne), met son ancienne base de côté dans
  `/data/ha/previous.db`, installe la dernière réplique et démarre comme
  n'importe quel serveur : sondes, alertes, port de l'interface et port des
  agents.
- **Ce qui suit la réplique.** Tout ce qui est dans la base : surveillances,
  règles et états d'alerte, comptes, canaux, identifiants, **l'autorité des
  agents** (CA) et le certificat HTTPS autosigné, le tout chiffré par le secret
  d'instance. Les agents joignent donc le nouvel actif avec le certificat
  qu'ils ont déjà : pas de réenrôlement.
- **Ce qui peut être perdu.** Ce qui a changé sur l'ancien actif après sa
  dernière réplique : au plus `DUMBMONIT_HA_SNAPSHOT_SECS` de configuration et
  d'état d'alerte. Les métriques vivent dans VictoriaMetrics, et les agents
  gardent leurs mesures tant qu'aucun serveur ne répond, puis les envoient au
  nouveau.
- **Arrêt propre.** Sur `docker compose stop`, l'actif écrit une dernière
  réplique et rend le bail. Le serveur de rôle `primary` reprend aussitôt un
  bail rendu ; le serveur en attente, lui, attend son délai de reprise : un
  simple redémarrage du primaire ne déplace rien.

## Réglages

| Variable | Défaut | Sens |
|---|---|---|
| `DUMBMONIT_HA_DIR` | *(absente : désactivé)* | Répertoire partagé, chemin absolu dans le conteneur (`/ha`). |
| `DUMBMONIT_HA_NODE` | *(obligatoire)* | Nom de ce serveur, différent sur chacun : 1 à 32 minuscules, chiffres, `-` ou `_` (`a`, `b`). |
| `DUMBMONIT_HA_ROLE` | *(obligatoire)* | `primary` sur le serveur qui tourne d'habitude, `standby` sur l'autre. |
| `DUMBMONIT_HA_TAKEOVER_MINUTES` | *(absente : manuel)* | Reprise automatique après ce nombre de minutes sans battement (2 au moins). L'actif s'arrête de lui-même après la moitié de ce délai sans renouvellement réussi : il est parti avant que le secours ne prenne la main. |
| `DUMBMONIT_HA_SNAPSHOT_SECS` | `30` | Période de publication de la réplique par l'actif, de 10 à 3600 secondes. |

## Exemple à deux nœuds

L'hôte C fait tourner VictoriaMetrics, le NAS exporte `/export/dumbmonit-ha`
en NFS. Sur l'hôte A :

```yaml
name: dumbmonit
services:
  dumbmonit:
    image: ghcr.io/selfhostr/dumbmonit:latest
    ports:
      - "8080:8080"
      - "8443:8443"
    environment:
      DUMBMONIT_VM_URL: http://monitor-c.lan:8428
      DUMBMONIT_SECRET: ${DUMBMONIT_SECRET}   # même valeur des deux côtés, dans .env
      DUMBMONIT_HA_DIR: /ha
      DUMBMONIT_HA_NODE: a
      DUMBMONIT_HA_ROLE: primary
      # DUMBMONIT_HA_TAKEOVER_MINUTES: "5"    # reprise automatique, facultative
    volumes:
      - dumbmonit-data:/data
      - dumbmonit-ha:/ha
    restart: unless-stopped                  # obligatoire : un serveur destitué redémarre en attente
    stop_grace_period: 30s
    read_only: true
    tmpfs: [/tmp]
    user: "0:0"
    cap_drop: [ALL]
    cap_add: [CHOWN, DAC_READ_SEARCH, SETUID, SETGID]
    security_opt: ["no-new-privileges:true"]

volumes:
  dumbmonit-data:
  dumbmonit-ha:
    driver_opts:
      type: nfs
      o: addr=nas.lan,rw,nfsvers=4.1
      device: ":/export/dumbmonit-ha"
```

Sur l'hôte B, le même fichier avec `DUMBMONIT_HA_NODE: b` et
`DUMBMONIT_HA_ROLE: standby`. Sur l'hôte C :

```yaml
services:
  victoriametrics:
    image: victoriametrics/victoria-metrics:v1.156.0
    command: ["-storageDataPath=/storage", "-retentionPeriod=12"]
    ports:
      - "8428:8428"
    volumes:
      - vm-data:/storage
    restart: unless-stopped

volumes:
  vm-data:
```

Le port 8428 ne doit être joignable que depuis A et B (une règle de pare-feu,
ou un réseau privé) : VictoriaMetrics n'a pas d'authentification à lui.

Démarrez A d'abord et attendez *this server is now the active one* dans son
journal, puis démarrez B : il écrit *standby: waiting*. Sur un serveur
existant, ajoutez-lui les variables (il devient `primary` et garde ses
données), copiez son `/data/secret.key` dans le volume de B, puis démarrez B.

Sous Kubernetes, le chart Helm (`deploy/helm/dumbmonit`) fait tourner un seul
serveur. Pour ce montage, déployez deux releases, chacune avec son propre volume
de données, ces variables et un volume `ReadWriteMany` monté sur
`DUMBMONIT_HA_DIR`.

## Les agents

Ajoutez le serveur de secours à chaque agent :

```yaml
server_url: https://monitor-a.lan:8080
server_urls:
  - https://monitor-b.lan:8080
```

Après trois échecs consécutifs, l'agent passe à l'adresse suivante, avec le
même certificat. Sur un secours, il réessaie la primaire toutes les deux
minutes, et y revient dès qu'elle répond en serveur actif. La page de la
machine affiche **Serveur : principal**, ou **Secours** avec l'adresse et
depuis quand.

## Les commandes du quotidien

À lancer dans le conteneur (`docker compose exec dumbmonit /dumbmonit ha …`) :

- `ha status` : l'état de ce serveur, qui tient le bail et s'il le renouvelle,
  la dernière réplique et tout problème (secret, version, stockage).
- `ha promote` : rendre actif ce serveur en attente. Refusé tant que l'actif
  renouvelle encore son bail ; arrêtez-le d'abord.
- `ha promote --force` : prendre la main bien que l'actif renouvelle son bail.
  Il s'arrête de lui-même à son renouvellement suivant (dans les 10 secondes),
  mais ses changements depuis sa dernière réplique sont perdus. À réserver au
  cas où l'actif est coupé de vous mais pas du répertoire partagé.

La sonde de santé du conteneur dit sain un serveur en attente qui fait son
travail, bien qu'aucun port ne soit ouvert.

### Bascule prévue

1. `docker compose stop` sur l'actif : dernière réplique, bail rendu.
2. `docker compose exec dumbmonit /dumbmonit ha promote` sur l'autre.
3. Redémarrez le premier : il attend en serveur de secours. Pour revenir en
   arrière, faites de même dans l'autre sens.

### Après une panne

Quand l'actif tombe, promouvez le serveur en attente (ou laissez la reprise
automatique le faire). Quand l'ancien serveur revient, il voit que l'autre
tient le bail et attend en serveur de secours, même si son rôle est `primary`.

## Pourquoi pas Litestream ou LiteFS

Les deux ont été évalués (novembre 2026) et restent utilisables autour de
DumbMonit si vous les préférez ; aucun n'est nécessaire.

- **[Litestream](https://litestream.io/)** (v0.5.18) envoie en continu le
  journal WAL de SQLite vers S3, SFTP, un chemin de fichier et d'autres, avec
  une fenêtre de perte plus courte que 30 secondes. Il tourne en processus à
  part (un conteneur voisin qui partage `/data`), prend la main sur les points
  de contrôle du WAL (il demande à l'application de couper les siens) et vous
  laisse garantir qu'un seul serveur écrit ; il n'a ni bail ni promotion, une
  restauration est un `litestream restore` à la main.
- **[LiteFS](https://fly.io/docs/litefs/)** (v0.5.14, avril 2025, toujours en
  bêta) réplique à travers un système de fichiers FUSE : il lui faut
  `/dev/fuse` et `CAP_SYS_ADMIN` dans le conteneur, et Consul pour une élection
  automatique du primaire.

Le mode intégré ne demande ni autre processus, ni privilège de plus, ni
nouvelle dépendance : un répertoire partagé, un bail qui écarte l'ancien actif,
et une copie de la base que le serveur en attente vérifie avant de s'y fier. Sa
fenêtre de perte est la période de réplication ; pour un serveur de
surveillance, dont les métriques vivent ailleurs, c'est le bon compromis.
