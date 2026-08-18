#!/usr/bin/env bash
#
# Takes one backup and exits, so it can be run on a schedule.
#
#   export -> check -> gzip -> upload -> download it back -> restore it -> prune
#
# The dump is fetched back out of the bucket and restored into a surrealdb
# started here in memory, with `restore.sh`, before any older backup is deleted.
# That way the upload is part of what gets tested, the restore is tested with the
# same script the runbook tells you to use, and a backup that doesn't restore
# never costs us one that does.
#
# Configuration, from the environment or from .env:
#   SURREAL_URL or SURREAL_HTTP_URL     the database to back up
#   SURREAL_USER, SURREAL_PASS          root credentials
#   SURREAL_NAMESPACE, SURREAL_DATABASE default to prod / prod
#   R2_ENDPOINT, R2_BUCKET              where the dumps go
#   R2_ACCESS_KEY_ID, R2_SECRET_ACCESS_KEY
#   BACKUP_PREFIX                       folder inside the bucket, default surrealdb
#   BACKUP_RETENTION                    how many dumps to keep, default 30
#   BACKUP_VERIFY=false                 skip the restore check
#   VERIFY_NAMESPACE, VERIFY_DATABASE   names the check restores under, both
#                                       default to restore_check
#   DISCORD_WEBHOOK_URL                 optional, posts the outcome of every run
#   LOGS_URL, R2_CONSOLE_URL            optional, override the links in that post
#   DISCORD_FAILURE_MENTION             optional, a role id (or @here / @everyone)
#                                       to ping when a run fails
set -euo pipefail

for tool in curl jq gzip mc; do
    command -v "$tool" >/dev/null || { echo "$tool is required" >&2; exit 2; }
done

if [ -f .env ]; then
    set -a
    # shellcheck disable=SC1091
    . ./.env
    set +a
fi

: "${SURREAL_USER:?set SURREAL_USER}"
: "${SURREAL_PASS:?set SURREAL_PASS}"
: "${R2_ENDPOINT:?set R2_ENDPOINT}"
: "${R2_ACCESS_KEY_ID:?set R2_ACCESS_KEY_ID}"
: "${R2_SECRET_ACCESS_KEY:?set R2_SECRET_ACCESS_KEY}"

# Cloudflare shows the S3 API endpoint with the bucket already on the end, and
# that is what people paste. mc wants the host on its own, so split whichever
# form we were given: the host becomes the endpoint, and any path is the bucket
# unless R2_BUCKET says otherwise.
ENDPOINT_HOST=$(printf '%s' "$R2_ENDPOINT" | sed -E 's#^([a-zA-Z][a-zA-Z0-9+.-]*://[^/]+).*#\1#')
ENDPOINT_PATH=$(printf '%s' "$R2_ENDPOINT" | sed -E 's#^[a-zA-Z][a-zA-Z0-9+.-]*://[^/]+##; s#^/##; s#/$##')

if [ -n "$ENDPOINT_PATH" ] && [ -n "${R2_BUCKET:-}" ] && [ "$ENDPOINT_PATH" != "$R2_BUCKET" ]; then
    echo "[backup] R2_ENDPOINT ends in /$ENDPOINT_PATH but R2_BUCKET is $R2_BUCKET, using $R2_BUCKET" >&2
fi
R2_BUCKET=${R2_BUCKET:-$ENDPOINT_PATH}
R2_ENDPOINT=$ENDPOINT_HOST

if [ -z "$R2_BUCKET" ]; then
    echo "set R2_BUCKET, or put the bucket on the end of R2_ENDPOINT" >&2
    exit 2
fi

