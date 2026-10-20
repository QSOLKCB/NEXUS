#!/usr/bin/env sh
# Independent multicore verification harness for a frozen NEXUS commit.
# See protocol/QBRAID.md. POSIX sh; needs git, cargo, python3, sha256sum.
#
# Usage:
#   REPO_URL=<git url> TEST_COMMIT=<sha> sh scripts/verify_harness.sh
#
# Everything is written beneath a new directory in the current working
# directory. The tested checkout is never modified; any generated file inside
# it invalidates the run.

set -eu

: "${REPO_URL:?set REPO_URL to the repository to test}"
: "${TEST_COMMIT:?set TEST_COMMIT to the frozen commit}"
RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)"
WORK_ROOT="$PWD/nexus-verify-$RUN_ID"
CONTROL_DIR="$WORK_ROOT/control"
REPO_DIR="$WORK_ROOT/tested"
SHORT="$(printf '%s' "$TEST_COMMIT" | cut -c1-8)"
RESULTS="$WORK_ROOT/nexus-verify-results-$SHORT"
TARGET="$WORK_ROOT/cargo-target"
export LC_ALL=C GIT_OPTIONAL_LOCKS=0

mkdir -p "$RESULTS/00-provenance/raw" "$RESULTS/01-build/primary" \
  "$RESULTS/02-correctness" "$RESULTS/03-parallel/thread-observation" \
  "$RESULTS/04-performance/raw" "$RESULTS/05-determinism" \
  "$RESULTS/06-memory/raw" "$RESULTS/analysis" "$TARGET"

git clone --depth 1 "$REPO_URL" "$CONTROL_DIR"
git -C "$CONTROL_DIR" rev-parse HEAD > "$RESULTS/00-provenance/control-commit.txt"
cp "$CONTROL_DIR/protocol/QBRAID.md" "$RESULTS/PROTOCOL.md"
cp "$CONTROL_DIR/scripts/summarize.py" "$CONTROL_DIR/scripts/provenance.py" "$RESULTS/analysis/"
cp "$0" "$RESULTS/analysis/verify_harness.sh"

git clone --no-checkout "$REPO_URL" "$REPO_DIR"
git -C "$REPO_DIR" checkout --detach "$TEST_COMMIT"
git -C "$REPO_DIR" rev-parse HEAD > "$RESULTS/00-provenance/tested-source-commit.txt"
[ "$(cat "$RESULTS/00-provenance/tested-source-commit.txt")" = "$TEST_COMMIT" ] || {
  echo "checkout does not match TEST_COMMIT" > "$RESULTS/00-provenance/INVALID-run.txt"; exit 97; }

invalidate() {
  printf '%s\n' "$1" > "$RESULTS/00-provenance/INVALID-run.txt"
  git -C "$REPO_DIR" status --porcelain=v1 --untracked-files=all --ignored=matching \
    > "$RESULTS/00-provenance/INVALID-status.txt" || true
  echo "INVALID RUN: $1" >&2
  exit 97
}
clean_tree() {
  STATUS="$(git -C "$REPO_DIR" status --porcelain=v1 --untracked-files=all --ignored=matching)"
  [ -z "$STATUS" ] || invalidate "tested checkout is not clean"
  [ ! -e "$REPO_DIR/target" ] || invalidate "in-tree target directory exists"
}
clean_tree
( cd "$REPO_DIR" && git ls-files -z | sort -z | xargs -0 sha256sum ) \
  > "$RESULTS/00-provenance/source-before.sha256"

# --- raw provenance -------------------------------------------------------
P="$RESULTS/00-provenance/raw"
git -C "$REPO_DIR" show -s --format=fuller HEAD > "$P/tested-commit-show.txt"
git -C "$CONTROL_DIR" show -s --format=fuller HEAD > "$P/control-commit-show.txt"
cp "$REPO_DIR/Cargo.toml" "$P/Cargo.toml.txt"
command -v rustc > "$P/rustc-path.txt"; command -v cargo > "$P/cargo-path.txt"
rustc -Vv > "$P/rustc.txt" 2>&1; cargo -Vv > "$P/cargo.txt" 2>&1
rustup show active-toolchain > "$P/rustup-active-toolchain.txt" 2>&1 || true
( cd "$REPO_DIR" && env CARGO_TARGET_DIR="$TARGET" cargo metadata --locked --no-deps --format-version 1 ) \
  > "$P/cargo-metadata.json" 2> "$P/cargo-metadata.stderr"
