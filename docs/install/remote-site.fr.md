# Surveiller un site distant

Un seul DumbMonit, plusieurs réseaux. Un **agent relais** installé sur le
site distant fait tourner, pour le compte du serveur, les sondes des
surveillances que vous lui assignez, et rapporte les résultats via la même
connexion sortante qu'il utilise déjà pour envoyer ses propres métriques.
Rien à ouvrir sur le site distant, pas de VPN, pas de deuxième instance à
surveiller.

```
 siège                                     site distant (NAT, pare-feu)
 ┌──────────────────┐   HTTPS, sortant    ┌──────────────────────────────┐
 │ serveur DumbMonit│◀────────────────────│ agent relais                 │
 │  planificateur   │  sondes à lancer ─▶ │  fait tourner SNMP/Proxmox/HTTP…│──▶ commutateur, NAS,
 │  alertes, UI     │◀── échantillons,    │  envoie aussi ses propres      │    hyperviseur…
 │                  │    verdict          │  métriques                    │
 └──────────────────┘                     └──────────────────────────────┘
```

!!! info "Canal chiffré avec certificat d'agent"
    Le relais, comme tout agent, parle au serveur en TLS 1.3 avec son propre
    certificat d'agent (TLS mutuel). Ouvrez le **port TCP 8443** (le port des
    agents) du site distant vers le serveur : le relais se connecte toujours
    vers l'extérieur, rien n'a besoin de l'atteindre. C'est le seul port
    nécessaire pour surveiller depuis un autre site. Voir
    [Sécurité du canal des agents](agent-security.md).

## 1. Publier le serveur sur HTTPS

!!! tip "Un seul port pour un site distant"
    La commande d'installation affichée pour un relais n'utilise **que le port des
    agents** : le script d'installation, le binaire de l'agent et le canal chiffré
    viennent tous de `https://<serveur>:8443` (ou `DUMBMONIT_AGENT_PUBLIC_URL`). La commande épingle la clé du
    certificat de ce port et l'empreinte SHA-256 de chaque binaire : le
    téléchargement est vérifié de bout en bout sans ouvrir le port de
    l'interface (8080) au site. Les outils de déploiement obtiennent la même
    commande par `GET /api/agent/install?agent_port_only=true`. Les paragraphes
    ci-dessous sur le port de l'interface ne concernent que les agents qui
    passent encore par l'ancien canal.

Le relais récupère ses tâches depuis le serveur : le serveur doit être
accessible depuis le site distant, typiquement
`https://monitor.example.org` derrière un reverse proxy. Deux choses
voyagent sur ce lien :

- le **jeton d'enrôlement** (`dmon_…`), dans l'en-tête `Authorization` de
  chaque requête — comme pour tout agent ;