NAMESPACE=${SURREAL_NAMESPACE:-prod}
DATABASE=${SURREAL_DATABASE:-prod}
PREFIX=${BACKUP_PREFIX:-surrealdb}
RETENTION=${BACKUP_RETENTION:-30}
RESTORE_SCRIPT=${RESTORE_SCRIPT:-/usr/local/bin/restore.sh}
SURREAL_BINARY=${SURREAL_BINARY:-surreal}
VERIFY_PORT=${VERIFY_PORT:-18100}
# The restore check runs against its own throwaway database, under its own names.
# It is already a separate surrealdb process, in memory, bound to localhost, but
# nothing here should even read as if it could be pointed at the live data.
VERIFY_NAMESPACE=${VERIFY_NAMESPACE:-restore_check}
VERIFY_DATABASE=${VERIFY_DATABASE:-restore_check}
MINIMUM_DUMP_BYTES=512
DISCORD_WEBHOOK_URL=${DISCORD_WEBHOOK_URL:-}
DISCORD_FAILURE_MENTION=${DISCORD_FAILURE_MENTION:-}

# What the run is busy with, so a failure can say where it fell over. Every step
# sets it before doing anything that can fail.
STEP="starting up"

# Somewhere to click from the notification. Railway hands the ids to every
# container it runs, and the R2 account id is the first label of the endpoint
# host. Either can be overridden, dashboards move around.
if [ -z "${LOGS_URL:-}" ] && [ -n "${RAILWAY_PROJECT_ID:-}" ] && [ -n "${RAILWAY_SERVICE_ID:-}" ]; then
    LOGS_URL="https://railway.com/project/$RAILWAY_PROJECT_ID/service/$RAILWAY_SERVICE_ID"
    [ -n "${RAILWAY_ENVIRONMENT_ID:-}" ] && LOGS_URL="$LOGS_URL?environmentId=$RAILWAY_ENVIRONMENT_ID"
fi

if [ -z "${R2_CONSOLE_URL:-}" ]; then
    R2_ACCOUNT=$(printf '%s' "$R2_ENDPOINT" | sed -nE 's#^https?://([0-9a-f]+)\.r2\.cloudflarestorage\.com/?$#\1#p')
    [ -n "$R2_ACCOUNT" ] && R2_CONSOLE_URL="https://dash.cloudflare.com/$R2_ACCOUNT/r2/default/buckets/$R2_BUCKET"
fi

# Discord renders these as markdown links. A run that can't work out either one
# just doesn't carry it, rather than posting something that goes nowhere.
links() {
    LINKS=""
    [ -n "${R2_CONSOLE_URL:-}" ] && LINKS="[bucket]($R2_CONSOLE_URL)"
    if [ -n "${LOGS_URL:-}" ]; then
        [ -n "$LINKS" ] && LINKS="$LINKS · "
        LINKS="$LINKS[logs]($LOGS_URL)"
    fi
    [ -n "$LINKS" ] && printf '\n\n%s' "$LINKS"
}

# Discord is told how the run went, if a webhook was configured. A webhook that
# doesn't answer is not allowed to fail a backup that worked, and the url is a
# secret in its own right so it never gets echoed.
#
# The fourth argument is a mention to ping, and it has to ride in `content`:
# a mention written inside an embed renders as a link but notifies nobody, which
# is a quiet way to build an alert that never reaches anyone. `allowed_mentions`
# is always sent, so this can only ever ping the one thing it was asked to.
notify() {
    [ -z "$DISCORD_WEBHOOK_URL" ] && return 0

    CONTENT=""
    ALLOWED='{"parse":[]}'
    case "${4:-}" in
        "") ;;
        @here | @everyone)
            CONTENT="$4"
            ALLOWED='{"parse":["everyone"]}'
            ;;
        *[!0-9]*)
            # Already a full mention, or something we don't recognise. Passed
            # through as written, with nothing whitelisted to ping.
            CONTENT="$4"
            ;;
        *)
            CONTENT="<@&$4>"
            ALLOWED=$(jq -nc --arg role "$4" '{parse:[],roles:[$role]}')
            ;;
    esac

    PAYLOAD=$(jq -n --arg content "$CONTENT" --arg title "$1" --arg body "$2" \
        --argjson color "$3" --argjson allowed "$ALLOWED" \
        '{content:$content,allowed_mentions:$allowed,
          embeds:[{title:$title,description:$body,color:$color}]}')
    curl -sS -m 15 -X POST -H "Content-Type: application/json" \
        -d "$PAYLOAD" "$DISCORD_WEBHOOK_URL" >/dev/null 2>&1 \
        || echo "[backup] could not reach the discord webhook" >&2
}

