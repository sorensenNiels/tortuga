#!/usr/bin/env bash
# Makes a release (docs/RELEASING.md): bumps the version in Cargo.toml, runs `make verify`,
# commits, tags vX.Y.Z and pushes both, which starts the Release workflow (a draft GitHub release).
#
# Usage: scripts/release.sh patch|minor|major   (or: make release-patch / release-minor / release-major)
set -euo pipefail

fail() {
    echo "tortuga: $*" >&2
    exit 1
}

part=${1:-}
case $part in
patch | minor | major) ;;
*) fail "usage: $0 patch|minor|major" ;;
esac
cd "$(dirname "$0")/.."

# Releases come from an up-to-date, clean main, so the tag points at what CI has seen.
branch=$(git rev-parse --abbrev-ref HEAD)
[ "$branch" = main ] || fail "releases are made from main, not $branch"
[ -z "$(git status --porcelain)" ] || fail "commit or stash your changes first (git status)"
git fetch --quiet --tags origin main
[ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] ||
    fail "main is not the same as origin/main: pull or push first"

# The first `version = "…"` in Cargo.toml is [workspace.package] (the Release workflow reads it the same way).
current=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
[[ $current =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "can't read the version in Cargo.toml (found '$current')"
IFS=. read -r major minor patch <<<"$current"
case $part in
major) next="$((major + 1)).0.0" ;;
minor) next="$major.$((minor + 1)).0" ;;
patch) next="$major.$minor.$((patch + 1))" ;;
esac
tag="v$next"
if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
    fail "tag $tag already exists (delete it with 'git tag -d $tag' if it was never pushed)"
fi

read -r -p "Release $tag (now $current): bump, verify, commit, tag and push? [y/N] " answer
[[ $answer =~ ^[yY]$ ]] || fail "release cancelled"

# Put Cargo.toml and Cargo.lock back if anything fails before the commit.
trap 'git checkout --quiet -- Cargo.toml Cargo.lock; echo "tortuga: release failed, version left at $current" >&2' ERR

awk -v old="version = \"$current\"" -v new="version = \"$next\"" \
    '!done && $0 == old { print new; done = 1; next } { print }' Cargo.toml >Cargo.toml.new
mv Cargo.toml.new Cargo.toml
cargo update --workspace --quiet
make verify

git commit --quiet -m "Release $tag" -- Cargo.toml Cargo.lock
trap - ERR
git tag -a "$tag" -m "Tortuga $next"

git push --atomic origin main "$tag" ||
    fail "pushing failed; the release commit and tag $tag are only local. Push them with: git push --atomic origin main $tag"

echo "tortuga: released $tag. The Release workflow now builds the installers into a draft GitHub release."
echo "tortuga: next: check the installers (docs/RELEASING.md, 'Check an installer') and publish the draft."
