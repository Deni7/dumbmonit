# Kubernetes

DumbMonit runs on Kubernetes as **one pod**: the server, its bundled
VictoriaMetrics and a SQLite database in `/data`. A Helm chart and plain
manifests are provided.

!!! warning "One replica, always"
    SQLite has a single writer. Run exactly one replica (the chart pins it).
    Resilience comes from the persistent volume and a fast restart, not from
    scaling out.

## Install with Helm

Create the instance secret first (16 characters minimum). It encrypts monitor
credentials and tokens; keep it with your backups.

```bash
kubectl create namespace dumbmonit
kubectl -n dumbmonit create secret generic dumbmonit-secret \
  --from-literal=DUMBMONIT_SECRET="$(openssl rand -hex 24)"

helm install dumbmonit oci://ghcr.io/selfhostr/charts/dumbmonit \
  --namespace dumbmonit \
  --set secret.existingSecret=dumbmonit-secret \
  --set agentPublicUrl=https://agents.example.com:8443
```

Then read the one-time code for the first admin account:

```bash
kubectl -n dumbmonit logs statefulset/dumbmonit | grep -i code
kubectl -n dumbmonit port-forward svc/dumbmonit 8080:8080   # https://localhost:8080
```

Without `secret.existingSecret`, DumbMonit generates `/data/secret.key` on the
volume: back it up with the data.

## Two ports, two Services

| Port | Service | Exposure |
|------|---------|----------|
| 8080 (HTTPS) | `dumbmonit` (ClusterIP) | Web interface. Optional Ingress. |
| 8443 (TLS 1.3 + client certificates) | `dumbmonit-agents` (LoadBalancer by default, or NodePort) | Agents on other networks. |

The agent channel is authenticated by mutual TLS **end to end**. Publish it
with a `LoadBalancer` or `NodePort` Service (set `agentService.type`), never
through an Ingress or a proxy that terminates TLS. Set `agentPublicUrl`
(`DUMBMONIT_AGENT_PUBLIC_URL`) to the address agents will use; recent
DumbMonit versions read it for install commands and enrolment, older images
ignore it.

DumbMonit serves HTTPS on 8080 too, so an Ingress must reach the backend over
HTTPS. The chart sets the ingress-nginx annotation
`nginx.ingress.kubernetes.io/backend-protocol: "HTTPS"`; adapt it to your
controller. Add the Ingress host to `publicUrl`/`tlsNames` and your controller
to `trustedProxies` (see [HTTPS and encryption](https.md)).

```yaml
# values.yaml
publicUrl: https://monit.example.com
trustedProxies: 10.42.0.0/16        # your ingress controller pods
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

## Storage, VictoriaMetrics, security

- `persistence.size` (10Gi by default) and `persistence.storageClassName` set
  the volume holding `/data` (database, secret, bundled VictoriaMetrics).
- To use your own VictoriaMetrics, set `victoriametrics.externalUrl`
  (`DUMBMONIT_VM_URL`); the bundled one is then not started.
- The pod runs as uid 65532, with a read-only root filesystem (`/tmp` is an
  emptyDir), no capabilities and the `RuntimeDefault` seccomp profile.
  `pingSysctl` lets ping monitors work without privileges.
- The liveness probe calls `/api/health` over HTTPS (it always answers `200`). For a
  readiness probe of your own, use `/api/ready`: `200` when the database and
  VictoriaMetrics answer, `503` otherwise.
- Optional: `networkPolicy.enabled`, `podDisruptionBudget.enabled`.
- Any other `DUMBMONIT_*` variable goes in `extraEnv`
  (see [Docker](docker.md) for the list).

## Plain manifests

Without Helm, [`deploy/kubernetes/`](https://github.com/selfhostr/dumbmonit/tree/main/deploy/kubernetes)
holds the same objects: `dumbmonit.yaml` (ServiceAccount, Services,
StatefulSet) and `optional.yaml` (Ingress, PodDisruptionBudget,
NetworkPolicy). Create the `dumbmonit-secret` Secret as above, edit the hosts,
then `kubectl apply -f`.

## Update

```bash
helm upgrade dumbmonit oci://ghcr.io/selfhostr/charts/dumbmonit \
  --namespace dumbmonit --reuse-values --version <chart version>
```

The chart version follows DumbMonit's release tag; the image tag defaults to
it. Back up before updating (see [Backup and restore](backup.md)).
