#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
cargo build --release --example bench
BIN=target/release/examples/bench
RUN=""
if [ "$(uname)" = "Linux" ] && [ -z "${DISPLAY:-}" ] && [ -z "${WAYLAND_DISPLAY:-}" ]; then
	if command -v xvfb-run >/dev/null 2>&1; then
		RUN="xvfb-run -a -s '-screen 0 1280x900x24'"
	else
		echo "No display and no xvfb-run: run this from a desktop session." >&2
		exit 1
	fi
fi
echo "# enso visual editor benchmark"
echo
echo "Host: $(uname -srm). Times in ms, CPU-side (render to end of paint); GPU raster and presentation not included."
echo "work = input handler + frame; wait = time until the next display frame, reported apart."
for size in ${BENCH_SIZES:-1k 50k 200k 255k}; do
	eval "$RUN $BIN $size" 2>/dev/null | grep -v '^$' || echo "($size failed)"
done
echo
echo "Per-sample JSON: target/bench/<size>.jsonl"
