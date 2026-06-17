#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BULB="/home/sehaxe/bulb/target/release/bulb"
DATA_DIR="$SCRIPT_DIR/data"
RESULTS_DIR="$SCRIPT_DIR/results"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
RESULT_FILE="$RESULTS_DIR/bench_${TIMESTAMP}.md"

mkdir -p "$RESULTS_DIR"

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; CYAN='\033[0;36m'; NC='\033[0m'

bench_count=0

header() {
    echo ""
    echo -e "${CYAN}══════════════════════════════════════════════════════════════${NC}"
    echo -e "${CYAN}  $1${NC}"
    echo -e "${CYAN}══════════════════════════════════════════════════════════════${NC}"
    echo ""
    echo "## $1" >> "$RESULT_FILE"
    echo "" >> "$RESULT_FILE"
    echo "| Method | Min | Avg | Max | Notes |" >> "$RESULT_FILE"
    echo "| --- | --- | --- | --- | --- |" >> "$RESULT_FILE"
}

result() {
    local label="$1" value="$2" unit="$3" notes="${4:-}"
    bench_count=$((bench_count + 1))
    printf "  %-40s %8s %s" "$label" "$value" "$unit"
    [ -n "$notes" ] && printf "  ${YELLOW}(%s)${NC}" "$notes"
    printf "\n"
    echo "| $label | $value $unit | ${notes:-} |" >> "$RESULT_FILE"
}

time_ms() {
    local runs=$1; shift
    local times=()
    for ((i=0; i<runs; i++)); do
        local t0 t1
        t0=$(date +%s%N)
        "$@" >/dev/null 2>&1
        t1=$(date +%s%N)
        local ms=$(( (t1 - t0) / 1000000 ))
        times+=($ms)
    done
    local min=999999 avg=0 max=0
    for t in "${times[@]}"; do
        (( t < min )) && min=$t
        (( t > max )) && max=$t
        avg=$((avg + t))
    done
    avg=$((avg / runs))
    echo "$min $avg $max"
}

echo "# bulb benchmarks — $(date)" > "$RESULT_FILE"
echo "" >> "$RESULT_FILE"

# ══════════════════════════════════════════════════════════════════
# BENCH 1: Decompression speed
# ══════════════════════════════════════════════════════════════════
header "1. Decompression Speed"

for pkg in small.pkg.tar.bz3 medium.pkg.tar.bz3 large.pkg.tar.bz3; do
    [ -f "$DATA_DIR/$pkg" ] || continue
    pkgname="${pkg%.pkg.tar.bz3}"
    size_kb=$(( $(stat -c%s "$DATA_DIR/$pkg") / 1024 ))
    TMPROOT=$(mktemp -d)
    read -r mn av mx <<< $(time_ms 5 $BULB --root "$TMPROOT" --db-path "$TMPROOT/b.db" --store-path "$TMPROOT/store" install "$DATA_DIR/$pkg")
    result "bz3 parallel install ($pkgname, ${size_kb}KB)" "$av" "ms" "min=$mn max=$mx"
    rm -rf "$TMPROOT"
done

for pkg in large.pkg.tar.zst; do
    [ -f "$DATA_DIR/$pkg" ] || continue
    size_kb=$(( $(stat -c%s "$DATA_DIR/$pkg") / 1024 ))
    TMPROOT=$(mktemp -d)
    read -r mn av mx <<< $(time_ms 5 $BULB --root "$TMPROOT" --db-path "$TMPROOT/b.db" --store-path "$TMPROOT/store" install "$DATA_DIR/$pkg")
    result "zstd-19 install (large, ${size_kb}KB)" "$av" "ms" "min=$mn max=$mx"
    rm -rf "$TMPROOT"
done

echo "" >> "$RESULT_FILE"

# ══════════════════════════════════════════════════════════════════
# BENCH 2: Pure decompression (no install overhead)
# ══════════════════════════════════════════════════════════════════
header "2. Pure Decompression (no DB/store overhead)"

for pkg in large.pkg.tar.bz3; do
    TMPF=$(mktemp)
    read -r mn av mx <<< $(time_ms 10 $BULB bench-decompress "$DATA_DIR/$pkg" -o "$TMPF")
    result "bz3 parallel decompress" "$av" "ms" "min=$mn max=$mx"
    rm -f "$TMPF"
done

