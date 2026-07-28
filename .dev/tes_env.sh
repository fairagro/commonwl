#!/usr/bin/env bash
# Spins up a local GA4GH TES environment for iterating on the TES backend:
# rustfs (S3-compatible storage) + Funnel (TES server), mirroring the `tes` matrix leg of
# .github/workflows/conformance.yaml and .github/funnel-config.yaml as closely as possible.
#
# Usage:
#   .dev/tes_env.sh start     # start rustfs + funnel, wait until both are healthy
#   .dev/tes_env.sh stop      # tear both down
#   .dev/tes_env.sh status    # check whether they're up and reachable
#   .dev/tes_env.sh env       # print the shell exports needed to point tools at this env
#   .dev/tes_env.sh watchdog  # (run in background, alongside a conformance run) poll
#                             # Funnel's health and restart it if it goes down - works
#                             # around Funnel's own upstream crash under concurrent S3
#                             # downloads (a Go panic in its debug-log formatter) so a
#                             # mid-suite crash only loses the in-flight tests instead of
#                             # every test after it
#
# After 'start', run the TES conformance suite against it with:
#   eval "$(.dev/tes_env.sh env)"
#   BACKEND=tes cargo test -p cwl_engine --test conformance test_conformance_tes -- --nocapture
# or with the conformance CLI + cwltest, same as CI:
#   eval "$(.dev/tes_env.sh env)"
#   cargo build --release -p conformance
#   BACKEND=tes cwltest --test testdata/cwl/conformance_tests.yaml --tool target/release/conformance

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
STATE_DIR="${TES_ENV_DIR:-$SCRIPT_DIR/.tes-env}"
FUNNEL_BIN="$STATE_DIR/funnel"
FUNNEL_PID_FILE="$STATE_DIR/funnel.pid"
RUSTFS_CONTAINER=commonwl-rustfs-dev

S3_ENDPOINT="http://localhost:9000"
S3_BUCKET="commonwl-bucket"
S3_ACCESS_KEY="rustfsadmin"
S3_SECRET_KEY="rustfsadmin"
S3_REGION="us-east-1"
TES_ENDPOINT="http://localhost:8000"

usage() {
    sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'
}

wait_for() {
    # wait_for <description> <curl-args...>
    local desc="$1"
    shift
    for _ in $(seq 1 30); do
        if curl -sf "$@" >/dev/null 2>&1; then
            return 0
        fi
        sleep 1
    done
    echo "error: $desc did not become reachable in time" >&2
    return 1
}

start_rustfs() {
    if docker inspect "$RUSTFS_CONTAINER" >/dev/null 2>&1; then
        echo "rustfs: container '$RUSTFS_CONTAINER' already exists, reusing"
        docker start "$RUSTFS_CONTAINER" >/dev/null 2>&1 || true
    else
        echo "rustfs: starting new container '$RUSTFS_CONTAINER'"
        mkdir -p "$STATE_DIR/data" "$STATE_DIR/logs"
        # rustfs runs as uid/gid 10001 in the container; the mounted host dirs need to be
        # owned by that uid or it fails to initialize (matches the CI job's `sudo chown` step).
        sudo chown -R 10001:10001 "$STATE_DIR/data" "$STATE_DIR/logs" 2>/dev/null \
            || echo "warning: could not chown $STATE_DIR/{data,logs} to 10001:10001 (no sudo?); rustfs may fail to start" >&2
        docker run -d --name "$RUSTFS_CONTAINER" \
            -p 9000:9000 -p 9001:9001 \
            -e RUSTFS_ALLOW_INSECURE_DEFAULT_CREDENTIALS=true \
            -v "$STATE_DIR/data:/data" \
            -v "$STATE_DIR/logs:/logs" \
            rustfs/rustfs:latest >/dev/null
    fi

    echo "rustfs: waiting for health check..."
    wait_for "rustfs" "$S3_ENDPOINT/health"
    echo "rustfs: up at $S3_ENDPOINT"
}

make_bucket() {
    echo "rustfs: ensuring bucket s3://$S3_BUCKET exists..."
    if command -v aws >/dev/null 2>&1; then
        AWS_ACCESS_KEY_ID="$S3_ACCESS_KEY" AWS_SECRET_ACCESS_KEY="$S3_SECRET_KEY" \
            aws s3 mb "s3://$S3_BUCKET" --endpoint-url "$S3_ENDPOINT" --region "$S3_REGION" \
            >/dev/null 2>&1 || true
    elif python3 -c "import boto3" >/dev/null 2>&1; then
        python3 - "$S3_ENDPOINT" "$S3_BUCKET" "$S3_REGION" "$S3_ACCESS_KEY" "$S3_SECRET_KEY" <<'PYEOF'
import sys
import boto3
from botocore.exceptions import ClientError

endpoint, bucket, region, key, secret = sys.argv[1:6]
client = boto3.client(
    "s3",
    endpoint_url=endpoint,
    region_name=region,
    aws_access_key_id=key,
    aws_secret_access_key=secret,
)
try:
    client.create_bucket(Bucket=bucket)
except ClientError as e:
    code = e.response.get("Error", {}).get("Code", "")
    if code not in ("BucketAlreadyOwnedByYou", "BucketAlreadyExists"):
        raise
PYEOF
    else
        echo "error: need either the 'aws' CLI or python3+boto3 to create the S3 bucket" >&2
        echo "       install one of: 'pip install boto3', or the AWS CLI" >&2
        exit 1
    fi
    echo "rustfs: bucket s3://$S3_BUCKET ready"
}

