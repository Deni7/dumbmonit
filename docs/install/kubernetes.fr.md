# Kubernetes

DumbMonit tourne sur Kubernetes dans **un seul pod** : le serveur, son
VictoriaMetrics embarqué et une base SQLite dans `/data`. Un chart Helm et des
manifests bruts sont fournis.

!!! warning "Un seul réplica, toujours"
    SQLite n'a qu'un seul écrivain. Lancez exactement un réplica (le chart le
    fixe). La résilience vient du volume persistant et d'un redémarrage
    rapide, pas de la montée en charge horizontale.

## Installer avec Helm

Créez d'abord le secret de l'instance (16 caractères minimum). Il chiffre les
identifiants et jetons des moniteurs ; conservez-le avec vos sauvegardes.

```bash
kubectl create namespace dumbmonit
kubectl -n dumbmonit create secret generic dumbmonit-secret \
  --from-literal=DUMBMONIT_SECRET="$(openssl rand -hex 24)"

helm install dumbmonit oci://ghcr.io/selfhostr/charts/dumbmonit \
  --namespace dumbmonit \
  --set secret.existingSecret=dumbmonit-secret \
  --set agentPublicUrl=https://agents.example.com:8443
```

Lisez ensuite le code à usage unique du premier compte administrateur :

```bash
kubectl -n dumbmonit logs statefulset/dumbmonit | grep -i code
kubectl -n dumbmonit port-forward svc/dumbmonit 8080:8080   # https://localhost:8080
```

Sans `secret.existingSecret`, DumbMonit génère `/data/secret.key` sur le
volume : sauvegardez-le avec les données.

## Deux ports, deux Services

| Port | Service | Exposition |
|------|---------|------------|
| 8080 (HTTPS) | `dumbmonit` (ClusterIP) | Interface web. Ingress optionnel. |
| 8443 (TLS 1.3 + certificats clients) | `dumbmonit-agents` (LoadBalancer par défaut, ou NodePort) | Agents d'autres réseaux. |

Le canal des agents est authentifié par TLS mutuel **de bout en bout**.
Publiez-le avec un Service `LoadBalancer` ou `NodePort` (`agentService.type`),
jamais à travers un Ingress ou un proxy qui termine le TLS. Renseignez
`agentPublicUrl` (`DUMBMONIT_AGENT_PUBLIC_URL`) avec l'adresse que les agents
utiliseront ; les versions récentes de DumbMonit la lisent pour les commandes
d'installation et l'enrôlement, les images plus anciennes l'ignorent.

DumbMonit sert aussi du HTTPS sur le 8080 : un Ingress doit joindre le backend
en HTTPS. Le chart pose l'annotation ingress-nginx
`nginx.ingress.kubernetes.io/backend-protocol: "HTTPS"` ; adaptez-la à votre
contrôleur. Ajoutez l'hôte de l'Ingress à `publicUrl`/`tlsNames` et votre
contrôleur à `trustedProxies` (voir [HTTPS et chiffrement](https.md)).

```yaml
# values.yaml
publicUrl: https://monit.example.com
trustedProxies: 10.42.0.0/16        # les pods de votre contrôleur d'ingress
ingress:
  enabled: true
  className: nginx
  hosts:
    - host: monit.example.com
      paths: [{path: /, pathType: Prefix}]
  tls:
    - secretName: monit-tls
      hosts: [monit.example.com]
```

## Stockage, VictoriaMetrics, sécurité

- `persistence.size` (10Gi par défaut) et `persistence.storageClassName`
  règlent le volume de `/data` (base, secret, VictoriaMetrics embarqué).
- Pour utiliser votre propre VictoriaMetrics, renseignez
  `victoriametrics.externalUrl` (`DUMBMONIT_VM_URL`) ; l'embarqué n'est alors
  pas démarré.
- Le pod tourne en uid 65532, système de fichiers racine en lecture seule
  (`/tmp` est un emptyDir), sans capacité et avec le profil seccomp
  `RuntimeDefault`. `pingSysctl` permet aux moniteurs ping de fonctionner sans
  privilèges.
- Les sondes appellent `/api/health` en HTTPS.
- Optionnel : `networkPolicy.enabled`, `podDisruptionBudget.enabled`.
- Toute autre variable `DUMBMONIT_*` va dans `extraEnv`
  (liste dans [Docker](docker.md)).

## Manifests bruts

Sans Helm, [`deploy/kubernetes/`](https://github.com/selfhostr/dumbmonit/tree/main/deploy/kubernetes)
contient les mêmes objets : `dumbmonit.yaml` (ServiceAccount, Services,
StatefulSet) et `optional.yaml` (Ingress, PodDisruptionBudget,
NetworkPolicy). Créez le Secret `dumbmonit-secret` comme ci-dessus, adaptez les
hôtes, puis `kubectl apply -f`.

## Mise à jour

```bash
helm upgrade dumbmonit oci://ghcr.io/selfhostr/charts/dumbmonit \
  --namespace dumbmonit --reuse-values --version <version du chart>
```

La version du chart suit le tag de publication de DumbMonit ; le tag de
l'image en découle par défaut. Sauvegardez avant de mettre à jour (voir
[Sauvegarde et restauration](backup.md)).