for pkg in large.pkg.tar.zst; do
    TMPF=$(mktemp)
    read -r mn av mx <<< $(time_ms 10 $BULB bench-decompress "$DATA_DIR/$pkg" -o "$TMPF")
    result "zstd-19 decompress" "$av" "ms" "min=$mn max=$mx"
    rm -f "$TMPF"
done

echo "" >> "$RESULT_FILE"

# ══════════════════════════════════════════════════════════════════
# BENCH 3: pacman vs bulb
# ══════════════════════════════════════════════════════════════════
header "3. pacman vs bulb Install"

if command -v pacman &>/dev/null; then
    for pkg in large.pkg.tar.zst; do
        [ -f "$DATA_DIR/$pkg" ] || continue
        TMPROOT=$(mktemp -d)
        mkdir -p "$TMPROOT/var/lib/pacman"
        read -r mn av mx <<< $(time_ms 3 sudo pacman -U "$DATA_DIR/$pkg" --root "$TMPROOT" --dbpath "$TMPROOT/var/lib/pacman" --noconfirm 2>/dev/null)
        result "pacman -U zstd" "$av" "ms" "min=$mn max=$mx"
        rm -rf "$TMPROOT"
    done
    for pkg in large.pkg.tar.bz3; do
        [ -f "$DATA_DIR/$pkg" ] || continue
        TMPROOT=$(mktemp -d)
        mkdir -p "$TMPROOT/var/lib/pacman"
        read -r mn av mx <<< $(time_ms 3 sudo pacman -U "$DATA_DIR/$pkg" --root "$TMPROOT" --dbpath "$TMPROOT/var/lib/pacman" --noconfirm 2>/dev/null)
        result "pacman -U bz3" "$av" "ms" "min=$mn max=$mx"
        rm -rf "$TMPROOT"
    done
else
    echo "  (skipped: pacman not available or no sudo access)"
    echo "| (skipped) | - | - | - | pacman not available |" >> "$RESULT_FILE"
fi

for pkg in large.pkg.tar.zst; do
    TMPROOT=$(mktemp -d)
    read -r mn av mx <<< $(time_ms 5 $BULB --root "$TMPROOT" --db-path "$TMPROOT/b.db" --store-path "$TMPROOT/store" install "$DATA_DIR/$pkg")
    result "bulb install zstd" "$av" "ms" "min=$mn max=$mx"
    rm -rf "$TMPROOT"
done

for pkg in large.pkg.tar.bz3; do
    TMPROOT=$(mktemp -d)
    read -r mn av mx <<< $(time_ms 5 $BULB --root "$TMPROOT" --db-path "$TMPROOT/b.db" --store-path "$TMPROOT/store" install "$DATA_DIR/$pkg")
    result "bulb install bz3" "$av" "ms" "min=$mn max=$mx"
    rm -rf "$TMPROOT"
done

echo "" >> "$RESULT_FILE"

# ══════════════════════════════════════════════════════════════════
# BENCH 4: Query speed
# ══════════════════════════════════════════════════════════════════
header "4. Query Speed"

read -r mn av mx <<< $(time_ms 20 $BULB query 2>/dev/null)
result "bulb query (all packages)" "$av" "ms" "min=$mn max=$mx"

read -r mn av mx <<< $(time_ms 20 $BULB query bash 2>/dev/null)
result "bulb query bash" "$av" "ms" "min=$mn max=$mx"

if command -v pacman &>/dev/null; then
    read -r mn av mx <<< $(time_ms 20 pacman -Q 2>/dev/null)
    result "pacman -Q" "$av" "ms" "min=$mn max=$mx"

    read -r mn av mx <<< $(time_ms 20 pacman -Qi bash 2>/dev/null)
    result "pacman -Qi bash" "$av" "ms" "min=$mn max=$mx"

    read -r mn av mx <<< $(time_ms 20 pacman -Ql bash 2>/dev/null)
    result "pacman -Ql bash" "$av" "ms" "min=$mn max=$mx"
fi

echo "" >> "$RESULT_FILE"

# ══════════════════════════════════════════════════════════════════
# BENCH 5: Build speed
# ══════════════════════════════════════════════════════════════════
header "5. Build Speed"