clean_tree
{
  printf 'timestamp_utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo '--- uname ---'; uname -a
  echo '--- os-release ---'; cat /etc/os-release 2>/dev/null || true
  echo '--- processors ---'; nproc || true; nproc --all || true
  echo '--- lscpu ---'; lscpu || true
  echo '--- affinity ---'; grep -E 'Cpus_allowed_list' /proc/self/status || true
  echo '--- cgroup ---'; cat /sys/fs/cgroup/cpu.max 2>/dev/null || true
  cat /sys/fs/cgroup/cpuset.cpus.effective 2>/dev/null || true
  cat /sys/fs/cgroup/memory.max 2>/dev/null || true
  echo '--- memory ---'; free -h || true
  echo '--- time ---'; /usr/bin/time --version 2>&1 || echo 'usr-bin-time: absent'
} > "$P/environment.txt" 2>&1
python3 "$RESULTS/analysis/provenance.py" --cpu-capacity-evidence > "$P/cpu-capacity.json"
EFFECTIVE_CPUS="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["effective_cpus"])' "$P/cpu-capacity.json")"
printf '%s\n' "$EFFECTIVE_CPUS" > "$P/effective-cpus.txt"
# Preserve fractional quotas; even a sub-core environment still runs W=1.
MAX_CPU_WORKERS="$(python3 -c 'import sys; print(max(1, int(float(sys.argv[1]))))' "$EFFECTIVE_CPUS")"

# --- build ----------------------------------------------------------------
required() {
  NAME="$1"; shift
  set +e
  ( cd "$REPO_DIR" && env CARGO_TARGET_DIR="$TARGET" "$@" ) \
    > "$RESULTS/01-build/primary/$NAME.log" 2>&1
  S=$?; set -e
  printf '%s\n' "$S" > "$RESULTS/01-build/primary/$NAME.exit-status"
  clean_tree
  [ "$S" -eq 0 ] || { echo "required step failed: $NAME" >&2; exit "$S"; }
}
required format-check cargo fmt --all -- --check
required clippy cargo clippy --locked --all-targets -- -D warnings
required build-release cargo build --release --locked
required tests cargo test --locked
NEXUS="$TARGET/release/nexus"
sha256sum "$NEXUS" > "$RESULTS/01-build/primary/binary.sha256"

# --- experiment runner ----------------------------------------------------
run() {  # run <outdir> <name> <args...>
  OUT="$1"; NAME="$2"; shift 2
  set +e
  "$NEXUS" "$@" > "$OUT/$NAME.csv" 2> "$OUT/$NAME.stderr"
  S=$?; set -e
  printf '%s\n' "$S" > "$OUT/$NAME.exit-status"
}
timed() {  # timed <outdir> <name> <args...>   (memory via /usr/bin/time -v, else /proc VmHWM sampling)
  OUT="$1"; NAME="$2"; shift 2
  if [ -x /usr/bin/time ]; then
    set +e
    /usr/bin/time -v -o "$RESULTS/06-memory/raw/$NAME.time.txt" "$NEXUS" "$@" \
      > "$OUT/$NAME.csv" 2> "$OUT/$NAME.stderr"
    S=$?; set -e
  else
    "$NEXUS" "$@" > "$OUT/$NAME.csv" 2> "$OUT/$NAME.stderr" &
    CHILD=$!
    HWM=0
    while kill -0 "$CHILD" 2>/dev/null; do
      V="$(awk '/^VmHWM:/ {print $2}' "/proc/$CHILD/status" 2>/dev/null || true)"
      [ -n "$V" ] && [ "$V" -gt "$HWM" ] && HWM="$V"
      sleep 0.01
    done
    set +e; wait "$CHILD"; S=$?; set -e
    printf 'method: /proc/<pid>/status VmHWM sampled every 10 ms (usr-bin-time absent)\nMaximum resident set size (kbytes): %s\n' "$HWM" \
      > "$RESULTS/06-memory/raw/$NAME.time.txt"
  fi
  printf '%s\n' "$S" > "$OUT/$NAME.exit-status"
}

