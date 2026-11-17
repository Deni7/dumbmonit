#!/usr/bin/env bash
# Crée la clé Ed25519 qui signe les binaires de l'agent (voir
# docs/development/agent-signing.md). Pour les mainteneurs, une fois, sur une
# machine de confiance — jamais en CI.
#
#   scripts/agent-signing-key.sh [DOSSIER]   (défaut : ~/.dumbmonit-agent-signing,
#                                            hors du dépôt pour ne jamais la committer)
#
# Ce que le script fait :
#   - crée DOSSIER en 0700, la clé privée agent-signing.pem en 0600 ;
#   - écrit la moitié publique dans agent-signing.pub.pem ;
#   - vérifie la paire par une signature d'essai ;
#   - affiche les deux commandes qui restent à lancer, à la main.
#
#   - ajoute la clé publique à crates/agent/signing/release-keys.pem (liste de
#     clés acceptées : les précédentes y restent, c'est la rotation), sauf si
#     elle y est déjà. Il reste à committer ce fichier : release.yml refuse de
#     signer avec une clé qui n'y figure pas.
#
# Ce qu'il ne fait PAS : committer, toucher aux secrets GitHub, ni envoyer la
# clé où que ce soit. Il refuse d'écraser une clé existante.
#
# Prérequis : OpenSSL 1.1.1 ou plus récent (Ed25519, `pkeyutl -rawin`).
set -euo pipefail

DIR=${1:-$HOME/.dumbmonit-agent-signing}
KEY="$DIR/agent-signing.pem"
PUB="$DIR/agent-signing.pub.pem"

command -v openssl >/dev/null 2>&1 || { echo "openssl is required" >&2; exit 1; }
if [ -e "$KEY" ]; then
    echo "Refusing to overwrite $KEY: move it away first." >&2
    exit 1
fi

umask 077
mkdir -p "$DIR"
chmod 700 "$DIR"

openssl genpkey -algorithm ed25519 -out "$KEY"
chmod 600 "$KEY"
openssl pkey -in "$KEY" -pubout -out "$PUB"

# Signature d'essai, exactement comme release.yml la produit et comme l'agent
# la vérifie (signature détachée de 64 octets bruts).
probe=$(mktemp)
trap 'rm -f "$probe" "$probe.sig"' EXIT
head -c 4096 /dev/urandom > "$probe"
openssl pkeyutl -sign -rawin -inkey "$KEY" -in "$probe" -out "$probe.sig"
[ "$(wc -c < "$probe.sig" | tr -d ' ')" = 64 ] || { echo "unexpected signature size" >&2; exit 1; }
openssl pkeyutl -verify -rawin -pubin -inkey "$PUB" -in "$probe" -sigfile "$probe.sig" >/dev/null

# Liste des clés acceptées par l'agent, dans le dépôt qui contient ce script.
KEYS="$(cd "$(dirname "$0")/.." && pwd)/crates/agent/signing/release-keys.pem"
if [ -f "$KEYS" ]; then
    if grep -qxF "$(sed -n 2p "$PUB")" "$KEYS"; then
        added="already listed in $KEYS"
    else
        {
            printf '\n# Key added on %s (rotation: keep the previous keys until agents have updated).\n' "$(date +%F)"
            cat "$PUB"
        } >> "$KEYS"
        added="appended to $KEYS (commit it)"
    fi
else
    added="release-keys.pem not found: append the block below to crates/agent/signing/release-keys.pem"
fi

cat <<FIN
Ed25519 agent signing key created:
  private key: $KEY  (keep it offline; never commit it)
  public key:  $PUB

Public key: $added

$(cat "$PUB")

Next:
  1. Commit crates/agent/signing/release-keys.pem and push it to dev, then
     release: agents trust the new key from that version on.
  2. Store the private key as the repository secret used by release.yml:

  gh secret set AGENT_SIGNING_KEY -R selfhostr/dumbmonit < "$KEY"

Keep an offline copy of $KEY, then delete it from this machine.
FIN