# The app is configured with a websocket url, the export endpoint is http on the
# same host
if [ -z "${SURREAL_HTTP_URL:-}" ]; then
    : "${SURREAL_URL:?set SURREAL_HTTP_URL or SURREAL_URL}"
    SURREAL_HTTP_URL=$(printf '%s' "$SURREAL_URL" | sed -e 's#/rpc$##' -e 's#^ws://#http://#' -e 's#^wss://#https://#')
fi

WORK_DIR=$(mktemp -d)
SUMMARY=""
finish() {
    EXIT_CODE=$?
    [ -n "${SCRATCH_PID:-}" ] && kill "$SCRATCH_PID" 2>/dev/null || true
    rm -rf "$WORK_DIR"

    # The prefix is in the title, so two schedules writing to the same channel
    # are told apart at a glance rather than by reading the key
    if [ "$EXIT_CODE" = "0" ]; then
        notify "Backup ok ($PREFIX)" "$SUMMARY$(links)" 3066993
    else
        notify "Backup failed ($PREFIX)" \
            "Fell over while $STEP, exit code $EXIT_CODE.$(links)" \
            15158332 "$DISCORD_FAILURE_MENTION"
    fi
}
trap finish EXIT

# mc keeps its config in $HOME by default, which isn't always writable
MC="mc --config-dir $WORK_DIR/.mc --quiet"
$MC alias set store "$R2_ENDPOINT" "$R2_ACCESS_KEY_ID" "$R2_SECRET_ACCESS_KEY" --api S3v4 >/dev/null
TARGET="store/$R2_BUCKET/$PREFIX"

# ---------------------------------------------------------------- export
STEP="exporting $NAMESPACE/$DATABASE"
echo "[backup] exporting $NAMESPACE/$DATABASE from $SURREAL_HTTP_URL"
STATUS=$(curl -sS --max-time 1800 -X GET "$SURREAL_HTTP_URL/export" \
    -u "$SURREAL_USER:$SURREAL_PASS" \
    -H "surreal-ns: $NAMESPACE" -H "surreal-db: $DATABASE" \
    -H "Accept: application/octet-stream" \
    -o "$WORK_DIR/dump.surql" -w '%{http_code}')

if [ "$STATUS" != "200" ]; then
    echo "[backup] the export endpoint answered with $STATUS" >&2
    exit 1
fi

# A dump nobody checked is a guess. These catch the realistic failures: an empty
# answer, an error page, a connection cut halfway.
DUMP_BYTES=$(wc -c < "$WORK_DIR/dump.surql")
if [ "$DUMP_BYTES" -lt "$MINIMUM_DUMP_BYTES" ]; then
    echo "[backup] the dump is $DUMP_BYTES bytes, expected at least $MINIMUM_DUMP_BYTES" >&2
    exit 1
fi
grep -q "OPTION IMPORT;" "$WORK_DIR/dump.surql" || {
    echo "[backup] the dump has no 'OPTION IMPORT;' header, it isn't a surrealql export" >&2
    exit 1
}
case "$(tail -c 200 "$WORK_DIR/dump.surql" | tr -d '[:space:]' | tail -c 1)" in
    ";") ;;
    *) echo "[backup] the dump doesn't end on a complete statement, it was cut short" >&2; exit 1 ;;
esac

# ---------------------------------------------------------------- upload
STEP="uploading the dump"
KEY="$NAMESPACE-$DATABASE-$(date -u +%Y%m%dT%H%M%SZ).surql.gz"
gzip -c "$WORK_DIR/dump.surql" > "$WORK_DIR/$KEY"
UPLOADED_BYTES=$(wc -c < "$WORK_DIR/$KEY")
echo "[backup] uploading $PREFIX/$KEY ($DUMP_BYTES bytes, $UPLOADED_BYTES compressed)"
$MC cp "$WORK_DIR/$KEY" "$TARGET/$KEY"

# ---------------------------------------------------------------- verify
if [ "${BACKUP_VERIFY:-true}" = "false" ]; then
    echo "[backup] restore check skipped, this dump goes out unverified"
