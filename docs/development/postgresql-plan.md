# PostgreSQL as an optional database: plan

Status: **proposal, not started** (inventory of November 2026, migrations up
to `0290`). SQLite stays the default and keeps every feature it has today.
PostgreSQL would be an option chosen with one setting, for teams that already
run and back up a PostgreSQL, or that want a managed one. This page lists what
the code relies on today and proposes phases the owner can approve one by one.

The [active/standby mode](../install/high-availability.md) already covers
server failover without PostgreSQL. Nothing here changes that mode for SQLite.

## 1. Inventory

All counts are from `crates/server/src` (`*.rs`) unless noted, made with
`grep`, so they are close but not exact.

### Driver, pool, transactions, migrations

- **Driver.** `sqlx` 0.9 with the `sqlite` feature only
  (`crates/server/Cargo.toml:43`). SQLite itself is compiled into the binary
  by `libsqlite3-sys` (`Dockerfile:34`). The `postgres` driver of the same
  `sqlx` is **already compiled** into the workspace for the database monitors
  (`crates/collectors/Cargo.toml:119`, pure Rust with rustls), so PostgreSQL
  support adds no new crate and no licence question for `cargo deny`.
- **Pool.** `db::open_with` (`db/mod.rs:87`), called once from
  `main.rs:118`, builds a `SqlitePool`: WAL, `synchronous = NORMAL`, foreign
  keys on, `busy_timeout` 5 s, `cache_size` 2 MB, `temp_store = MEMORY`
  (`db/mod.rs:103-145`), 4 connections by default (`DEFAULT_POOL_SIZE`,
  `DUMBMONIT_DB_POOL`, `config.rs:451`). The file path is fixed:
  `/data/dumbmonit.db` (`config.rs:535`, `db/mod.rs:50`).
- **The pool type is concrete everywhere.** `SqlitePool` appears 1 066 times
  in 124 files (136 more in 64 test files); `sqlx::sqlite::SqliteRow` in 31
  files (row mappers such as `db/alerts.rs:109`); `Transaction<'_, Sqlite>` 82
  times (`db/target_channels.rs:52`); two generic `Executor<Database =
  Sqlite>` helpers (`db/status_lang.rs:71`).
- **Queries.** 1 248 `sqlx::query*` calls in 124 files, plus 224 in 59 test
  files. All are runtime strings: no `query!` compile-time macros, one
  `QueryBuilder` (`auth/audit.rs:154`). Placeholders are `?`, and `?N` in 7
  files (59 uses, e.g. `backup/import/netconfig.rs:92`). Biggest users:
  `backup/` 212, `db/` 204, `auth/` 149, `collectors/` 147, `api/` 103.
- **Transactions.** 84 `begin()` in 33 files, all deferred (no `BEGIN
  IMMEDIATE`), no `SELECT … FOR UPDATE` anywhere.
- **Migrations.** 125 files, `0001`-`0290` with gaps (ranges are handed out
  per task), 3 270 lines, embedded by `sqlx::migrate!("src/db/migrations")`
  (`db/mod.rs:28`) and applied on every start. `db::open_up_to`
  (`db/mod.rs:97`) replays history up to a version for upgrade tests
  (`notify/webpush/mod.rs:605`, `tests/agent_token_names_migration.rs`,
  `tests/oncall_toggle_migration.rs`).

### SQLite-specific SQL

