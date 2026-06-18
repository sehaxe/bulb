#!/usr/bin/env bash
set -euo pipefail

DATA_DIR="/home/sehaxe/bulb/benchmarks/data"
RESULTS_DIR="/home/sehaxe/bulb/benchmarks/results"
mkdir -p "$RESULTS_DIR"

TMPTAR=$(mktemp)
TMPCOMP=$(mktemp)
TMPDECOMP=$(mktemp)
trap 'rm -f "$TMPTAR" "$TMPCOMP" "$TMPDECOMP"' EXIT

bzip3 -d -c < "$DATA_DIR/large.pkg.tar.bz3" > "$TMPTAR"
ORIG=$(stat -c%s "$TMPTAR")
printf "Source: large tar (uncompressed) = %d KB\n" $((ORIG / 1024))
echo ""

fmt="%-12s %5s  %8s %6s  %8s %8s\n"
divider="─────────────────────────────────────────────────────────────────────────────"

run_one() {
    local name="$1" lvl="$2" comp="$3" decomp="$4"

    local t0 t1 t2 t3
    t0=$(date +%s%N)
    eval "$comp" < "$TMPTAR" > "$TMPCOMP" 2>/dev/null
    t1=$(date +%s%N)
    local cs=$(stat -c%s "$TMPCOMP")

    t2=$(date +%s%N)
    eval "$decomp" < "$TMPCOMP" > "$TMPDECOMP" 2>/dev/null
    t3=$(date +%s%N)

    local ds=$(stat -c%s "$TMPDECOMP")
    if [ "$ds" -ne "$ORIG" ]; then
        printf "  %-12s %s  CORRUPT (got %d want %d)\n" "$name" "$lvl" "$ds" "$ORIG"
        return
    fi

    local c_ms=$(( (t1 - t0) / 1000000 ))
    local d_ms=$(( (t3 - t2) / 1000000 ))
    local c_kb=$((cs / 1024))
    local ratio=$((cs * 100 / ORIG))
    local o_kb=$((ORIG / 1024))

    printf "$fmt" "$name" "$lvl" "${c_kb}KB" "${ratio}%" "${c_ms}ms" "${d_ms}ms"
}

echo ""
echo "╔═══════════════════════════════════════════════════════════════════════════════╗"
echo "║               COMPRESSION FORMAT BENCHMARK (raw tar input)                  ║"
echo "╚═══════════════════════════════════════════════════════════════════════════════╝"
echo ""

# ═══════════════════════════════════════════════════════════════
printf "┌─────────────────────────────────────────────────────────────────────────────┐\n"
printf "│  zstd — optimal speed/ratio (used by pacman)                               │\n"
printf "├────────────┬───────┬──────────┬───────┬──────────┬──────────┤\n"
printf "$fmt" "Format" "Level" "Size" "Ratio" "Compress" "Decompr"
printf "├────────────┼───────┼──────────┼───────┼──────────┼──────────┤\n"
for lvl in 1 3 5 10 15 19; do
    run_one "zstd" "$lvl" "zstd -c -${lvl}" "zstd -d -c"
done
printf "└────────────┴───────┴──────────┴───────┴──────────┴──────────┘\n"

echo ""

# ═══════════════════════════════════════════════════════════════
printf "┌─────────────────────────────────────────────────────────────────────────────┐\n"
printf "│  lz4 — fastest decompression                                               │\n"
printf "├────────────┬───────┬──────────┬───────┬──────────┬──────────┤\n"
printf "$fmt" "Format" "Level" "Size" "Ratio" "Compress" "Decompr"
printf "├────────────┼───────┼──────────┼───────┼──────────┼──────────┤\n"
for lvl in 1 9 12; do
    run_one "lz4" "$lvl" "lz4 -c -${lvl}" "lz4 -d -c"
done
printf "└────────────┴───────┴──────────┴───────┴──────────┴──────────┘\n"

echo ""

# ═══════════════════════════════════════════════════════════════
printf "┌─────────────────────────────────────────────────────────────────────────────┐\n"
printf "│  xz — best compression ratio                                               │\n"
printf "├────────────┬───────┬──────────┬───────┬──────────┬──────────┤\n"
printf "$fmt" "Format" "Level" "Size" "Ratio" "Compress" "Decompr"
printf "├────────────┼───────┼──────────┼───────┼──────────┼──────────┤\n"
for lvl in 1 3 6 9; do
    run_one "xz" "$lvl" "xz -c -${lvl}" "xz -d -c"
done
printf "└────────────┴───────┴──────────┴───────┴──────────┴──────────┘\n"

echo ""

# ═══════════════════════════════════════════════════════════════
printf "┌─────────────────────────────────────────────────────────────────────────────┐\n"
printf "│  brotli — good ratio for text-heavy content                                │\n"
printf "├────────────┬───────┬──────────┬───────┬──────────┬──────────┤\n"
printf "$fmt" "Format" "Level" "Size" "Ratio" "Compress" "Decompr"
printf "├────────────┼───────┼──────────┼───────┼──────────┼──────────┤\n"
for lvl in 1 4 6 9 11; do
    run_one "brotli" "$lvl" "brotli -c -${lvl}" "brotli -d -c"
done
printf "└────────────┴───────┴──────────┴───────┴──────────┴──────────┘\n"

echo ""

# ═══════════════════════════════════════════════════════════════
printf "┌─────────────────────────────────────────────────────────────────────────────┐\n"
printf "│  bzip3 — parallel decompression (bulb's native format)                     │\n"
printf "├────────────┬───────┬──────────┬───────┬──────────┬──────────┤\n"
printf "$fmt" "Format" "Level" "Size" "Ratio" "Compress" "Decompr"
printf "├────────────┼───────┼──────────┼───────┼──────────┼──────────┤\n"
if which bzip3 >/dev/null 2>&1; then
    for blk in 100 200 400 800; do
        run_one "bz3" "b${blk}" "bzip3 -c -b ${blk}" "bzip3 -d -c"
    done
else
    echo "  (bzip3 not found)"
fi
printf "└────────────┴───────┴──────────┴───────┴──────────┴──────────┘\n"

echo ""

# ═══════════════════════════════════════════════════════════════
printf "┌─────────────────────────────────────────────────────────────────────────────┐\n"
printf "│  gzip — legacy baseline                                                    │\n"
printf "├────────────┬───────┬──────────┬───────┬──────────┬──────────┤\n"
printf "$fmt" "Format" "Level" "Size" "Ratio" "Compress" "Decompr"
printf "├────────────┼───────┼──────────┼───────┼──────────┼──────────┤\n"
for lvl in 1 6 9; do
    run_one "gzip" "$lvl" "gzip -c -${lvl}" "gzip -d -c"
done
printf "└────────────┴───────┴──────────┴───────┴──────────┴──────────┘\n"
