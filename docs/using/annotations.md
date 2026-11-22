# Change markers

A change marker says "something changed here": a deployment, an update, a
maintenance, a configuration change. DumbMonit draws it as a thin vertical line
on the charts (or as a shaded band when it lasted a while), so when a curve
breaks you can see what happened just before.

Markers are drawn on the charts of a [monitor page](devices.md) and in the
[metrics explorer](dashboards.md). Hover, focus or tap a marker for its title,
time, details and link. A list above the charts gives the same
information as text, so it works with a keyboard or a screen reader. The switch
**On charts** hides the markers; the choice is remembered in your browser.

## Add a marker by hand

On a monitor page, **Add a marker**:

| Field | What it does |
| --- | --- |
| **Title** | What happened, such as *Deploy v2.4.1*. |
| **When** | Now by default. |
| **Until** | Optional. Gives the marker an end, for a change that lasted a while: a 10-minute deployment is drawn as a band. |
| **Details**, **Link** | Optional. The link is a pipeline run, a ticket or a changelog (`http://` or `https://`). |
| **Applies to** | *This monitor only*, or *All monitors*. |

Administrators and operators can add and delete markers; everyone else can read
them. Accounts restricted by [teams](teams.md) see the markers that apply to the
whole instance and those that touch a monitor they can see, and can only add
markers to monitors their teams let them handle (never to the whole instance).

## Where a marker applies

A marker has a **scope**:

| Scope | `scope` in the API | Shown on |
| --- | --- | --- |
| Everything | `{"kind": "global"}` (the default) | Every monitor's charts, and the explorer. |
| Some monitors | `{"kind": "monitors", "targets": [4, 7]}` | Those monitors. |
| A folder | `{"kind": "folder", "folder": "Production"}` | The monitors of that folder and its subfolders. |
| Tags | `{"kind": "tags", "tags": ["env=prod"]}` | Monitors carrying one of these tags, as `key` or `key=value`. |

In the explorer, every marker you may see is drawn, whatever its scope.

## Markers and alerts

When an alert fires within 30 minutes after a marker that applies to the same
monitor (counted from the end of the marker when it is a window), the alert's
detail says **After change: *title***. It is a lead, not a verdict: the marker
and the alert are only close in time.

## Post markers from a pipeline

`POST /api/annotations` accepts an [API token](assistant.md#1-create-a-token) with
the **write** scope, which is what a CI job needs. Create a token named after the
pipeline, keep it in the CI's secret store, and call the API when a deployment
starts or ends.

```bash
curl -sk -X POST https://dumbmonit.example:8080/api/annotations \
  -H "Authorization: Bearer $DUMBMONIT_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
        "title": "Deploy v2.4.1",
        "text": "Rolled out by the pipeline.",
        "link": "https://ci.example/runs/42",
        "tags": ["deploy"],
        "scope": { "kind": "folder", "folder": "Production" },
        "source": "ci"
      }'
```

Only `title` is required. `at` defaults to now; send it, in RFC 3339
(`2026-11-22T10:00:00Z`), to record something after the fact, and add `until` for
a window. The answer is the marker, with its `id`.

### GitHub Actions

```yaml
jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      # ... your deployment steps ...
      - name: Mark the deployment in DumbMonit
        if: success()
        env:
          DUMBMONIT_URL: ${{ vars.DUMBMONIT_URL }}
          DUMBMONIT_TOKEN: ${{ secrets.DUMBMONIT_TOKEN }}
        run: |
          curl -fsS -X POST "$DUMBMONIT_URL/api/annotations" \
            -H "Authorization: Bearer $DUMBMONIT_TOKEN" \
            -H "Content-Type: application/json" \
            -d "$(jq -n \
                  --arg title "Deploy ${GITHUB_REF_NAME}" \
                  --arg link "${GITHUB_SERVER_URL}/${GITHUB_REPOSITORY}/actions/runs/${GITHUB_RUN_ID}" \
                  '{title: $title, link: $link, tags: ["deploy"], source: "ci"}')"
```

### Read, edit, delete

```bash
# Markers touching a window, for one monitor (id 12)
curl -s -H "Authorization: Bearer $DUMBMONIT_TOKEN" \
  "https://dumbmonit.example:8080/api/annotations?target=12&from=2026-11-22T00:00:00Z&to=2026-11-23T00:00:00Z"

# Change a marker: only the fields you send change; "until": null removes the window
curl -s -X PATCH -H "Authorization: Bearer $DUMBMONIT_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"title": "Deploy v2.4.2"}' https://dumbmonit.example:8080/api/annotations/42

curl -s -X DELETE -H "Authorization: Bearer $DUMBMONIT_TOKEN" \
  https://dumbmonit.example:8080/api/annotations/42
```

A `read` token can list markers but not add them. `GET` returns the 500 most
recent markers of the window at most.

## Retention and backup

Markers older than a year are purged. They are part of the
[backup](../install/backup.md): a restore brings them back, their monitors
found again by type and address; a marker whose monitors are not in the backup is
skipped and reported.
