"""Documentation build settings (English only).

Two places publish the same build:
- Read the Docs (the mirror): nothing to set, the canonical URL comes from READTHEDOCS_CANONICAL_URL.
- The product website, under /docs/ (built outside this repository): DUMBMONIT_DOCS_BASE is the
  docs root of the website (https://dumbmonit.selfhostr.io/docs/).
Either way the header carries the navigation of the website (overrides/partials/header.html),
with its labels taken from hooks/site_labels.json (a copy of the few labels of the website), so
the docs read as part of the site.
"""

import json
import os
from pathlib import Path

SITE = "https://dumbmonit.selfhostr.io/"
REPO = "https://github.com/selfhostr/dumbmonit"
LABELS = Path(__file__).resolve().parent / "site_labels.json"


def _site_nav(docs_home: str) -> dict:
    """The website's top navigation (labels from the site)."""
    t = json.loads(LABELS.read_text()) if LABELS.exists() else {}
    links = [
        (SITE, "nav_product", ""),
        (docs_home, "nav_docs", "current"),
        (REPO, "k010", "github"),
    ]
    return {
        "home": SITE,
        "home_label": t.get("k172", "DumbMonit"),
        "label": t.get("k173", "Sections"),
        "menu": t.get("nav_menu", "Menu"),
        "links": [{"url": u, "label": t.get(k, k), "kind": kind} for u, k, kind in links],
    }


def on_config(config):
    base = (os.environ.get("DUMBMONIT_DOCS_BASE") or "").strip()
    if base and not base.endswith("/"):
        base += "/"
    canonical = os.environ.get("READTHEDOCS_CANONICAL_URL")
    if base:
        config.site_url = base
    elif canonical:
        config.site_url = canonical if canonical.endswith("/") else canonical + "/"

    config.extra["site"] = _site_nav(config.site_url)
    # The logo goes back to the website, like the site's own header.
    config.extra["homepage"] = config.extra["site"]["home"]
    return config