else
    STEP="downloading $PREFIX/$KEY back"
    echo "[backup] downloading $PREFIX/$KEY back to restore it"
    $MC cp "$TARGET/$KEY" "$WORK_DIR/verify-$KEY"

    STEP="restoring $PREFIX/$KEY to check it"
    echo "[backup] starting a scratch surrealdb in memory on $VERIFY_PORT," \
        "restoring into $VERIFY_NAMESPACE/$VERIFY_DATABASE"
    "$SURREAL_BINARY" start --user verify --pass verify \
        --bind "127.0.0.1:$VERIFY_PORT" memory >"$WORK_DIR/scratch.log" 2>&1 &
    SCRATCH_PID=$!

    for _ in $(seq 1 60); do
        if curl -sf "http://127.0.0.1:$VERIFY_PORT/health" >/dev/null 2>&1; then break; fi
        sleep 0.5
    done
    curl -sf "http://127.0.0.1:$VERIFY_PORT/health" >/dev/null || {
        echo "[backup] the scratch database never came up" >&2
        exit 1
    }

    SURREAL_HTTP_URL="http://127.0.0.1:$VERIFY_PORT" \
    SURREAL_URL="ws://127.0.0.1:$VERIFY_PORT" \
    SURREAL_USER=verify SURREAL_PASS=verify \
    SURREAL_NAMESPACE="$VERIFY_NAMESPACE" SURREAL_DATABASE="$VERIFY_DATABASE" \
        "$RESTORE_SCRIPT" "$WORK_DIR/verify-$KEY"

    scratch_count() {
        curl -sS -X POST "http://127.0.0.1:$VERIFY_PORT/sql" -u "verify:verify" \
            -H "surreal-ns: $VERIFY_NAMESPACE" -H "surreal-db: $VERIFY_DATABASE" \
            -H "Accept: application/json" \
            --data "SELECT count() FROM $1 GROUP ALL;" | jq -r '.[0].result[0].count // 0'
    }

    # A dump of an empty database restores perfectly and is worth nothing
    for table in user influenced_by; do
        COUNT=$(scratch_count "$table")
        if [ "$COUNT" = "0" ]; then
            echo "[backup] $table came back empty, this dump is not a backup" >&2
            exit 1
        fi
        echo "[backup] restored $table: $COUNT"
        RESTORED="${RESTORED:-}$table $COUNT, "
    done

    kill "$SCRATCH_PID" 2>/dev/null || true
    SCRATCH_PID=""
fi

# ---------------------------------------------------------------- prune
# Last on purpose: an upload or a restore that failed must never cost us an older
# backup that is still good. The keys are timestamped, so they sort oldest first.
STEP="pruning old backups"
# Only keys this script writes are candidates. Retention counts per prefix, so two
# schedules sharing a bucket (hourly keeping 24, daily keeping 30) each prune their
# own folder and leave the other alone, and anything else stored in the bucket is
# never up for deletion.
KEYS=$($MC ls "$TARGET/" | awk '{print $NF}' \
    | grep -E '^.+-[0-9]{8}T[0-9]{6}Z\.surql\.gz$' | sort || true)
TOTAL=$(printf '%s\n' "$KEYS" | grep -c . || true)
if [ "$TOTAL" -gt "$RETENTION" ]; then
    printf '%s\n' "$KEYS" | head -n "$((TOTAL - RETENTION))" | while read -r old; do
        [ -z "$old" ] && continue
        echo "[backup] removing the expired backup $old"
        $MC rm "$TARGET/$old" >/dev/null || echo "[backup] could not remove $old" >&2
    done
    PRUNED=$((TOTAL - RETENTION))
fi

RESTORED_SUMMARY=${RESTORED:-not checked}
SUMMARY="\`$PREFIX/$KEY\`
$DUMP_BYTES bytes, $UPLOADED_BYTES compressed
restored ${RESTORED_SUMMARY%, }
${PRUNED:-0} old backup(s) removed"

echo "[backup] done: $PREFIX/$KEY"