| Category | Count | Examples | PostgreSQL |
|---|---|---|---|
| `strftime(…)` in queries | 103 in 32 files | `db/targets.rs:171` | compute in Rust, bind |
| `datetime('now')`, `datetime('now', ?)` | 65 in 26 files | `db/relay_backlog.rs:41`, `webchange/store.rs:99` | compute in Rust, bind |
| `strftime('%s', …)` (epoch arithmetic) | 9 | `website/store.rs:50` | `EXTRACT(EPOCH …)` helper |
| `INSERT OR IGNORE` | 24 in 16 files | `first_run.rs:116`, `db/target_channels.rs:62` | `ON CONFLICT DO NOTHING` (also valid SQLite) |
| `INSERT OR REPLACE` | 5 in 2 files | `db/status_pages.rs:553`, `vulns/store.rs:290` | `ON CONFLICT (…) DO UPDATE` |
| `ON CONFLICT … DO …` / `excluded.` | 107 in 56 files / 306 | `db/targets.rs`, `vulns/store.rs` | same syntax |
| `RETURNING` | 54 in 34 files | | same |
| `last_insert_rowid()` | 10 in 5 files | `auth/users.rs:255`, `reports/store.rs:159` | `RETURNING id` |
| `json_extract` (`json_remove` in migrations) | 3 in 2 files | `collectors/agent/commands.rs:141`, `packs/mod.rs:325` | `::jsonb ->> 'k'` helper |
| `COLLATE NOCASE` | 26 in 11 files, 7 in migrations | `db/monitor_templates.rs:70`, `0007_users.sql:13` | ICU collation named `nocase` |
| `LIKE` (ASCII case-insensitive in SQLite) | 3 | `auth/audit.rs:154` (with `ESCAPE '\'`), `first_run.rs:136` | `ILIKE` where it matters |
| `IFNULL` | 2 | `backup/import/netconfig.rs:92` | `COALESCE` (also valid SQLite) |
| `PRAGMA` at run time | 7 in 4 files | `db/mod.rs:124`, `ha/standby.rs:322-326` | SQLite only |
| `VACUUM INTO` | 2 | `backup/local.rs:286`, `ha/active.rs:218` | SQLite only |
| Error text `"UNIQUE constraint failed"` | 9 in 8 files | `api/targets.rs:1553`, `api/oncall.rs:256` | `DatabaseError::is_unique_violation()` |
| `GLOB`, `julianday`, `group_concat`, `iif`, `:memory:` | 0 | | |

**Timestamps are TEXT in three formats.** Rust writes `%Y-%m-%d %H:%M:%S`
(`db::timestamp`, `db/mod.rs:31`), as does `datetime('now')` (79 uses in
code and migrations). `strftime` writes `%Y-%m-%dT%H:%M:%SZ` (67 uses),
`%Y-%m-%dT%H:%M:%S` (20) or `%Y-%m-%d %H:%M:%S` (5). Each column keeps one
format, and comparisons are string comparisons. Rust parses these strings
itself (`backup/local.rs:576`, `collectors/agent/store.rs:1358`); no column is
decoded as a `chrono` type, and the few `chrono` binds go through a
formatting helper (`auth/session.rs:372`). That is good news: keeping the
columns as TEXT on PostgreSQL, with the same bytes, keeps every comparison.

**Booleans are integers.** 29 `CHECK (x IN (0, 1))` in migrations, 252
`col = 0`/`col = 1` literals in SQL, 184 `i64::from(…)` binds, 107 `!= 0`
decodes. Two queries return a boolean expression (`SELECT EXISTS(…)`,
`api/onboarding.rs:191`): SQLite returns an integer, PostgreSQL a `bool`.

**Type affinity.** SQLite stores any value in any column and `sqlx` decodes
an `INTEGER` into `i64` whatever its size. On PostgreSQL `sqlx` refuses to
decode an `int4` into `i64`, so every integer column must be `BIGINT`. The 14
`SUM(…)` return `numeric` on PostgreSQL and need a cast.

**Schema features in the 125 migrations.** 128 `CREATE TABLE`, 149 `ALTER
TABLE` (146 `ADD COLUMN`, 1 `DROP COLUMN` in `0234`), 92 indexes of which
partial ones in 7 files (`0058_ldap_users.sql`), 53 `AUTOINCREMENT` (33
files), 85 `INTEGER PRIMARY KEY`, 9 `WITHOUT ROWID` (`0003_alerting.sql:142`),
102 `CHECK`, 117 `REFERENCES` (100 `ON DELETE CASCADE`), 31 `BLOB`, 15
`REAL`, 86 `DEFAULT (expr)` (mostly `datetime('now')`), 8 trigger statements
in 4 files (`0034`, `0054`, `0120`, `0185`, with `INSERT OR IGNORE` and
`json_extract` inside `0185`), two table rebuilds (`0013`, and `0045` with
`-- no-transaction`), and one `PRAGMA writable_schema` edit of a `CHECK`
(`0043_operator_role.sql:16`).

