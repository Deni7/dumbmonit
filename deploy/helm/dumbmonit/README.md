# DumbMonit Helm chart

Installs DumbMonit (one container: server, bundled VictoriaMetrics, SQLite) on Kubernetes.

```bash
kubectl create namespace dumbmonit
kubectl -n dumbmonit create secret generic dumbmonit-secret \
  --from-literal=DUMBMONIT_SECRET="$(openssl rand -hex 24)"
helm install dumbmonit oci://ghcr.io/selfhostr/charts/dumbmonit \
  --namespace dumbmonit \
  --set secret.existingSecret=dumbmonit-secret \
  --set agentPublicUrl=https://agents.example.com:8443
```

## Things to know

- **One replica only.** SQLite has a single writer; the chart pins `replicas: 1`. Resilience comes from the PVC, not from scaling.
- **Two ports, two Services.** The web UI (8080, HTTPS) sits behind a ClusterIP Service and an optional Ingress.
  The agent channel (8443, TLS 1.3 with client certificates) has its own Service (`LoadBalancer` by default, or `NodePort`).
  Never put it behind an Ingress or proxy that terminates TLS: the mTLS handshake must reach DumbMonit.
- The UI itself serves HTTPS. Probes use HTTPS on `/api/health`; an Ingress must talk HTTPS to the backend (ingress-nginx annotation provided).
- Runs as uid 65532, read-only root filesystem (`/tmp` is an emptyDir), all capabilities dropped. `/data` is the PVC.
- `DUMBMONIT_AGENT_PUBLIC_URL` (`agentPublicUrl`) is read by recent DumbMonit versions only; older images ignore it.
- Back up the data **and** the instance secret.

## Main values

| Key | Default | Meaning |
|---|---|---|
| `image.tag` | appVersion | Image tag |
| `secret.existingSecret` / `secret.value` | empty | `DUMBMONIT_SECRET`; empty generates `/data/secret.key` |
| `publicUrl`, `agentPublicUrl`, `tlsNames`, `trustedProxies` | empty | Matching `DUMBMONIT_*` variables |
| `tls.existingSecret` | empty | `kubernetes.io/tls` Secret for the web port |
| `victoriametrics.externalUrl` | empty | `DUMBMONIT_VM_URL`; disables the bundled VictoriaMetrics |
| `persistence.size`, `.storageClassName`, `.existingClaim` | 10Gi | Volume for `/data` |
| `service.type` | ClusterIP | Web UI Service |
| `agentService.enabled`, `.type`, `.nodePort` | true, LoadBalancer | Agent Service |
| `ingress.enabled`, `.hosts`, `.tls` | false | Optional UI Ingress |
| `networkPolicy.enabled`, `.uiFrom`, `.agentFrom` | false | Ingress rules per port |
| `podDisruptionBudget.enabled` | false | PDB (`maxUnavailable: 1`) |
| `pingSysctl` | true | Allows ICMP ping monitors without privileges |
| `extraEnv` | `[]` | Any other `DUMBMONIT_*` variable |

All values: [`values.yaml`](values.yaml), validated by `values.schema.json`.
Full guide: <https://dumbmonit.selfhostr.io/docs/install/kubernetes/>.
