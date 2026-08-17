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
The `backup` binary takes one SurrealQL dump of the database, checks it, gzips it and uploads it 
to an S3 compatible bucket (Cloudflare R2), then deletes everything past `BACKUP_RETENTION`. It 
takes a backup and exits, so it can be run on a schedule.

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
entrypoint. Nothing else is in there, so it comes out around 100MB against the server's rust 
based image.

```
docker build -f Dockerfile.backup -t mapper-influences-backup .
docker run --rm --env-file .env mapper-influences-backup
```

#### Running it on a schedule in Railway
Add a service pointing at this same repo, set its Dockerfile path to `Dockerfile.backup`, and give 
it a cron schedule. Point its `SURREAL_URL` at the private domain, backups don't need the database 
to be reachable from the internet. A failed backup exits non-zero, so a broken run is reported as 
failed instead of quietly doing nothing.

The credentials are given to the container at run time and are deliberately not build arguments: 
anything baked in with `ENV` can be read back out of the image.

#### Restoring
Two things will bite you here, so they're worth knowing before you need them:

- **A 3.x `surreal` CLI cannot talk to a 2.x server.** It speaks `POST /rpc` and gets a 400. Use 
  `curl` against the http endpoints, or install a matching 2.x CLI.
- **The dumps carry no `script_migration` rows.** The schema is in there, the migration history 
  isn't, so a restored database looks unmigrated to `surrealdb-migrations` and it will try to 
  apply everything again over a database that already has the schema.

```
# Download and unpack a dump from the bucket, then
curl -X POST "$SURREAL_HTTP_URL/import" \
  -u "$SURREAL_USER:$SURREAL_PASS" \
  -H "surreal-ns: prod" -H "surreal-db: prod" \
  --data-binary @backup.surql
```

Restore into a throwaway local database once in a while. A backup you have never restored is a 
hypothesis, not a backup.

### How to run tests
Tests utilize [Testcontainers](https://testcontainers.com/) to set up a new database for each test function. 
Testcontainers is based on docker. So be sure to have docker installed.

Then run `cargo test`

Tests record the osu! API responses into files. These files are then added to the repository to allow CI to work without 
calling osu! API every time. So if you make changes to the tests, delete the files in `/tests/data` and run tests with osu! API requests.

## How to satisfy taplo (what even is it?)
[Taplo](https://taplo.tamasfe.dev/) is a toml file toolkit. You can format and check formatting of toml files. It even has an LSP!

For basic usage, run `cargo install taplo-cli --locked` and run `taplo fmt` to format the toml files.