start_funnel() {
    if [ ! -x "$FUNNEL_BIN" ]; then
        echo "funnel: no local binary, downloading..."
        mkdir -p "$STATE_DIR"
        local tag
        # Mirrors CI's `curl .../releases/latest | jq .tag_name`, but falls back to resolving
        # the redirect target of /releases/latest in case the REST API host is unreachable
        # (rate-limited/blocked) - the redirect works even where the API endpoint doesn't.
        tag=$(curl -sS -m 10 "https://api.github.com/repos/calypr/funnel/releases/latest" 2>/dev/null \
            | jq -r '.tag_name // empty' 2>/dev/null || true)
        if [ -z "$tag" ]; then
            tag=$(curl -sSL -m 10 -o /dev/null -w '%{url_effective}' \
                "https://github.com/calypr/funnel/releases/latest" | sed 's#.*/tag/##')
        fi
        if [ -z "$tag" ]; then
            echo "error: could not resolve the latest Funnel release tag" >&2
            exit 1
        fi
        echo "funnel: latest release is $tag"
        local asset="${FUNNEL_ASSET:-funnel-linux-amd64-${tag}.tar.gz}"
        curl -sSL "https://github.com/calypr/funnel/releases/download/${tag}/${asset}" \
            | tar -xz -C "$STATE_DIR" funnel
        chmod +x "$FUNNEL_BIN"
    fi

    if [ -f "$FUNNEL_PID_FILE" ] && kill -0 "$(cat "$FUNNEL_PID_FILE")" 2>/dev/null; then
        echo "funnel: already running (pid $(cat "$FUNNEL_PID_FILE"))"
    else
        echo "funnel: starting server..."
        "$FUNNEL_BIN" server run --config "$REPO_ROOT/.github/funnel-config.yaml" \
            >"$STATE_DIR/funnel.log" 2>&1 &
        disown
        echo $! >"$FUNNEL_PID_FILE"
    fi

    echo "funnel: waiting for health check..."
    wait_for "funnel" "$TES_ENDPOINT/v1/tasks"
    echo "funnel: up at $TES_ENDPOINT"
}

start() {
    start_rustfs
    make_bucket
    start_funnel
    echo
    echo "TES dev environment is up."
    print_env
}

stop() {
    if [ -f "$FUNNEL_PID_FILE" ]; then
        kill "$(cat "$FUNNEL_PID_FILE")" 2>/dev/null || true
        rm -f "$FUNNEL_PID_FILE"
        echo "funnel: stopped"
    fi
    if docker inspect "$RUSTFS_CONTAINER" >/dev/null 2>&1; then
        docker rm -f "$RUSTFS_CONTAINER" >/dev/null
        echo "rustfs: stopped and removed"
    fi
}

status() {
    if docker inspect -f '{{.State.Status}}' "$RUSTFS_CONTAINER" >/dev/null 2>&1; then
        echo "rustfs: container is $(docker inspect -f '{{.State.Status}}' "$RUSTFS_CONTAINER")"
    else
        echo "rustfs: not running"
    fi
    if curl -sf "$S3_ENDPOINT/health" >/dev/null 2>&1; then
        echo "rustfs: reachable at $S3_ENDPOINT"
    else
        echo "rustfs: not reachable at $S3_ENDPOINT"
    fi
    if [ -f "$FUNNEL_PID_FILE" ] && kill -0 "$(cat "$FUNNEL_PID_FILE")" 2>/dev/null; then
        echo "funnel: process running (pid $(cat "$FUNNEL_PID_FILE"))"
    else
        echo "funnel: no tracked process"
    fi
    if curl -sf "$TES_ENDPOINT/v1/tasks" >/dev/null 2>&1; then
        echo "funnel: reachable at $TES_ENDPOINT"
    else
        echo "funnel: not reachable at $TES_ENDPOINT"
    fi
}

watchdog() {
    local interval="${TES_WATCHDOG_INTERVAL:-5}"
    echo "funnel: watchdog started (checking every ${interval}s)"
    while true; do
        if ! curl -sf "$TES_ENDPOINT/v1/tasks" >/dev/null 2>&1; then
            echo "funnel: watchdog detected server is down, restarting..."
            start_funnel || echo "funnel: restart attempt failed, will retry in ${interval}s"
        fi
        sleep "$interval"
    done
}

print_env() {
    cat <<EOF
export AWS_ACCESS_KEY_ID=$S3_ACCESS_KEY
export AWS_SECRET_ACCESS_KEY=$S3_SECRET_KEY
export AWS_REGION=$S3_REGION
export S3_ENDPOINT_URL=$S3_ENDPOINT
export BACKEND=tes
EOF
}

case "${1:-}" in
    start) start ;;
    stop) stop ;;
    status) status ;;
    env) print_env ;;
    watchdog) watchdog ;;
    *)
        usage
        exit 1
        ;;
esac