**Other dialect differences to audit.** `NULL` sorts first in SQLite and
last in PostgreSQL (164 `ORDER BY`, no `NULLS FIRST/LAST` today). Text
without a collation sorts bytewise in SQLite and by locale in PostgreSQL
unless the database uses the `C` locale. 8 `GROUP BY`, some with SQLite's
bare-column leniency. Read-then-write in a transaction is serialised by
SQLite's single writer and is not under PostgreSQL's `READ COMMITTED` (for
example `MAX(position) + 1`, `db/targets.rs:120`).

### Code that relies on SQLite being a file

| Where | What |
|---|---|
| `backup/local.rs` | Scheduled local backups: `VACUUM INTO` a copy plus `secret.key` in `/data/backups`, rotated. |
| `ha/active.rs:170-230`, `ha/standby.rs:300-390` | HA replica: `VACUUM INTO` the shared directory every 30 s, the standby copies it, runs `PRAGMA quick_check`, and installs it over `dumbmonit.db`, `-wal`, `-shm`. |
| `selfmon.rs:366` | Database size = file plus `-wal`. |
| `storage/mod.rs:545` | Disk usage view classifies `.db`, `-wal`, `-shm` files. |
| `demo/mod.rs:45` | Demo reset deletes the database files. |
| `privileges.rs` | Start-up walk of `/data` ownership (file-based, harmless). |
| `api/kuma_import.rs:144` | Uptime Kuma import reads an **uploaded** SQLite file. Unrelated to our backend: stays as it is. |
| `backup/` bundle (`export.rs`, `import/`) | Goes through ordinary queries: works on both backends once the queries are ported. It carries configuration, not history. |

### Tests

988 `#[tokio::test]`, 141 integration files (about 43 000 lines) in
`crates/server/tests`. 88 of them open a fresh SQLite file in a temporary
directory through `tests/common/mod.rs:54` (`setup_with`) or `db::open`;
nothing uses `:memory:`. Unit tests in `src` also open temporary files
(`alerting/engine.rs:775`, `oncall/runner.rs:867`). CI runs `cargo test
--all-features` once (`.github/workflows/ci.yml:124`); `upgrade.yml` upgrades
a real previous release.

### Deploy

- `Dockerfile:34`: builds SQLite in; stays.
- `docker-compose*.yml`: only the `/data` volume; no database setting.
- `deploy/helm/dumbmonit`: `values.yaml:1`, `templates/statefulset.yaml:8`,
  `Chart.yaml:3`, `README.md:17`, `templates/NOTES.txt:1` all say "one
  replica, SQLite has a single writer". `deploy/kubernetes/dumbmonit.yaml:8`
  says the same.
- `docs/reference/configuration.md:24`: `DUMBMONIT_DB_POOL` described as
  SQLite connections.

## 2. Options

| | A. `sqlx::Any` | B. Facade over native drivers | C. sea-query or diesel |
|---|---|---|---|
| Idea | One `AnyPool`, the driver is picked from the URL. | `db::Db` enum of `SqlitePool`/`PgPool`, with thin `query`, `Row` and `Tx` wrappers. | Queries written in a builder (sea-query) or an ORM (diesel) that emits each dialect. |
| Placeholders | Not rewritten: `?` fails on PostgreSQL, we rewrite anyway. | Rewritten once, `?`/`?N` to `$N`, cached. | Generated. |
| Types | Lowest common denominator (integers, floats, text, bytes, bool); no `chrono`, no SQLite options in code. | Native on each side; binds checked at compile time against both drivers. | Native. |
| SQLite-only features (`VACUUM INTO`, pragmas, HA replica) | Awkward: no `SqlitePool` to reach. | `Db::sqlite()` gives the native pool. | Separate code path. |
| Rewrite of 1 248 queries | Mechanical. | Mechanical. | Every query rewritten by hand, plus models for diesel. |
| Risk | Less maintained part of sqlx; runtime decode surprises. | We own about 400 lines of wrapper. | Months of work and a new dependency tree. |

