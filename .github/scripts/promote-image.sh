#!/usr/bin/env bash
set -euo pipefail

image_name="$1"
version_file="${2:-VERSION}"
variant="${3:-}"
preferred="${4:-}"

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

suffix="${variant:+-${variant}}"
source="${repo}:sha-${head_sha}${suffix}"
tags=(-t "${repo}:latest${suffix}" -t "${repo}:${version}${suffix}")
if [[ $preferred == preferred ]]; then
  tags+=(-t "${repo}:latest" -t "${repo}:${version}")
fi
echo "Promoting ${source} to variant${preferred:+ and preferred} tags"

docker buildx imagetools inspect "$source" >/dev/null

docker buildx imagetools create "${tags[@]}" "$source"
