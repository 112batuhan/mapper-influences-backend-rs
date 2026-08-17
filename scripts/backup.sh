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
: "${R2_BUCKET:?set R2_BUCKET}"
: "${R2_ACCESS_KEY_ID:?set R2_ACCESS_KEY_ID}"
: "${R2_SECRET_ACCESS_KEY:?set R2_SECRET_ACCESS_KEY}"

NAMESPACE=${SURREAL_NAMESPACE:-prod}
DATABASE=${SURREAL_DATABASE:-prod}
PREFIX=${BACKUP_PREFIX:-surrealdb}
RETENTION=${BACKUP_RETENTION:-30}
RESTORE_SCRIPT=${RESTORE_SCRIPT:-/usr/local/bin/restore.sh}
SURREAL_BINARY=${SURREAL_BINARY:-surreal}
VERIFY_PORT=${VERIFY_PORT:-18100}
MINIMUM_DUMP_BYTES=512

# The app is configured with a websocket url, the export endpoint is http on the
# same host
if [ -z "${SURREAL_HTTP_URL:-}" ]; then
    : "${SURREAL_URL:?set SURREAL_HTTP_URL or SURREAL_URL}"
    SURREAL_HTTP_URL=$(printf '%s' "$SURREAL_URL" | sed -e 's#/rpc$##' -e 's#^ws://#http://#' -e 's#^wss://#https://#')
fi

WORK_DIR=$(mktemp -d)
cleanup() {
    [ -n "${SCRATCH_PID:-}" ] && kill "$SCRATCH_PID" 2>/dev/null || true
    rm -rf "$WORK_DIR"
}
trap cleanup EXIT

# mc keeps its config in $HOME by default, which isn't always writable
MC="mc --config-dir $WORK_DIR/.mc --quiet"
$MC alias set store "$R2_ENDPOINT" "$R2_ACCESS_KEY_ID" "$R2_SECRET_ACCESS_KEY" --api S3v4 >/dev/null
TARGET="store/$R2_BUCKET/$PREFIX"

# ---------------------------------------------------------------- export
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
KEY="$NAMESPACE-$DATABASE-$(date -u +%Y%m%dT%H%M%SZ).surql.gz"
gzip -c "$WORK_DIR/dump.surql" > "$WORK_DIR/$KEY"
UPLOADED_BYTES=$(wc -c < "$WORK_DIR/$KEY")
echo "[backup] uploading $PREFIX/$KEY ($DUMP_BYTES bytes, $UPLOADED_BYTES compressed)"
$MC cp "$WORK_DIR/$KEY" "$TARGET/$KEY"

# ---------------------------------------------------------------- verify
if [ "${BACKUP_VERIFY:-true}" = "false" ]; then
    echo "[backup] restore check skipped, this dump goes out unverified"
else
    echo "[backup] downloading $PREFIX/$KEY back to restore it"
    $MC cp "$TARGET/$KEY" "$WORK_DIR/verify-$KEY"

    echo "[backup] starting a scratch surrealdb in memory on $VERIFY_PORT"
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
    SURREAL_NAMESPACE="$NAMESPACE" SURREAL_DATABASE="$DATABASE" \
        "$RESTORE_SCRIPT" "$WORK_DIR/verify-$KEY"

    scratch_count() {
        curl -sS -X POST "http://127.0.0.1:$VERIFY_PORT/sql" -u "verify:verify" \
            -H "surreal-ns: $NAMESPACE" -H "surreal-db: $DATABASE" -H "Accept: application/json" \
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
    done

    kill "$SCRATCH_PID" 2>/dev/null || true
    SCRATCH_PID=""
fi

# ---------------------------------------------------------------- prune
# Last on purpose: an upload or a restore that failed must never cost us an older
# backup that is still good. The keys are timestamped, so they sort oldest first.
KEYS=$($MC ls "$TARGET/" | awk '{print $NF}' | sort)
TOTAL=$(printf '%s\n' "$KEYS" | grep -c . || true)
if [ "$TOTAL" -gt "$RETENTION" ]; then
    printf '%s\n' "$KEYS" | head -n "$((TOTAL - RETENTION))" | while read -r old; do
        [ -z "$old" ] && continue
        echo "[backup] removing the expired backup $old"
        $MC rm "$TARGET/$old" >/dev/null || echo "[backup] could not remove $old" >&2
    done
fi

echo "[backup] done: $PREFIX/$KEY"
