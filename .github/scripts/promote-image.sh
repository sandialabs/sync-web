#!/usr/bin/env bash
set -euo pipefail

image_name="$1"
version_file="${2:-VERSION}"

repo="ghcr.io/${GITHUB_REPOSITORY}/${image_name}"
version="$(cat "$version_file")"

lookup_head_sha() {
  local response
  response="$(curl -fsSL \
    -H "Authorization: Bearer ${GITHUB_TOKEN}" \
    -H "Accept: application/vnd.github+json" \
    -H "X-GitHub-Api-Version: 2022-11-28" \
    "https://api.github.com/repos/${GITHUB_REPOSITORY}/commits/${GITHUB_SHA}/pulls")"
  jq -r '.[0].head.sha // empty' <<<"$response"
}

head_sha="$(lookup_head_sha)"
if [[ -z "$head_sha" ]]; then
  echo "No associated pull request found for ${GITHUB_SHA}; using main commit SHA." >&2
  head_sha="$GITHUB_SHA"
fi

source="${repo}:sha-${head_sha}"
echo "Promoting ${source} to ${repo}:latest and ${repo}:${version}"

docker buildx imagetools inspect "$source" >/dev/null

docker buildx imagetools create \
  -t "${repo}:latest" \
  -t "${repo}:${version}" \
  "$source"
