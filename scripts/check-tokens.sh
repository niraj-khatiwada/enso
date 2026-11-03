#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
status=0
for file in $(find src -name '*.rs' ! -name tokens.rs ! -name tests.rs | sort); do
	awk -v file="$file" '
		/^#\[cfg\((all\()?test/ { exit }
		/^[[:space:]]*(pub(\([a-z]+\))? )?const / { next }
		{
			line = $0
			while (match(line, /(px|opacity)\(-?[0-9][0-9.]*\)/)) {
				value = substr(line, RSTART, RLENGTH)
				if (value !~ /\(-?[01]\.?\)$/) {
					printf "%s:%d: %s\n", file, NR, value
					found = 1
				}
				line = substr(line, RSTART + RLENGTH)
			}
		}
		END { exit found }
	' "$file" || status=1
done
if [ "$status" -ne 0 ]; then
	echo "Use a token from src/tokens.rs or a named constant for these sizes and opacities." >&2
fi
exit "$status"
