# Première alerte en cinq minutes

De `docker compose up -d` à un message sur votre téléphone, sans lire le reste de la
documentation. Chaque étape dit ce que vous devez voir.

| Étape | Ce que vous faites | Durée habituelle |
|---|---|---|
| 1. Démarrer | `docker compose up -d` (premier téléchargement de l'image) | 30 à 60 s |
| 2. Code de configuration | `docker compose logs dumbmonit` | 15 s |
| 3. Compte | Ouvrir la page, accepter le certificat une fois, créer le compte administrateur | 1 min |
| 4. Quoi surveiller | Saisir une adresse, appuyer sur Entrée | 15 s |
| 5. Où vont les alertes | Un clic sur ntfy (ou Discord, Telegram, e-mail, webhook), puis **Ajouter un canal** | 30 s |
| 6. Preuve | **Envoyer une alerte de test** : le message arrive | 15 s |

Environ trois minutes et demie, surtout pour le téléchargement de l'image et le compte.

## 1. Démarrer le conteneur

```bash
mkdir dumbmonit && cd dumbmonit
curl -fsSLO https://raw.githubusercontent.com/selfhostr/dumbmonit/main/docker-compose.yml
docker compose up -d
docker compose logs dumbmonit   # le code de configuration à usage unique, par exemple K7XQ4-M9PRT
```

## 2. Créer le compte

Ouvrez `https://<votre-hôte>:8080`. Le certificat est auto-signé au premier démarrage :
acceptez l'avertissement une fois ([le remplacer plus tard](https.md)). Saisissez le
code de configuration, un nom d'utilisateur et un mot de passe d'au moins 12
caractères, puis **Créer le compte et continuer**.

## 3. Votre première alerte

L'assistant de configuration s'ouvre sur **Votre première alerte**, trois maillons d'une
même chaîne. Le point de chaque maillon passe au vert, avec le mot *fait*, une fois
qu'il est prouvé.

1. **Que faut-il surveiller ?** Saisissez une adresse et cliquez sur **Surveiller** :
    - `https://nas.lan` ou n'importe quelle URL : une vérification de site (code HTTP,
      certificat) ;
    - `192.168.1.10` ou `routeur.lan` : un ping ;
    - `nas.lan:22` : un port TCP.

    Le moniteur est sondé aussitôt, pas à la minute suivante : en quelques secondes
    vous lisez *nas.lan répond.* (ou pourquoi il ne répond pas). Le reste (SNMP,
    Proxmox, l'agent, un NAS) est sous **Un autre type**, dans un nouvel onglet.
2. **Où l'alerte doit-elle vous joindre ?** Un clic sur un type de canal. Le
   formulaire ne demande que ce dont ce type a besoin, et le nom reprend celui du type.
    - **ntfy** est le plus rapide jusqu'au téléphone : un sujet privé est déjà rempli.
      Ajoutez le canal, installez l'application ntfy sur le téléphone et abonnez-vous
      au sujet affiché, ou ouvrez le lien en dessous.
    - Discord demande une URL de webhook, Telegram un jeton de bot et un identifiant
      de discussion, l'e-mail un serveur SMTP, le webhook n'importe quelle URL.
      **Autres canaux** les liste tous.
3. **Le prouver.** **Envoyer une alerte de test** envoie un message vers ce canal par
   le même notificateur qu'une vraie alerte. Vous devez lire *Test envoyé à ntfy* et
   voir le message sur le téléphone.

C'est tout le chemin. **Continuer** passe au nom et à la langue de l'instance, à
d'autres moniteurs et à l'agent ; **Passer la configuration** quitte l'assistant à tout
moment.

## Quand arrive une vraie alerte ? {#quand-arrive-une-vraie-alerte}

Les règles livrées sont actives dès le départ et préviennent tous les canaux actifs.
Avec l'intervalle de vérification par défaut de 60 s :

| Le moniteur | Règle | De la panne au message |
|---|---|---|
| Un site, un ping ou un port TCP ne répond plus | *Service en panne* : deux minutes de vérifications en échec (au moins trois d'affilée) | 2,5 à 4 min |
| Tout autre moniteur cesse de remonter | *Surveillance inaccessible* : aucune mesure depuis trois minutes, confirmé au cycle suivant | 3 à 4,5 min |

Une vérification manquée isolée ne déclenche jamais d'alerte. Les alertes qui sonnent
ensemble partent en un seul message (fenêtre de regroupement de 30 s, *Réglages →
Notifications*). Chaque délai se règle : l'intervalle sur le moniteur, le délai de
chaque règle dans *Alertes → Règles*.

## Le mesurer sur votre instance

*Réglages → Système → À propos → Premier lancement* indique quand cette instance a
franchi chaque étape : démarrée sans compte, administrateur créé, premier moniteur,
premier canal, premier message remis (un test compte), première vraie alerte remise,
chacune avec son délai depuis le départ. Ces heures restent sur l'instance ; rien n'est
envoyé ailleurs.

## Ensuite

- [Ajouter d'autres moniteurs](first-device.md) : SNMP, un scan réseau, Proxmox, l'agent.
- [Canaux de notification](../notifications.md) et [règles d'alerte](../alerting/rules.md).
- Sauvegardez *Réglages → Sauvegarde* **et** `/data/secret.key` : voir
  [Sauvegarde](backup.md).
