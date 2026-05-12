#!/bin/sh
set -eu

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
repo_root="$(CDPATH= cd -- "$script_dir/../.." && pwd)"
version="${ROVER_VERSION:-v0.0.1-alpha.1}"
repo="${ROVER_REPO:-thalesgelinger/rover}"
bin_image="${ROVER_BIN_IMAGE:-rover:bin-local}"
debian_image="${ROVER_DEBIAN_IMAGE:-rover:debian-local}"
alpine_image="${ROVER_ALPINE_IMAGE:-rover:alpine-local}"

docker build \
  -f "$repo_root/docker/bin.Dockerfile" \
  --build-arg "ROVER_VERSION=$version" \
  --build-arg "ROVER_REPO=$repo" \
  -t "$bin_image" \
  "$repo_root"

for kind in debian alpine; do
  image_var="${kind}_image"
  image="$(eval "printf '%s' \"\${$image_var}\"")"
  docker build \
    -f "$repo_root/docker/$kind.Dockerfile" \
    --build-arg "ROVER_VERSION=$version" \
    --build-arg "BIN_IMAGE=$bin_image" \
    -t "$image" \
    "$repo_root"

  docker run --rm --entrypoint sh "$image" -c '! command -v cargo && rover --help >/tmp/rover-help.txt'

  tmp_dir="$(mktemp -d)"
  trap 'rm -rf "$tmp_dir"' EXIT INT TERM
  cat > "$tmp_dir/app.lua" <<'LUA'
local api = rover.server { host = "127.0.0.1", port = 4242 }

function api.health.get(ctx)
  return { ok = true }
end

return api
LUA

  container_id="$(docker run -d --rm -v "$tmp_dir:/app" "$image" run /app/app.lua)"
  trap 'docker rm -f "$container_id" >/dev/null 2>&1 || true; rm -rf "$tmp_dir"' EXIT INT TERM

  for _ in $(seq 1 30); do
    if docker exec "$container_id" sh -c 'if command -v curl >/dev/null 2>&1; then curl -fsS http://127.0.0.1:4242/health; else wget -qO- http://127.0.0.1:4242/health; fi' | grep -q '"ok"'; then
      docker rm -f "$container_id" >/dev/null 2>&1 || true
      rm -rf "$tmp_dir"
      trap - EXIT INT TERM
      continue 2
    fi
    sleep 1
  done

  docker logs "$container_id"
  exit 1
done
