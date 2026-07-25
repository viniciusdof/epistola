#!/usr/bin/env bash
set -euo pipefail

# Requires CODEBERG_TOKEN, SHA, and TAG_NAME in the environment, and release
# artifacts already collected in dist/.

api="https://codeberg.org/api/v1/repos/viniciusdof/epistola"

payload="$(jq -n --arg sha "$SHA" --arg tag "$TAG_NAME" '{
  tag_name: $tag,
  name: $tag,
  body: ("Automated nightly build from commit " + $sha + ". Unstable — expect breakage and reverted features."),
  prerelease: true,
  target_commitish: $sha
}')"

response="$(curl -s -w '\n%{http_code}' -X POST -H "Authorization: token $CODEBERG_TOKEN" \
  -H "Content-Type: application/json" -d "$payload" "$api/releases")"
http_code="$(tail -n1 <<< "$response")"
body="$(sed '$d' <<< "$response")"
if [ "$http_code" -ge 300 ]; then
  echo "Failed to create Codeberg release (HTTP $http_code):" >&2
  echo "$body" >&2
  exit 1
fi
release_id="$(jq -r '.id' <<< "$body")"

for f in dist/*.exe dist/*.dmg dist/*.deb dist/*.AppImage dist/*.tar.gz; do
  [ -e "$f" ] || continue
  response="$(curl -s -w '\n%{http_code}' -X POST -H "Authorization: token $CODEBERG_TOKEN" \
    -F "attachment=@$f" "$api/releases/$release_id/assets?name=$(basename "$f")")"
  http_code="$(tail -n1 <<< "$response")"
  if [ "$http_code" -ge 300 ]; then
    echo "Failed to upload $f (HTTP $http_code):" >&2
    sed '$d' <<< "$response" >&2
    exit 1
  fi
done