**Recommended: B.** It keeps the SQL text we have, keeps SQLite exactly as it
is, and lets the compiler find the binds that cannot work on PostgreSQL
(unsigned integers, for example). Option C is rejected on cost; option A
saves the wrapper but loses typed decoding and the SQLite-specific paths, so
it is only the fallback if the spike (phase 0) shows B is harder than
expected.

### Shape of the facade

```rust
// crates/server/src/db/backend.rs (new)
#[derive(Clone)]
pub enum Db { Sqlite(SqlitePool), Postgres(PgPool) }

db::query("UPDATE targets SET last_error = ? WHERE id = ?")
    .bind(error).bind(id)
    .execute(&db).await?;
```

- `bind<T>` requires `Encode + Type` for both drivers.
- `Row::get`/`try_get` dispatch to `SqliteRow`/`PgRow`.
- `Db::begin()` returns a `Tx` that is either kind.
- `Db::dialect()` feeds a handful of helpers for the residue, for example
  `dialect.epoch("last_check_at")` or `dialect.json_text("args", "name")`.

Call sites change from `sqlx::query(…)…(pool)` to `db::query(…)…(&db)` and
from `&SqlitePool` to `&Db`, mostly by search and replace.

### Migrations: two trees, not a translation layer

Translating SQLite DDL at run time is not realistic here: triggers need
PL/pgSQL functions, `0043` edits `sqlite_master`, `0013` and `0045` rebuild
tables, and `AUTOINCREMENT`, `WITHOUT ROWID` and affinity all differ. The
proposal:

- `db/migrations/` stays the SQLite history, untouched.
- `db/migrations_pg/` starts with **one baseline** at the cut-over version
  (for example `0300_baseline.sql`), written from the SQLite schema of that
  version. Mapping: `INTEGER` to `BIGINT`, `INTEGER PRIMARY KEY
  AUTOINCREMENT` to `BIGINT GENERATED BY DEFAULT AS IDENTITY` (by default,
  so the copy tool can insert ids), `BLOB` to `BYTEA`, `REAL` to `DOUBLE
  PRECISION`, `TEXT` stays `TEXT`, timestamps stay `TEXT` with defaults
  producing the same format, integer booleans stay `BIGINT` with their
  `CHECK`, `WITHOUT ROWID` dropped, triggers rewritten as functions, and
  `COLLATE NOCASE` kept as written thanks to an ICU collation created as
  `nocase` (unquoted identifiers fold to lower case, so the SQL text is the
  same on both sides).
- After the baseline, **every new migration is written twice**, same number,
  same name, one file per tree. A test checks that both trees have the same
  versions above the baseline, and a schema test (run in the PostgreSQL CI
  job) compares tables, columns, nullability, indexes and foreign keys
  between `pragma_table_info` and `information_schema`.
- A second `sqlx::migrate!("src/db/migrations_pg")` migrator; each backend
  keeps its own `_sqlx_migrations` table.

## 3. Phases

Effort is in developer days for one developer who knows the code, including
review. Each phase ends in a releasable state; phases 1 and 2 change nothing
for users and can ship in normal releases.

| # | Phase | Days |
|---|---|---|
| 0 | Spike and decision | 3 |
| 1 | Portable SQL, on SQLite only | 8-10 |
| 2 | Facade, on SQLite only | 5-7 |
| 3 | PostgreSQL schema and migrations | 5-7 |
| 4 | PostgreSQL at run time | 4-6 |
| 5 | Tests on both backends, CI | 8-12 |
| 6 | SQLite to PostgreSQL copy tool | 4-5 |
| 7 | Deploy files and documentation | 2-3 |
| | **Total** | **39-53 days, about 8 to 11 weeks** |