- les **identifiants de surveillance** des surveillances que le relais sonde
  (communauté SNMP, jeton d'API Proxmox…), déchiffrés, à l'intérieur de
  chaque tâche de sonde.

Le port web n'accepte que HTTPS, donc les deux sont chiffrés en chemin. Sur
internet, placez le serveur derrière un reverse proxy avec un certificat public
(il se connecte à DumbMonit en HTTPS) ; sur un LAN ou un VPN, le certificat
autosigné convient, épinglé par l'empreinte de la commande affichée. Le jeton est le seul secret en vol jusqu'à ce que vous assigniez des
surveillances au relais. Les identifiants ne sont jamais écrits sur disque d'un
côté ou de l'autre : une tâche de sonde vit dans la mémoire du serveur
jusqu'à ce que le relais la rapporte, et l'agent ne garde rien entre deux
sondes.

Le reverse proxy doit transférer `/api/agent/relay` comme `/api/ingest`,
autoriser des corps de requête jusqu'à 16 Mo (un hyperviseur chargé rapporte
des milliers d'échantillons), et garder les connexions inactives ouvertes
pendant au moins 60 secondes : le serveur garde une requête de relais
jusqu'à 25 secondes en attendant du travail (long polling).

## 2. Créer un jeton d'enrôlement

Le plus simple : **Ajouter une surveillance → Serveur avec l'agent**, cochez
*Cet agent est un relais pour un site distant* (ou passez par l'entrée
*Relais* de la page d'ajout). Nommez le site, créez le jeton et lancez la
commande d'installation affichée sur une machine du site ; la page attend son
premier envoi. Le jeton porte lui-même le mode relais et le site : l'agent
devient relais dès son premier envoi, sans rien modifier sur la machine.

**Un agent déjà installé** devient relais depuis sa page dans DumbMonit : dans
le panneau *Relais*, activez *Relais pour un site distant*, indiquez le site,
enregistrez. L'agent bascule à son prochain envoi, sans redémarrage. On le
désactive de la même façon. Un agent trop ancien pour suivre ce réglage y est
signalé : mettez-le à jour en relançant la commande d'installation, ou utilisez
`relay: true` ci-dessous.

Les variantes ci-dessous règlent le mode relais sur la machine elle-même ; il
prime alors sur l'interrupteur de l'interface.

## 3. Démarrer le relais sur le site distant

Sous Linux, macOS ou FreeBSD, l'installateur en une ligne avec `--relay` et le
site (inutile avec un jeton de relais créé depuis la page d'ajout) :

```sh
curl -sSL -k --pinnedpubkey sha256//… https://monitor.example.org:8443/install.sh | sudo sh -s -- \
  --token=dmon_… --url=https://monitor.example.org:8443 --pinned-key=sha256//… \
  --tls-url=https://monitor.example.org:8443 --ca-pin=… --relay --site='Lyon office'
```

Copiez la vraie commande depuis DumbMonit : elle porte les épingles et les
empreintes.

Sous Windows, ajoutez `-Relay -Site 'Lyon office'` à la commande d'installation
PowerShell.

Ou n'importe quelle machine sur le site distant avec Docker, en utilisant
`docker-compose.agent.yml` du dépôt :

```sh
DUMBMONIT_AGENT_URL=https://monitor.example.org:8443 \
DUMBMONIT_AGENT_TLS_URL=https://monitor.example.org:8443 \
DUMBMONIT_AGENT_CA_PIN=… \
DUMBMONIT_AGENT_TOKEN=dmon_… \
DUMBMONIT_AGENT_HOSTNAME=relay-lyon \
DUMBMONIT_AGENT_RELAY=true \
DUMBMONIT_AGENT_SITE="Lyon office" \
docker compose -f docker-compose.agent.yml up -d
```

Un agent déjà installé devient relais avec deux clés de plus dans
`/etc/dumbmonit/agent.yaml`, puis un redémarrage de l'agent :

```yaml
relay: true
site: Lyon office
```

La machine apparaît dans DumbMonit en quelques secondes, comme une surveillance
agent ordinaire, et sa page dit *Relais · site Lyon office · relaie 0
surveillances*. Le journal de l'agent dit `relay mode enabled: waiting for
probes from the server` une fois son premier lot accepté.

!!! note "Ce que le relais doit atteindre"
    Le relais sonde les surveillances depuis son propre réseau : il a besoin du
    même accès que le serveur aurait besoin s'il était là — UDP 161 pour
    SNMP, 8006 pour Proxmox, 5001 pour DSM, et ainsi de suite. Pour les
    surveillances **ping**, le conteneur a besoin du sysctl `net.ipv4.ping_group_range`, déjà défini
    dans le fichier compose (voir [Ping ICMP sans capacité](docker.md#icmp-ping-without-a-capability)). Pour des surveillances sur un VLAN ou un réseau
    link-local, ou pour des cibles `localhost`, utilisez
    `network_mode: host`.

## 4. Assigner des surveillances au relais

Ajoutez les surveillances distantes comme d'habitude — commutateur SNMP, nœud
Proxmox, vérification HTTP — avec leur adresse **telle que vue depuis le
site distant** (`192.168.10.1`, `https://pve.lan:8006`). Dans le formulaire
de surveillance, sous *Plus d'options*, réglez **Atteint via** sur l'agent relais
plutôt que *Direct*. Les surveillances existantes peuvent être basculées de la
même façon ; repasser à *Direct* fait que le serveur les sonde à nouveau.

À partir de là :

- le serveur mettra en file une sonde pour la surveillance à chaque intervalle ;
  le relais la récupère en quelques millisecondes (il attend du travail
  sur le serveur), lance le collecteur et rapporte ;
- **Sonder maintenant** et la détection de profil sur la page de la surveillance
  passent aussi par le relais ;
- la *dernière sonde* et l'erreur de la surveillance suivent le verdict du
  relais. Quand le relais ne récupère pas une sonde à temps, la surveillance
  montre *Délai dépassé : l'agent relais … n'a pas récupéré la sonde* —
  vérifiez que l'agent tourne avec `relay: true` ;
- la page de l'agent montre *Relais pour N surveillances*, et
  `GET /api/relays` liste les relais avec leur site et leur compteur.

## 5. Installer des agents derrière le relais

Un site derrière un pare-feu a souvent des serveurs qui ne joignent pas du tout
DumbMonit : seul le relais sort. Ces serveurs peuvent quand même faire tourner
l'agent complet (système, disques, services, conteneurs, sauvegardes) : ils se
connectent à la **passerelle** du relais, qui transporte leur connexion
jusqu'au serveur.

```
 site distant (seul le relais sort)                        siège
 ┌────────────┐  TLS 1.3, mTLS   ┌──────────────┐  tunnel dans la     ┌──────────────────┐
 │ agent      │═════════════════▶│ agent relais │═ connexion mTLS  ══▶│ serveur DumbMonit│
 │ (serveur A)│  vers le port    │  passerelle  │  du relais, TCP     │  port agents 8443│
 └────────────┘  8443 du relais  └──────────────┘  8443 sortant       └──────────────────┘
        └──────────── une seule session TLS, de bout en bout, agent ⇄ serveur ─┘
```

**Dans DumbMonit**, ajoutez une surveillance, choisissez **Serveur avec
l'agent**, et sous *Comment la machine joint-elle ce serveur ?* choisissez
**Via &lt;relais&gt; (site)**. Indiquez l'adresse par laquelle le site joint
le relais (une IP ou un nom local). Les commandes affichées — Linux/macOS,
Windows, Docker — visent alors la passerelle du relais
(`--url=https://192.168.10.5:8443 --tls-url=https://192.168.10.5:8443`) et
portent toujours l'empreinte de l'autorité **du serveur**. Le flux attend
ensuite la première connexion et permet de nommer la machine, de la ranger
dans un dossier et de la placer sur un site, sans quitter la page. Depuis un
site dans *Ajouter par site*, **Serveur avec l'agent** présélectionne le
relais de ce site ; la page du relais propose **Installer un agent à travers
lui**.

Un relais ouvre sa passerelle (port 8443 par défaut) dès qu'il se connecte, et
aussitôt quand vous le choisissez pour une installation ou changez sa
passerelle : le serveur met fin à l'attente longue du relais, il n'y a pas
d'envoi à attendre. Le relais essaie ensuite lui-même sa passerelle, à
l'adresse qu'utilise le site, comme le ferait une machine du site, et en rend
compte : l'écran d'ajout affiche la commande une fois cet essai réussi, ou dit
ce qui a échoué. L'essai ne voit pas un pare-feu placé entre les machines du
site et le relais (le relais ne le traverse pas pour se joindre lui-même). Le
panneau *Relais* de sa page montre la passerelle — *À l'écoute sur le port 8443
— 3 agents connectés* — et permet d'en changer le port et l'adresse, ou de la
fermer. La commande d'installation d'un relais ouvre ce port sur le pare-feu du
relais, **au seul réseau du site** (firewalld ou ufw sous Linux, le pare-feu
Windows avec `LocalSubnet`), le dit à l'écran, et `--uninstall` (`-Uninstall`)
retire la règle ; `--no-firewall` (`-NoFirewall`) laisse le pare-feu tel quel.
Sur un relais Docker, `docker-compose.agent.yml` publie 8443
(`DUMBMONIT_AGENT_GATEWAY_PUBLISH=192.168.10.5:8443` le limite à l'adresse du
LAN). `gateway_listen: 0.0.0.0:9443` dans l'`agent.yaml` du relais
(`DUMBMONIT_AGENT_GATEWAY_LISTEN`) prime sur le réglage de l'interface. Les
relais installés avant ce fonctionnement gardent fermée une passerelle qu'ils
n'avaient jamais ouverte.

Ce qui passe par le relais :

- **Le canal chiffré de l'agent**, intact. Le relais reconnaît un ClientHello
  TLS et recopie les octets, dans les deux sens, dans un tunnel qu'il ouvre
  vers le port des agents du serveur sous *son propre* certificat client (une
  mise à niveau HTTP sur `/api/agent/relay/tunnel`). Le serveur mène alors la
  poignée de main TLS 1.3 avec l'agent du site comme s'il s'était connecté en
  direct : certificat client de l'agent, autorité épinglée, jeton et secret de
  liaison sont vérifiés par le serveur, de bout en bout. L'enrôlement suit le
  même chemin.
- **Le script d'installation et le binaire**, que le relais relit auprès du
  serveur par son propre canal et sert sur le même port (`/install.sh`,
  `/install.ps1`, `/download/<fichier>` seulement). La commande
  d'installation porte le SHA-256 de chaque binaire : le script refuse tout
  autre fichier.

Le serveur retient, pour chaque agent, le relais par lequel il se connecte. La
page de l'agent indique *Joint ce serveur via le relais …* ; la carte de
topologie dessine le relais comme son parent ; et quand le relais tombe, les
agents derrière lui sont montrés `suppressed` par le relais, pas comme autant
de pannes séparées. Un agent qui se connecte ensuite en direct (ou par un
autre relais) est mis à jour à son envoi suivant.

Le kit Active Directory offre le même choix : sous **Chemin**, choisissez le
relais, et les serveurs du domaine téléchargent l'agent depuis lui et
remontent à travers lui.

## Alerter quand le relais est en panne

Un relais qui devient silencieux emporte ses surveillances avec lui : leurs
sondes s'arrêtent, et ils deviendraient tous *inaccessibles* d'un coup. Le
relais est traité comme un **parent** de chaque surveillance qu'il sonde (en
plus du parent que vous avez pu régler) : quand le relais lui-même est
*inaccessible*, les alertes de ses surveillances sont montrées comme
`suppressed` avec `suppressed_by` = le relais, et une seule notification
sort — pour le relais. Le parent que vous avez réglé explicitement garde
son rôle et gagne quand les deux sont en panne.

L'alerte du relais le dit : sa surveillance affiche *relay unreachable: no
measurement for …* (message du serveur, en anglais), et le moniteur de salle
montre **Relais injoignable depuis …** — une ligne pour tout le site, pas une
par équipement.

## Quand le lien avec le serveur tombe

Un relais ne s'arrête pas à la première coupure du WAN : il continue de sonder
son site et ne perd rien.

1. **Il garde son planning.** Tant que le serveur répond, le relais lui
   redemande toutes les quelques minutes la liste des surveillances qu'il
   sonde, avec leur période et les identifiants nécessaires. Il la range
   chiffrée à côté de son secret de liaison (`relay-plan.enc`, AES-256-GCM,
   avec une clé aléatoire qui lui est propre dans `relay-plan.key`, lisible du
   seul compte de l'agent).
2. **Il sonde seul.** Quand le serveur ne répond plus depuis environ 35
   secondes, le relais planifie lui-même chaque surveillance, à sa période
   habituelle, avec les mêmes collecteurs et le même plafond de concurrence.
3. **Il garde les résultats sur disque.** Chaque résultat part dans un tampon
   disque (`spool/relay/`), écrit au fil de l'eau, avec sa somme de contrôle,
   forcé sur le disque : une coupure de courant sur le site ne perd rien de ce
   qui a déjà été sondé. Le tampon partage le `buffer_max_mb` de l'agent
   (256 Mo par défaut, compressé ; trois quarts pour les résultats du relais,
   un quart pour les mesures de la machine elle-même). Plein, il sacrifie les
   résultats les plus anciens ; `agent_relay_dropped_results` les compte, et le
   serveur journalise la perte au retour du relais.
4. **Il rattrape.** Dès que le serveur répond, il reprend la planification, et
   le relais lui renvoie ce qu'il a gardé, du plus ancien au plus récent, par
   lots de 200 résultats au plus séparés d'une pause. Chaque résultat porte ses
   horodatages d'origine : les graphiques du site se comblent d'eux-mêmes.
   Chaque résultat a un numéro de séquence ; un lot envoyé deux fois (accusé de
   réception perdu avec le lien) est reconnu et n'est pas réécrit. Un résultat
   pour un équipement qui ne passe plus par ce relais est ignoré.

Pendant la coupure, le site apparaît comme une seule alerte, celle du relais
(voir plus haut), et les équipements derrière lui comme `suppressed`. Leur état
courant n'est pas réécrit par le passé : dès que le serveur les sonde de
nouveau, leur statut est celui du présent, et leur historique est complet.

Un résultat que le relais a obtenu lien établi mais n'a pas pu rendre (le lien
est tombé entre-temps) est gardé de la même façon plutôt que perdu. Un
équipement qui n'a pas répondu pendant la coupure laisse un trou, exactement
comme si le serveur l'avait sondé.

**Ce que protège le chiffrement.** Le planning gardé contient les identifiants
des équipements du site. Le chiffrer protège contre la fuite du fichier seul :
copie du répertoire de données sans le fichier de clé, sauvegarde partielle,
disque récupéré d'un ancien relais. Il ne protège pas contre qui peut lire tout
le répertoire de l'agent avec ses droits ou ceux d'un administrateur : il y
trouverait la clé, mais aussi le certificat et le secret de liaison de l'agent,
qui lui permettent de toute façon de demander ce même planning au serveur.
Contre cela, ce sont les permissions de la machine qui protègent, pas la
cryptographie. Couper le mode relais efface le planning, sa clé et les
résultats en attente ; un relais révoqué (jeton ou certificat refusé) efface
aussitôt son planning. `buffer_max_mb: 0` désactive tout cela : le relais ne
sonde plus que quand le serveur le lui demande.

## Sonder depuis le serveur et plusieurs relais

Un moniteur n'a pas à choisir entre le serveur et un relais : il peut être sondé depuis les deux, ou depuis plusieurs relais, avec un quorum et un tour de confirmation avant toute alerte. Voir [Sonder depuis plusieurs emplacements](../devices/services.fr.md#sonder-depuis-plusieurs-emplacements).

## Dépannage

| Ce que vous voyez | Cause et correction |
|---|---|
| La machine relais n'apparaît jamais | La commande n'a pas atteint le serveur. Depuis le site distant, ouvrez l'adresse du serveur dans un navigateur et vérifiez le TCP 8443. Voir [Ports réseau](ports.md). |
| La page d'ajout affiche un message rouge sur le canal | Le port 8443 n'est pas publié ou est bloqué. Publiez-le (`"8443:8443"` sous `ports:`) et ouvrez-le dans le pare-feu. |
| La page de l'agent n'indique pas *Relais* | L'interrupteur relais est éteint. Ouvrez la surveillance, utilisez la **roue crantée**, activez *Relais pour un site distant*, indiquez le site, enregistrez. C'est pris en compte au prochain envoi. |
| Une surveillance affiche *Timed out: relay agent … did not pick up the probe* (message du serveur, en anglais) | Le relais est arrêté ou pas en mode relais. Vérifiez le service sur la machine relais et que `relay: true` est défini. |
| Une surveillance affiche un message rouge sous son adresse | Le relais ne joint pas l'équipement. L'adresse doit être celle **vue depuis le site distant**, et le relais a besoin des mêmes accès que le serveur. |
| Les agents installés derrière le relais n'apparaissent pas | Lancez la commande copiée depuis la page *Installer un agent derrière ce relais*, pas la commande directe. Le relais ouvre sa passerelle en quelques secondes ; l'écran d'ajout dit s'il a pu la joindre à l'adresse du site. |

## Limites

- Un relais par surveillance. Une surveillance sondée par le relais n'est pas aussi
  sondé par le serveur.
- Les surveillances de type agent ne sont jamais sondées par un relais (elles
  envoient leurs propres métriques) ; un agent peut en revanche se *connecter*
  par la passerelle d'un relais (voir plus haut). Un relais se connecte
  toujours en direct : pas de relais derrière un relais.
- La passerelle d'un relais accepte 64 connexions à la fois et 4 Mio/s par
  tunnel et par sens par défaut (`gateway_max_connections`,
  `gateway_rate_kib`) ; le serveur accepte 256 tunnels par relais.
- Le relais fait tourner 8 sondes à la fois au plus ; une sonde est bornée
  par le délai de sonde du serveur (`DUMBMONIT_PROBE_TIMEOUT`), plafonné à
  120 s sur l'agent.
- Un agent relais a besoin des collecteurs compilés : les agents plus
  anciens que cette fonctionnalité ignorent `relay: true`, et le serveur
  rapporte leurs sondes comme en délai dépassé. Réinstallez avec le binaire
  ou l'image actuels.
- Pendant une coupure, le relais ne sonde que ce que son dernier planning
  listait : une surveillance ajoutée entre-temps attend le serveur. Les sondes
  multi-emplacements et les sauvegardes de configuration restent au serveur.
