#!/usr/bin/env bash
#
# Restores a dump written by the `backup` binary, or by a manual GET /export.
#
# Three things about restoring a surrealdb dump are easy to get wrong, and this
# script exists so nobody has to remember them at the worst possible moment:
#
#  1. The import runs TWICE. The export writes tables alphabetically, so the
#     `influenced_by` edges come before the `user` records they point at, and
#     surrealdb refuses an edge whose vertices don't exist yet. One bad reference
#     fails the whole batched statement, so a single pass restores every user and
#     activity and silently loses the entire influence graph. The second pass,
#     with the users already in place, puts the edges back.
#  2. HTTP 200 does not mean the import worked. The failures come back inside the
#     body, one result per statement, so both passes are checked for real errors.
#  3. The endpoint needs a Content-Type, or it answers 415.
#
# Usage:
#   scripts/restore.sh <dump-file>            # .surql or .surql.gz
#
# Configuration, read from the environment, or from .env when it's there:
#   SURREAL_URL or SURREAL_HTTP_URL   where to restore to
#   SURREAL_USER, SURREAL_PASS        root credentials
#   SURREAL_NAMESPACE, SURREAL_DATABASE   default to prod / prod
#   RESTORE_FORCE=true                allow restoring into a database that
#                                     already holds records
set -euo pipefail

DUMP_FILE=${1:-}
if [ -z "$DUMP_FILE" ]; then
    echo "usage: $0 <dump-file(.gz)>" >&2
    exit 2
fi
if [ ! -f "$DUMP_FILE" ]; then
    echo "no such dump: $DUMP_FILE" >&2
    exit 2
fi

for tool in curl jq gzip; do
    command -v "$tool" >/dev/null || { echo "$tool is required" >&2; exit 2; }
done

# Same .env the rest of the project uses, without overriding anything already set
if [ -f .env ]; then
    set -a
    # shellcheck disable=SC1091
    . ./.env
    set +a
fi

: "${SURREAL_USER:?set SURREAL_USER}"
: "${SURREAL_PASS:?set SURREAL_PASS}"
NAMESPACE=${SURREAL_NAMESPACE:-prod}
DATABASE=${SURREAL_DATABASE:-prod}

# The app is configured with a websocket url, the import endpoint is http on the
# same host
if [ -z "${SURREAL_HTTP_URL:-}" ]; then
    : "${SURREAL_URL:?set SURREAL_HTTP_URL or SURREAL_URL}"
    SURREAL_HTTP_URL=$(printf '%s' "$SURREAL_URL" | sed -e 's#/rpc$##' -e 's#^ws://#http://#' -e 's#^wss://#https://#')
fi

WORK_DIR=$(mktemp -d)
trap 'rm -rf "$WORK_DIR"' EXIT

surreal_sql() {
    curl -sS --max-time 120 -X POST "$SURREAL_HTTP_URL/sql" \
        -u "$SURREAL_USER:$SURREAL_PASS" \
        -H "surreal-ns: $NAMESPACE" -H "surreal-db: $DATABASE" \
        -H "Accept: application/json" \
        --data "$1"
}

count_of() {
    surreal_sql "SELECT count() FROM $1 GROUP ALL;" | jq -r '.[0].result[0].count // 0'
}

# gzipped dumps are what lands in the bucket
if [ "${DUMP_FILE##*.}" = "gz" ]; then
    echo "unpacking $DUMP_FILE"
    gzip -dc "$DUMP_FILE" > "$WORK_DIR/dump.surql"
    DUMP="$WORK_DIR/dump.surql"
else
    DUMP="$DUMP_FILE"
fi

echo "restoring $(du -h "$DUMP" | cut -f1) into $NAMESPACE/$DATABASE at $SURREAL_HTTP_URL"

# A dump merges into whatever is already there, it doesn't replace it. Restoring
# on top of live data is how you end up with two half databases.
EXISTING_USERS=$(count_of user)
if [ "$EXISTING_USERS" != "0" ] && [ "${RESTORE_FORCE:-}" != "true" ]; then
    echo "refusing to restore: $NAMESPACE/$DATABASE already holds $EXISTING_USERS users." >&2
    echo "restore into an empty namespace and swap over, or set RESTORE_FORCE=true." >&2
    exit 1
fi

for pass in 1 2; do
    echo "import pass $pass ..."
    STATUS=$(curl -sS --max-time 1800 -X POST "$SURREAL_HTTP_URL/import" \
        -u "$SURREAL_USER:$SURREAL_PASS" \
        -H "surreal-ns: $NAMESPACE" -H "surreal-db: $DATABASE" \
        -H "Accept: application/json" \
        -H "Content-Type: text/plain" \
        --data-binary "@$DUMP" \
        -o "$WORK_DIR/pass$pass.json" -w '%{http_code}')

    if [ "$STATUS" != "200" ]; then
        echo "import pass $pass was refused with HTTP $STATUS" >&2
        head -c 400 "$WORK_DIR/pass$pass.json" >&2
        exit 1
    fi

    TOTAL=$(jq 'length' "$WORK_DIR/pass$pass.json")
    FAILED=$(jq '[.[] | select(.status != "OK")] | length' "$WORK_DIR/pass$pass.json")
    echo "  $((TOTAL - FAILED))/$TOTAL statements applied"
done

# The second pass re-runs every schema definition, so "already exists" is
# expected there and only anything else counts as a failure
jq -r '.[] | select(.status != "OK") | .result' "$WORK_DIR/pass2.json" \
    | grep -v "already exists" > "$WORK_DIR/real-errors.txt" || true

if [ -s "$WORK_DIR/real-errors.txt" ]; then
    echo
    echo "the restore reported errors that are not schema redefinitions:" >&2
    sort "$WORK_DIR/real-errors.txt" | uniq -c | sort -rn | head -20 >&2
    exit 1
fi

echo
echo "restored:"
for table in user influenced_by activity; do
    printf '  %-16s %s\n' "$table" "$(count_of "$table")"
done

# The failure mode worth shouting about: the users come back, the graph doesn't
if grep -q "id: influenced_by:" "$DUMP" && [ "$(count_of influenced_by)" = "0" ]; then
    echo
    echo "the dump holds influence edges but none were restored." >&2
    echo "that is the ordering problem the second pass is meant to fix." >&2
    exit 1
fi

echo
echo "note: dumps carry no script_migration rows, so surrealdb-migrations will"
echo "consider this database unmigrated even though the schema is in place."