**Phase 0: spike (3 days).** Write `db::Db` and port `db/targets.rs` and
`auth/users.rs` to it; write the PostgreSQL baseline for those tables; run
their tests against a local PostgreSQL. Go or no-go on option B, and a firmer
estimate for phase 5.

**Phase 1: portable SQL on SQLite (8-10 days).** Every change here is valid
SQLite and is tested by the existing suite:

- move "now" and time arithmetic to Rust: one helper per stored format
  (`db::now()` exists; add `now_iso_z()` and `now_iso()`), and cut-offs
  computed in Rust instead of `datetime('now', ?)`. About 170 sites;
- `INSERT OR IGNORE` to `ON CONFLICT DO NOTHING` (24). `INSERT OR REPLACE`
  to `ON CONFLICT (key) DO UPDATE` (5), checking each one, because `REPLACE`
  deletes the old row first (cascades, columns that fall back to defaults);
- `last_insert_rowid()` to `RETURNING id` (10), `IFNULL` to `COALESCE`;
- unique violations through `sqlx::Error::as_database_error()` and
  `is_unique_violation()` instead of the message text (9);
- casts where the result type would differ: `SUM` (14), `SELECT EXISTS` (2);
- a source test that fails on new `strftime(`, `datetime(`, `INSERT OR`,
  `last_insert_rowid` in `crates/server/src` outside an allow-list, so the
  work does not erode while the rest is done.

**Phase 2: facade on SQLite (5-7 days).** `db/backend.rs` with `Db`,
`Query`, `Row`, `Tx`. Replace `SqlitePool`, `SqliteRow` and
`Transaction<Sqlite>` across the 124 files and the tests; `AppState` keeps
one `Db` where it had the pool (`state.rs`). The SQLite-only code
(`backup/local.rs`, `ha/`, `demo/`) asks `Db::sqlite()` for the native pool.
Placeholder rewriting is a no-op on SQLite. Still one backend for users.

**Phase 3: PostgreSQL schema (5-7 days).** The baseline described above
(128 tables, 92 indexes, 8 triggers), the second migrator, the version-parity
test and the schema comparison test. The rule "write each migration twice"
goes into `CONTRIBUTING.md` and the development docs.

**Phase 4: PostgreSQL at run time (4-6 days).**

- `DUMBMONIT_DATABASE_URL` (`postgres://…`, `sslmode` honoured; password
  also from `DUMBMONIT_DATABASE_PASSWORD_FILE`). Unset: SQLite, as today.
  `DUMBMONIT_DB_POOL` applies to both (default stays 4 for SQLite, 10 for
  PostgreSQL).
- Start-up checks: server version (14 or later), `UTF8` encoding, the `C`
  collation for the database (or a clear warning), the `nocase` collation
  and the migrations.
- Placeholder rewriting and the dialect helpers switched on.
- Features that do not apply: local backups are disabled with a message
  pointing to `pg_dump`; HA keeps the lease file but drops the replica (both
  servers point to the same database, the standby still opens nothing until
  promoted); database size from `pg_database_size()` in `selfmon.rs`; the
  storage view stops listing database files; demo reset truncates tables.
- Audit read-then-write transactions (84 `begin()`): add `FOR UPDATE` or a
  unique constraint plus retry where two requests could race.
- Watch query counts on hot paths (scheduler, alerting, ingest): a query that
  costs microseconds on a local file costs a network round trip on
  PostgreSQL. Batch where a loop runs one query per target.

**Phase 5: tests on both backends (8-12 days).** The biggest unknown.

- `tests/common/mod.rs` and the unit tests open the backend given by
  `DUMBMONIT_TEST_DATABASE_URL`; with PostgreSQL each test gets its own
  database created from a migrated template (`CREATE DATABASE t_<id>
  TEMPLATE dm_template`), dropped at the end, so tests stay parallel.
