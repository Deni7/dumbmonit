# PostgreSQL as an optional database: plan

Status: **not started**. This page says what supporting PostgreSQL next to
SQLite would take, so the decision can be made with numbers. SQLite stays the
default and the only backend until this is done; the
[active/standby mode](../install/high-availability.md) covers server failover
without it.

## Why, and why not yet

PostgreSQL would bring:

- two **active** servers behind a load balancer, or a managed database
  (RDS, Cloud SQL, a PostgreSQL the team already runs and backs up);
- point-in-time recovery and replication from the database's own tools;
- no single-writer limit on very large instances.

It costs a second backend to keep correct forever: every new query and every
migration written twice or written portably, a CI job per backend, and
support questions about a setup most homelabs will never use. For a homelab
or a small team, SQLite plus the built-in standby is simpler and enough.

## Where the code stands (November 2026)

Counted in `crates/server/src`:

| What | Count | Note |
|---|---|---|
| Migrations (`db/migrations/*.sql`) | 100 | SQLite dialect, applied by `sqlx::migrate!`. |
| `sqlx::query*` calls | about 1 150, in 99 files | All built at runtime (`sqlx::query`, `query_as`, `query_scalar`), no compile-time `query!` macros. Placeholders are `?`. |
| `SqlitePool` / `sqlx::sqlite::*` in signatures | about 960 / 31 | The pool type is concrete everywhere: `AppState`, collectors, alerting, API handlers, tests. |
| `strftime(…)` / `datetime(…)` | 138 / 89 | Timestamps are TEXT `YYYY-MM-DDTHH:MM:SSZ`, computed in SQL. |
| `INSERT OR IGNORE` / `INSERT OR REPLACE` | 20 / 5 | No PostgreSQL equivalent spelled that way. |
| `ON CONFLICT … DO …` | 98 | Portable, but PostgreSQL needs the conflict target every time. |
| `last_insert_rowid()` | 10 | Becomes `RETURNING id` (already used 57 times). |
| `AUTOINCREMENT` | 50 | `GENERATED ALWAYS AS IDENTITY`. |
| `COLLATE NOCASE` | 32 | `citext`, or `lower()` on both sides with an index. |
| `PRAGMA` / `VACUUM INTO` | 16 / 15 | Pool tuning, `backup/local.rs`, the HA replica: SQLite only. |
| `json_extract(…)` | 4 | `->>` in PostgreSQL. |

Less visible differences to audit, file by file:

- **Typing.** SQLite accepts any value in any column; booleans are stored as
  `INTEGER` (`i64::from(bool)`), blobs and text are sometimes compared, and
  some columns hold JSON as TEXT. PostgreSQL rejects mismatches at bind time.
- **Case and collation.** `LIKE` is case-insensitive for ASCII in SQLite and
  case-sensitive in PostgreSQL (`ILIKE`).
- **Integer division, `LIMIT -1`, `IS` comparisons, `GROUP BY` leniency**:
  SQLite accepts queries PostgreSQL refuses (bare columns outside aggregates).
- **Transactions and locking.** Code that relies on SQLite's single writer
  (read-then-update without `SELECT … FOR UPDATE`) becomes a race with two
  active servers.
- **Error messages.** Unique violations are recognised by SQLite's text
  (`"UNIQUE constraint failed"`, 8 places); PostgreSQL says it otherwise. Both
  should go through `sqlx::Error::as_database_error()` and its kind.
- **Tests.** About 110 integration test files open a SQLite file in a temp
  directory; they would need a PostgreSQL per test (schema per test, or
  transactions rolled back).

## What would have to be done

1. **A backend-neutral pool.** Replace `SqlitePool` in signatures by a
   `Db` type: either `sqlx::AnyPool` (one type, but sqlx does not rewrite `?`
   into `$1`, and type support is the lowest common denominator), or an enum
   over `SqlitePool` / `PgPool` with a small query helper that takes the SQL
   for each dialect. The enum is more verbose but keeps typed decoding and
   lets the hot paths stay native. Recommended: the enum, behind a
   `db::Db` facade, migrated module by module.
2. **Placeholders and dialect.** One helper that writes `?` for SQLite and
   `$n` for PostgreSQL, used by every query; time expressions moved to Rust
   (`chrono::Utc::now()` bound as a parameter) instead of `strftime('now')`,
   which removes most of the 227 time functions at once.
3. **Migrations.** A second directory, `db/migrations/postgres`, starting from
   a squashed baseline equal to the SQLite schema at the switch, then one file
   per change in both directories. A CI check compares both schemas
   (`information_schema` vs `sqlite_master`) so they cannot drift.
4. **SQLite-only features.** Local backups (`VACUUM INTO`) and the HA replica
   do not apply to PostgreSQL: the backup page points to `pg_dump`, HA becomes
   "two servers, one database" with an advisory lock (`pg_try_advisory_lock`)
   electing the one that runs the scheduler, alerting and agents' long polls,
   while both serve the UI.
5. **In-memory state.** Relay hub, multi-site rounds, discovery scans and
   cooldowns live in process memory (`AppState`). With two active servers,
   agents' long polls and relayed probes must stick to the elected server, or
   move to the database (`LISTEN/NOTIFY`).
6. **Moving an instance.** `dumbmonit db copy --to postgres://…`: reads every
   table from SQLite and writes it in dependency order, inside one
   transaction, then checks row counts. The encrypted columns move as they
   are (same instance secret).
7. **CI and docs.** A PostgreSQL service in CI running the whole test suite a
   second time, a `DUMBMONIT_DATABASE_URL` setting, and documentation.

## Estimate

| Step | Effort |
|---|---|
| Facade, placeholder helper, time in Rust (1–2) | 1 week |
| Porting about 1 150 queries, module by module, with review | 3 to 4 weeks |
| PostgreSQL baseline migration, schema comparison in CI (3) | 1 week |
| Backups, HA with an advisory lock, in-memory state (4–5) | 1 to 2 weeks |
| Copy tool, test suite on both backends, docs (6–7) | 1 to 2 weeks |
| **Total** | **7 to 10 developer-weeks**, then a permanent tax on every change |

## Recommendation

Not now. The built-in active/standby mode gives a homelab or a small team a
second server with no new dependency. Reconsider PostgreSQL when an instance
needs two active servers, a managed database, or outgrows SQLite's single
writer (well beyond the 5 000 monitors of [Sizing](../reference/sizing.md),
where VictoriaMetrics, not SQLite, is the first limit).