C="$RESULTS/02-correctness"
run "$C" audit-default audit
for L in 65536 1048576 16777216 4294967296; do run "$C" "audit-L$L" audit --cardinality "$L"; done
run "$C" audit-nondivisible audit --cardinality 1000003 --strands 777
run "$C" audit-golden audit --ordering golden
run "$C" trace-17 trace 17 10000
run "$C" trace-511-golden trace 511 10000 --ordering golden
run "$C" events-17 events 17 1000
run "$C" lattice-verify-1024 lattice-verify 1024 --rounds 16
run "$C" lattice-verify-4096 lattice-verify 4096 --rounds 64
run "$C" atlas-1m atlas 1000000 --rounds 16

WORKERS=""
for W in 1 2 4 8 16; do [ "$W" -le "$MAX_CPU_WORKERS" ] && WORKERS="$WORKERS $W"; done
printf '%s\n' "$WORKERS" > "$RESULTS/03-parallel/worker-matrix.txt"
for W in $WORKERS; do run "$RESULTS/03-parallel" "audit-W$W" audit --workers "$W"; done

D="$RESULTS/05-determinism"
for i in 1 2 3; do
  run "$D" "ledger-uniform-$i" ledger 100000
  run "$D" "ledger-golden-$i" ledger 100000 --ordering golden
done

PF="$RESULTS/04-performance/raw"
for i in 1 2 3 4 5; do
  timed "$PF" "bench-$i" bench 1000
  timed "$PF" "bench-4096-$i" bench 1000 --strands 4096
  timed "$PF" "lattice-run-$i" lattice-run 65536 128
  for W in $WORKERS; do timed "$PF" "bench-exec-W$W-$i" bench-exec 2000 262144 "$W"; done
done

# thread observation at the largest permitted worker count on a long audit
MAXW="$(printf '%s\n' $WORKERS | tail -n 1)"
"$NEXUS" audit --workers "$MAXW" --cardinality 4294967296 --strands 16777216 \
  > "$RESULTS/03-parallel/thread-observation/audit-W$MAXW.csv" 2>&1 &
PID=$!
MAX_T=0
while kill -0 "$PID" 2>/dev/null; do
  T="$(ls /proc/$PID/task 2>/dev/null | wc -l)"
  [ "$T" -gt "$MAX_T" ] && MAX_T="$T"
  sleep 0.02
done
set +e
wait "$PID"
S=$?
set -e
printf '%s\n' "$S" > "$RESULTS/03-parallel/thread-observation/audit-W$MAXW.exit-status"
printf 'command=audit --workers %s --cardinality 4294967296 --strands 16777216\nrequested_workers=%s\nmax_observed_os_threads=%s\nnote=observed threads include the main thread; sampling every 20 ms can miss short-lived workers\n' "$MAXW" "$MAXW" "$MAX_T" \
  > "$RESULTS/03-parallel/thread-observation/summary.txt"
clean_tree

( cd "$REPO_DIR" && git ls-files -z | sort -z | xargs -0 sha256sum ) \
  > "$RESULTS/00-provenance/source-after.sha256"
cmp "$RESULTS/00-provenance/source-before.sha256" "$RESULTS/00-provenance/source-after.sha256" \
  || invalidate "source manifest changed"

# --- analysis -------------------------------------------------------------
python3 "$RESULTS/analysis/provenance.py" "$RESULTS" > "$RESULTS/00-provenance/VERSION_PROVENANCE.json"
python3 "$RESULTS/analysis/summarize.py" "$RESULTS" > "$RESULTS/REPORT.md"
( cd "$RESULTS" && find . -type f ! -name SHA256SUMS.txt -print0 | sort -z | xargs -0 sha256sum ) \
  > "$RESULTS/SHA256SUMS.txt"
( cd "$WORK_ROOT" && zip -qr "nexus-verify-results-$SHORT.zip" "$(basename "$RESULTS")" \
  && sha256sum "nexus-verify-results-$SHORT.zip" > "nexus-verify-results-$SHORT.zip.sha256" )
echo "bundle: $WORK_ROOT/nexus-verify-results-$SHORT.zip"