- Migration-history tests (`open_up_to`, `tests/*_migration.rs`,
  `upgrade.yml`) stay SQLite only.
- CI: a second Rust test job with a `postgres` service on the latest major
  (check at the time) and one on the oldest supported, only on changes to
  `crates/server`. Coverage stays on the SQLite job.
- Fix what fails: decode mismatches, `NULL` ordering, case rules, races.

**Phase 6: copy tool (4-5 days).** `dumbmonit db copy --to
postgres://…`, run with the server stopped:

1. open the SQLite file read-only and check it is fully migrated, to the
   same version the binary expects;
2. migrate the target and refuse a target that already holds data;
3. copy every table except `_sqlx_migrations` in foreign-key order (computed
   from `pragma_foreign_key_list`), in batches, in one transaction; text,
   integers and encrypted `BLOB`s are copied byte for byte, so the same
   `secret.key` keeps decrypting them;
4. reset identity sequences to `max(id)`;
5. compare row counts per table and a checksum of the primary keys, print a
   report, and leave the SQLite file untouched as the way back.

The same tool in the other direction (`--from postgres://… --to sqlite`) is
cheap once the first exists and gives a way out; propose it as a follow-up.

**Phase 7: deploy and docs (2-3 days).** A `docker-compose.postgres.yml`
overlay example; Helm values `database.url` / `database.existingSecret` (the
"one replica" rule stays, see below); `docs/install/postgresql.md`, the
configuration reference, the backup and HA pages.

### Ongoing cost after delivery

Every migration written twice, a second CI test job (roughly doubling Rust
test time), and every new query checked on both backends. Expect 10 to 20 %
more effort on any change that touches the database.

## 4. Risks

| Risk | Mitigation |
|---|---|
| Type mismatches only show at run time (`int4` vs `i64`, `numeric` sums, `bool` expressions). | `BIGINT` everywhere, casts in phase 1, the full suite on PostgreSQL. |
| Ordering and case differences in UI lists (`NULL` first/last, locale sort). | `C` locale required, `nocase` collation, explicit `NULLS FIRST` where order matters. |
| Lost updates once SQLite's single writer is gone. | Transaction audit in phase 4; still one active server. |
| The two migration trees drift. | Version-parity and schema comparison tests in CI. |
| New SQLite-only SQL slips in during the months of work. | Source test from phase 1; phases 1-2 merged early. |
| Latency: many small queries per request or per probe round. | Measure in phase 4 with the existing load test (`load-test.yml`), batch hot loops. |
| Phase 5 takes longer than planned. | The spike measures it first; phases 1-2 are worth keeping even if PostgreSQL stops there. |

## 5. Out of scope

- **Several active servers.** The scheduler, alerting engine, relay hub,
  discovery scans, cooldowns and agents' long polls live in process memory
  (`AppState`, `state.rs:20`). With PostgreSQL there is still exactly one
  active server; the Helm chart keeps one replica. Active/active would need
  leader election (`pg_try_advisory_lock`) and shared state, a separate
  project.
- Native PostgreSQL types (`timestamptz`, `boolean`, `jsonb`): the first
  version keeps TEXT timestamps and integer booleans on purpose. A later
  migration can convert them.
- Backups of PostgreSQL from the UI (`pg_dump` is not in the image);
  replication and point-in-time recovery are the database's job.
- Metrics: VictoriaMetrics is unchanged.
- MySQL/MariaDB, `LISTEN/NOTIFY`, moving in-memory state to the database.
- The Uptime Kuma import, which reads an uploaded SQLite file whatever the
  backend.

## 6. Decisions for the owner

1. Approve option B and phase 0 (3 days), then decide on the rest with the
   spike's numbers.
2. Phases 1 and 2 can be approved alone: they make the SQL portable and
   cleaner with no user-visible change.
3. Minimum PostgreSQL version (proposal: 14) and whether the reverse copy
   tool is part of the first release.
