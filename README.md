<p align=center>
    <a href=https://www.mapperinfluences.com>
    <img src=https://github.com/aticie/Mapper-Influences-Backend/assets/36697363/9386b5e7-bd1c-41f1-bb47-398cca2c7b6b>
    </a>
</p>
<p align=center>
    <a href=https://www.mapperinfluences.com>https://www.mapperinfluences.com</a>
</p>

---


Mapper influences backend code.
This is actually a rewrite of [this repository](https://github.com/aticie/Mapper-Influences-Backend). 

This implementation has more complete responses, optimizes osu! API calls and uses SurrealDB instead MongoDB as database.
I'm more comfortable with rust and strong types so that's going to make things easier for me going forward.

`/docs` for endpoint documentation.

If you have feature requests or bug reports, 
you can do so in [frontend repository](https://github.com/Fursum/mapper-influences-frontend) 
or in our [discord](https://discord.gg/SAwxBDe3Rf)
## How to run

#### Easiest way would be to use docker:
- Copy `.env.example` and change the name to `.env` 
- Fill it with your credentials.
- Use `docker compose up` to run the project..

You might only want to run the database and the cache in docker, to do that just use 
`docker compose up surrealdb redis -d`

#### Cache
Responses from the osu! API, leaderboards and the graph data are cached in redis. Point
`REDIS_URL` at your instance, every entry has a TTL so nothing needs invalidating by hand.

The country agnostic leaderboards and the graph data are also rebuilt in the background, each by
its own task: every 5 minutes for the leaderboards, 20 for the heavier graph query. They all run
once at startup, then keep 20 seconds apart so their queries don't pile onto the database at the
same time. Every TTL outlives its own interval, so a failed rebuild keeps serving the previous
data instead of emptying the cache. `CACHE_REFRESH=false` turns the tasks off and everything falls
back to being filled on demand, which is what country specific leaderboards always do.

#### To run locally
`cargo run --release`

#### What is `conversion.rs` for?
It's a script to insert MongoDB data into SurrealDB. Don't use in production. I'm going to delete it after the migration is complete.

`cargo run --bin conversion`

## Backups
`scripts/backup.sh` takes one dump and exits, so it can be run on a schedule:

    export -> check it -> gzip -> upload to R2 -> download it back -> restore it -> prune old ones

The dumps are SurrealQL text rather than volume snapshots. A snapshot is tied to the storage format
the database wrote it in and stops being restorable once a newer version upgrades that format on
disk, which is how a database gets lost. A dump imports into any SurrealDB.

It restores every dump before that dump counts as taken: downloading it back out of the bucket
(so the upload is tested too), into a SurrealDB started in memory in the same container, under the
`restore_check` namespace and database, with `scripts/restore.sh` (so the script the runbook uses
is the script that gets tested). That scratch database is a separate process bound to localhost and
never shares a name with the live one, so the check cannot reach real data. Pruning
happens last, so an upload or a restore that failed never costs an older backup that still works.
A failed run exits non-zero. Set `DISCORD_WEBHOOK_URL` (channel settings → Integrations →
Webhooks) and every run posts its outcome: the key, the sizes and what the restore check found, or
which step it fell over on. A webhook that doesn't answer is logged and otherwise ignored, since a
backup that worked shouldn't be reported as failed because Discord was down. The post links to the
Railway logs and the R2 bucket, built from the ids Railway gives every container and from the
account id in `R2_ENDPOINT`. Neither link appears if it can't be worked out, and `LOGS_URL` /
`R2_CONSOLE_URL` override them if a dashboard moves.

| Variable | |
|---|---|
| `SURREAL_URL` | the database to back up, `SURREAL_HTTP_URL` overrides it |
| `SURREAL_USER`, `SURREAL_PASS` | root credentials |
| `SURREAL_NAMESPACE`, `SURREAL_DATABASE` | default `prod` / `prod` |
| `R2_ENDPOINT` | `https://<account-id>.r2.cloudflarestorage.com`, with or without the bucket on the end |
| `R2_BUCKET` | optional if `R2_ENDPOINT` already ends with it |
| `R2_ACCESS_KEY_ID`, `R2_SECRET_ACCESS_KEY` | |
| `BACKUP_PREFIX` | folder inside the bucket, default `surrealdb` |
| `BACKUP_RETENTION` | how many dumps to keep, default 30 |
| `BACKUP_VERIFY` | `false` skips the restore check |
| `VERIFY_NAMESPACE`, `VERIFY_DATABASE` | what the check restores under, both default `restore_check` |
| `DISCORD_WEBHOOK_URL` | optional, posts the outcome of every run to a channel |
| `LOGS_URL`, `R2_CONSOLE_URL` | optional, override the links in that post |

The R2 values come from the Cloudflare dashboard under R2 Object Storage: create a bucket, copy its
**S3 API** endpoint as shown (it has the bucket name on the end, which the script splits off), then
**Manage R2 API tokens** → create one with **Object Read & Write** scoped to that bucket.

#### Running it
`Dockerfile.backup` is the cron image: debian with `curl`, `jq`, `gzip`, the `surreal` binary for
the scratch database, `mc` for the bucket, and the two scripts. Nothing is compiled, so it builds
in seconds and comes out around 190MB.

```
docker build -f Dockerfile.backup -t mapper-influences-backup .
docker run --rm --env-file .env mapper-influences-backup
```


#### Restoring
`scripts/restore.sh <dump-file>` puts a dump back, gzipped or not, reading the same `SURREAL_*`
variables as everything else. `RESTORE_FORCE=true` allows a target that already holds records.

Four things about restoring a SurrealDB dump, all of which the script handles:

- **It imports twice.** The export writes tables alphabetically, so the `influenced_by` edges come
  before the `user` records they point at, and that table is `TYPE RELATION ... ENFORCED`, so
  SurrealDB rejects an edge whose vertices don't exist yet and one bad reference fails the whole
  batched statement. A single pass restores every user and activity and silently drops the entire
  influence graph, which looks perfectly healthy unless you count. The second pass puts the edges
  back, and the script fails if the dump holds edges and none of them made it.
- **A 200 doesn't mean it worked.** The import reports failures inside the response body, one
  result per statement. `already exists` on the second pass is expected, anything else is a failed
  restore.
- **Restore into an empty namespace and swap over.** A dump merges into whatever is already there
  rather than replacing it.
- **The dumps carry no `script_migration` rows**, so a restored database looks unmigrated to
  `surrealdb-migrations` even though the schema is in place.

A 3.x `surreal` CLI cannot talk to a 2.x server, which is why these are curl scripts rather than
`surreal export` / `surreal import`.

### How to run tests
Tests utilize [Testcontainers](https://testcontainers.com/) to set up a new database for each test function. 
Testcontainers is based on docker. So be sure to have docker installed.

Then run `cargo test`

Tests record the osu! API responses into files. These files are then added to the repository to allow CI to work without 
calling osu! API every time. So if you make changes to the tests, delete the files in `/tests/data` and run tests with osu! API requests.

## How to satisfy taplo (what even is it?)
[Taplo](https://taplo.tamasfe.dev/) is a toml file toolkit. You can format and check formatting of toml files. It even has an LSP!

For basic usage, run `cargo install taplo-cli --locked` and run `taplo fmt` to format the toml files.