TMPSRC=$(mktemp -d)
mkdir -p "$TMPSRC/bld/usr/bin"
echo '#!/bin/sh
echo hello' > "$TMPSRC/bld/usr/bin/hello"
cat > "$TMPSRC/bld/Bulb.toml" << 'EOF'
[package]
name = "bench-build"
version = "1.0"
release = "1"
arch = "x86_64"
desc = "Benchmark build"
packager = "bench"
EOF

TMPF=$(mktemp)
read -r mn av mx <<< $(time_ms 10 $BULB build "$TMPSRC/bld" -o "$TMPF")
result "bulb build (tiny pkg)" "$av" "ms" "min=$mn max=$mx"
rm -rf "$TMPSRC" "$TMPF"

echo "" >> "$RESULT_FILE"

# ══════════════════════════════════════════════════════════════════
# BENCH 6: Content store deduplication
# ══════════════════════════════════════════════════════════════════
header "6. Content Store (BLAKE3 dedup)"

TMPROOT=$(mktemp -d)
$BULB --root "$TMPROOT/r1" --db-path "$TMPROOT/b.db" --store-path "$TMPROOT/store" install "$DATA_DIR/large.pkg.tar.bz3" 2>/dev/null
INSTALLED1=$(du -sb "$TMPROOT/r1" | cut -f1)
STORE1=$(du -sb "$TMPROOT/store" | cut -f1)

$BULB --root "$TMPROOT/r2" --db-path "$TMPROOT/b.db" --store-path "$TMPROOT/store" install "$DATA_DIR/large.pkg.tar.bz3" 2>/dev/null
STORE2=$(du -sb "$TMPROOT/store" | cut -f1)
INSTALLED2=$(du -sb "$TMPROOT/r2" | cut -f1)

NO_DEDUP=$((INSTALLED1 * 2))
WITH_DEDUP=$STORE2
SAVED_PCT=$((100 - (WITH_DEDUP * 100 / NO_DEDUP)))

result "2x install without dedup" "$((NO_DEDUP / 1024))" "KB"
result "content store (with dedup)" "$((WITH_DEDUP / 1024))" "KB"
result "space saved" "$SAVED_PCT" "%" ""
rm -rf "$TMPROOT"

echo "" >> "$RESULT_FILE"

# ══════════════════════════════════════════════════════════════════
# BENCH 7: Sync DB parsing
# ══════════════════════════════════════════════════════════════════
header "7. Sync DB Parsing"

for db in /var/lib/pacman/sync/*.db; do
    [ -f "$db" ] || continue
    dbname=$(basename "$db")
    dsize_kb=$(( $(stat -c%s "$db") / 1024 ))
    TMPDB=$(mktemp -d)
    cp "$db" "$TMPDB/$dbname"
    read -r mn av mx <<< $(time_ms 10 $BULB bench-sync-parse "$TMPDB/$dbname")
    result "$dbname (${dsize_kb}KB)" "$av" "ms" "min=$mn max=$mx"
    rm -rf "$TMPDB"
done

echo "" >> "$RESULT_FILE"

# ══════════════════════════════════════════════════════════════════
# BENCH 8: Version comparison throughput
# ══════════════════════════════════════════════════════════════════
header "8. Version Comparison"

read -r mn av mx <<< $(time_ms 5 $BULB bench-vercmp 2>/dev/null)
result "vercmp throughput" "$av" "ms" "min=$mn max=$mx"

echo "" >> "$RESULT_FILE"

# ══════════════════════════════════════════════════════════════════
# BENCH 9: Generation management
# ══════════════════════════════════════════════════════════════════
header "9. Generation Management"

TMPDB=$(mktemp -d)
$BULB --db-path "$TMPDB/g.db" migrate 2>/dev/null || true

read -r mn av mx <<< $(time_ms 20 $BULB --db-path "$TMPDB/g.db" list-generations 2>/dev/null)
result "list-generations" "$av" "ms" "min=$mn max=$mx"
rm -rf "$TMPDB"

echo "" >> "$RESULT_FILE"
echo "---" >> "$RESULT_FILE"
echo "*$(date) — $bench_count benchmarks completed*" >> "$RESULT_FILE"

echo ""
echo -e "${GREEN}══════════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}  Done: ${bench_count} benchmarks${NC}"
echo -e "${GREEN}  Results: ${RESULT_FILE}${NC}"
echo -e "${GREEN}══════════════════════════════════════════════════════════════${NC}"
