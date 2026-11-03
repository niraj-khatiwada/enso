#!/bin/sh
set -eu
cd "$(dirname "$0")/.."

version="${1:-}"
case "$version" in
    v*) version=${version#v} ;;
esac
if ! printf '%s' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$'; then
    echo "Usage: make release VERSION=1.2.3   (or 1.2.3-beta.1 for a pre-release)" >&2
    exit 1
fi
tag="v$version"
current=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)

branch=$(git rev-parse --abbrev-ref HEAD)
if [ "$branch" != main ]; then
    echo "Release from main (you are on $branch)." >&2
    exit 1
fi
if [ -n "$(git status --porcelain)" ]; then
    echo "Commit or stash your changes first." >&2
    exit 1
fi
git fetch --quiet --tags origin main
if [ "$(git rev-parse HEAD)" != "$(git rev-parse origin/main)" ]; then
    echo "main is not up to date with origin/main: pull or push first." >&2
    exit 1
fi
if git rev-parse --quiet --verify "refs/tags/$tag" >/dev/null; then
    echo "$tag already exists." >&2
    exit 1
fi
latest=$(git tag --list 'v*' --sort=-v:refname | head -n 1)
if [ -n "$latest" ] && [ "$(printf '%s\n%s\n' "${latest#v}" "$version" | sort -V | tail -n 1)" != "$version" ]; then
    echo "$tag is not newer than the latest release, $latest." >&2
    exit 1
fi

if [ "$current" != "$version" ]; then
    sed -i.bak "0,/^version = \"$current\"/s//version = \"$version\"/" Cargo.toml
    rm -f Cargo.toml.bak
    cargo update --workspace --offline >/dev/null 2>&1 || cargo update --workspace >/dev/null
fi
cargo test --locked --quiet >/dev/null

if [ "$current" != "$version" ]; then
    git add Cargo.toml Cargo.lock
    git commit --quiet -m "Release $tag"
fi
git tag -a "$tag" -m "enso $tag"
git push --quiet origin main "$tag"

repo=$(git remote get-url origin | sed -E 's#(git@github.com:|https://github.com/)##; s#\.git$##')
echo "Tagged $tag. The release builds at https://github.com/$repo/actions and appears at https://github.com/$repo/releases/tag/$tag"
