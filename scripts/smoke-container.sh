#!/usr/bin/env bash
# Run the production image with an isolated volume and an ephemeral local port.
set -euo pipefail
image="${1:-tempo:local}"
name="tempo-smoke-$$"
volume="$name-data"
cleanup() {
  result=$?
  if [[ "$result" != 0 ]]; then docker logs "$name" >&2 || true; fi
  docker rm -f "$name" >/dev/null 2>&1 || true
  docker volume rm "$volume" >/dev/null 2>&1 || true
}
trap cleanup EXIT
docker volume create "$volume" >/dev/null
docker run -d --name "$name" --read-only --tmpfs /tmp --cap-drop ALL \
  --security-opt no-new-privileges --mount "type=volume,src=$volume,dst=/data" \
  -p 127.0.0.1::8080 "$image" >/dev/null
port="$(docker port "$name" 8080/tcp | head -n 1 | cut -d: -f2)"
base="http://127.0.0.1:$port"
for attempt in $(seq 1 40); do
  if curl --fail --silent "$base/api/health" > /dev/null; then break; fi
  if [[ "$attempt" == 40 ]]; then docker logs "$name"; exit 1; fi
  sleep 1
done
python3 - "$base" <<'PY'
import json, re, sys, urllib.request, urllib.error, urllib.parse
base = sys.argv[1]
def get(path):
    with urllib.request.urlopen(base + path) as r:
        return r.read(), r.headers
raw, headers = get('/api/health')
health = json.loads(raw)
assert health['runtime'] == 'native' and health['engine'].startswith('Stockfish 18'), health
assert headers['cache-control'] == 'no-store'
html, headers = get('/')
assert 'text/html' in headers['content-type']
assert headers['cache-control'] == 'no-store'
assert get('/?review=1')[1]['cache-control'] == 'no-store'
assert get('/index.html')[1]['cache-control'] == 'no-store'
assets = re.findall(r'(?:src|href)="(/assets/[^\"]+)"', html.decode())
assert assets
seen = set()
while assets:
    asset = assets.pop()
    if asset in seen:
        continue
    seen.add(asset)
    body, headers = get(asset)
    assert body
    assert headers['cache-control'] == 'public, max-age=31536000, immutable'
    if asset.endswith('.js'):
        assert b'/rust/chess_review.wasm' not in body
        assert b'stockfish-18-lite' not in body
        # Follow imports too: the chart is fetched only after opening a review.
        for ref in re.findall(r'''["'`](\.?\.?/[^"'`]+\.js)["'`]''', body.decode()):
            resolved = urllib.parse.urljoin(asset, ref)
            if resolved.startswith('/assets/'):
                assets.append(resolved)
assert any('/rating-panel-' in path for path in seen), seen
for path in ['/api/unknown', '/assets/rating-panel-missing.js', '/engine/stockfish.js', '/rust/chess_review.wasm']:
    try:
        get(path)
        raise AssertionError('Unexpected file or route: ' + path)
    except urllib.error.HTTPError as e:
        assert e.code == 404
        assert e.headers['cache-control'] == 'no-store'
print('Container smoke passed: native Stockfish 18, Rust API, UI and lazy chart assets, cache headers, no browser engine.')
PY
test "$(docker exec "$name" id -u)" = 10001
docker exec "$name" test -s /usr/share/stockfish/source.tar.gz
docker exec "$name" sh -c 'printf "durable\n" > /data/smoke.txt'
docker restart --time 90 "$name" >/dev/null
port="$(docker port "$name" 8080/tcp | head -n 1 | cut -d: -f2)"
base="http://127.0.0.1:$port"
test "$(docker exec "$name" cat /data/smoke.txt)" = durable
curl --fail --retry 15 --retry-connrefused --retry-all-errors --retry-delay 1 --silent "$base/api/health" >/dev/null
echo 'Non-root runtime, bundled Stockfish source and container restart passed.'
