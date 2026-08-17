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
Responses from the osu! API, leaderboards and the graph data are cached in redis. 
Point `REDIS_URL` to your instance, every entry gets a TTL so no manual invalidation is needed.

The country agnostic leaderboards and the graph data are rebuilt in the background, so requests 
don't have to wait for a cache miss to be filled. Each cached entry gets its own updater task 
with its own interval: 5 minutes for the leaderboards, 20 minutes for the heavier graph query. 
All of them run once at startup to warm a cold cache. After that the rebuilds are database 
heavy enough to keep out of each other's way, so they are spaced 20 seconds apart for every 
cycle that follows. Every TTL is longer than the interval it belongs to, 
which means a failed rebuild keeps serving the previous data instead of falling back to an empty 
cache. Set `CACHE_REFRESH=false` to turn the updaters off, everything then falls back to being 
filled on demand. Country specific leaderboards are always filled on demand.

#### To run locally
`cargo run --release`

#### What is `conversion.rs` for?
It's a script to insert MongoDB data into SurrealDB. Don't use in production. I'm going to delete it after the migration is complete.

`cargo run --bin conversion`

## Backups
The `backup` binary takes one SurrealQL dump of the database, checks it, gzips it, uploads it to an 
S3 compatible bucket (Cloudflare R2), restores it again to prove it works, and only then deletes 
everything past `BACKUP_RETENTION`. It takes a backup and exits, so it can be run on a schedule.

These dumps are plain text, which is the point of them. A volume snapshot is tied to the storage 
format the database wrote it in, so it stops being restorable the moment a newer version upgrades 
that format on disk. A dump imports into any SurrealDB.

```
cargo run --bin backup
```

It reads `SURREAL_URL`, `SURREAL_USER` and `SURREAL_PASS` (the same ones the server uses, the 
export endpoint is http on the same host) plus the `R2_*` variables in `.env.example`. Set 
`SURREAL_HTTP_URL` if the database isn't in the usual place, and `SURREAL_NAMESPACE` / 
`SURREAL_DATABASE` if they aren't `prod`.

#### Where the R2 credentials come from
In the Cloudflare dashboard, under R2 Object Storage:

1. Create a bucket. Its name goes in `R2_BUCKET`.
2. The bucket's settings page shows its **S3 API** endpoint, which is your account id followed by 
   `r2.cloudflarestorage.com`. `R2_ENDPOINT` is that url **without** the bucket name on the end, 
   so `https://<account-id>.r2.cloudflarestorage.com`.
3. **Manage R2 API tokens** → create an API token. **Object Read & Write** is enough, scoped to 
   that one bucket. The bucket is created by hand in step 1, so the token never needs admin rights.
4. The token page shows the **Access Key ID** and the **Secret Access Key** → `R2_ACCESS_KEY_ID` 
   and `R2_SECRET_ACCESS_KEY`. The secret is shown once, so save it before leaving the page.

`R2_REGION` stays `auto`; R2 has no regions but the S3 protocol insists on one being set.

It has its own image, `Dockerfile.backup`, which builds only this binary and runs it as the 
entrypoint, alongside the `surreal` binary and the restore script it checks its own work with. 
That comes out around 165MB, against the server's rust based image.

```
docker build -f Dockerfile.backup -t mapper-influences-backup .
docker run --rm --env-file .env mapper-influences-backup
```


#### Restoring
`scripts/restore.sh` puts a dump back. It takes the file, gzipped or not, and reads the same 
`SURREAL_*` variables as everything else, from the environment or from `.env`:

```
scripts/restore.sh surrealdb-prod-prod-20260818T000000Z.surql.gz
```

```
SURREAL_HTTP_URL=http://localhost:8000 SURREAL_USER=root SURREAL_PASS=root \
SURREAL_NAMESPACE=restore SURREAL_DATABASE=restore \
  scripts/restore.sh dump.surql
```

It unpacks the dump if needed, refuses to run against a database that already holds records, 
imports, checks the result and prints what landed:

```
restoring 23M into prod/prod at http://127.0.0.1:18002
import pass 1 ...
  123/146 statements applied
import pass 2 ...
  17/146 statements applied

restored:
  user             5363
  influenced_by    16006
  activity         71000
```

**Why it imports twice.** The export writes tables alphabetically, so the `influenced_by` edges 
come before the `user` records they point at, and SurrealDB rejects an edge whose vertices don't 
exist yet. One bad reference fails the whole batched statement, so a single pass restores every 
user and activity and silently drops the entire influence graph — the tables that are left look 
perfectly healthy unless you count. The second pass, with the users in place, puts the edges back. 
The script fails loudly if the dump holds edges and none of them made it.

**Why it reads the response.** The import answers `200` even when statements inside it failed; the 
errors are in the body, one result per statement. The script treats `already exists` on the second 
pass as expected (it re-runs every schema definition) and anything else as a failed restore.

Three more things worth knowing:

- **Restore into an empty namespace and swap over.** A dump merges into whatever is already there 
  rather than replacing it. The script refuses a non-empty target unless `RESTORE_FORCE=true`.
- **A 3.x `surreal` CLI cannot talk to a 2.x server.** It speaks `POST /rpc` and gets a 400, which 
  is why this is a curl script and not `surreal import`.
- **The dumps carry no `script_migration` rows.** The schema is in there, the migration history 
  isn't, so a restored database looks unmigrated to `surrealdb-migrations` and it will try to 
  apply everything again over a database that already has the schema.

#### Every backup is restored before it counts
A backup nobody has restored is a hypothesis, so the job doesn't trust its own work. After 
uploading, it downloads the dump back out of the bucket, starts a SurrealDB **in memory inside the 
same container**, restores into it with the very same `scripts/restore.sh` above, and checks that 
the users, influences and activities all came back. Only then does it prune. A dump that doesn't 
restore fails the run and takes no older backup down with it.

```
INFO backup: uploading surrealdb/prod-prod-20260818T000000Z.surql.gz (6449 bytes, 1401 compressed)
INFO backup: restoring surrealdb/prod-prod-20260818T000000Z.surql.gz to check it
INFO backup: restore check passed: [("user", 40), ("influenced_by", 39), ("activity", 1)]
```

Downloading it back rather than keeping the dump in memory is deliberate: it makes the upload part 
of what gets tested. Using the restore script rather than a second implementation is deliberate 
too, a private copy of the restore logic would only prove the private copy works.

This needs no docker, which is why it can run as a plain cron service. `Dockerfile.backup` carries 
the `surreal` binary for the scratch database, and the restore script with the `curl` and `jq` it 
needs. Restoring the whole production database this way peaks around 400MB of memory.

Set `BACKUP_VERIFY=false` to skip it, for running the binary somewhere without those pieces. The 
run then warns that the dumps are going out unchecked.

### How to run tests
Tests utilize [Testcontainers](https://testcontainers.com/) to set up a new database for each test function. 
Testcontainers is based on docker. So be sure to have docker installed.

Then run `cargo test`

Tests record the osu! API responses into files. These files are then added to the repository to allow CI to work without 
calling osu! API every time. So if you make changes to the tests, delete the files in `/tests/data` and run tests with osu! API requests.

## How to satisfy taplo (what even is it?)
[Taplo](https://taplo.tamasfe.dev/) is a toml file toolkit. You can format and check formatting of toml files. It even has an LSP!

For basic usage, run `cargo install taplo-cli --locked` and run `taplo fmt` to format the toml files.
