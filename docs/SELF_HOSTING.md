# Self-hosting Tempo

Tempo runs as one container: Rust serves the React UI and API, and a native Stockfish 18 process analyses games. SQLite stores the review queue, results and cache in a persistent volume. Jobs continue without a connected browser and interrupted jobs resume after restart.

The Docker and GitHub Actions files are prepared locally. No image has been published to a registry and no remote deployment has been run.

## Local Docker

```sh
docker compose up --build -d --wait
```

Open http://localhost:8080. The default host binding is `127.0.0.1:8080`. Set `TEMPO_PORT` if this port is occupied. Copy `.env.example` to `.env` to save local overrides.

```sh
docker compose logs -f --tail 100
docker compose stop
docker compose up -d --no-build --wait
```

`compose.yaml` uses a named `tempo-data` volume. Stopping, recreating or updating the container preserves it. `docker compose down -v` deletes it, including saved reviews. Keep a volume backup before upgrading. One server process owns a volume; a filesystem lock prevents two review workers from sharing it.

The container runs as UID/GID 10001 with a read-only root filesystem and a writable `/data` volume. The Compose defaults allow 1 GiB RAM and two CPUs; Stockfish uses one analysis thread and 64 MiB hash. The full NNUE network, Rust runtime and UI need additional memory. A 90-second shutdown grace period allows the current bounded engine search or archive request to stop cleanly. Hard termination is recoverable through SQLite and the per-game cache.

## Production with your existing Traefik

`compose.prod.yaml` is a standalone production definition. It matches Sela's current proxy configuration:

- External network: `web_proxy_net`, also selected explicitly by the `traefik.docker.network` label.
- HTTPS entrypoint: `websecure`, with the existing `myresolver` certificate resolver.
- HTTP entrypoint: `web`, with Tempo's own `tempo-redirect-to-https` middleware.
- Container service port: `8080`. No host port is published, so another app such as Bethel can also use 8080 inside its own container.

Tempo uses SQLite in its persistent `tempo-data` volume, so it needs only the proxy network. It does not need a PostgreSQL container or a second database network. Both Compose definitions use project name `tempo`, keeping the same `tempo_tempo-data` volume across updates.

Follow your `deploy` user's `~/apps/<project>/` convention: use `/home/deploy/apps/tempo` as `DEPLOY_PATH` if that is the user's home on the target VPS. The workflow installs the production definition there as `docker-compose.yml`. Before the first deployment, copy `.env.production.example` into that directory as `.env` and set:

```dotenv
TEMPO_DOMAIN=your-chosen-hostname.example.com
# Optional existing middleware chain, e.g. your authentication middleware:
TEMPO_TRAEFIK_MIDDLEWARES=
```

Point that hostname's DNS at your VPS. The existing Traefik container must already be attached to `web_proxy_net` with the named entrypoints and resolver configured. The workflow checks that the network exists; it does not install or restart Traefik.

The server's `.env` persists untouched across deployments. The chosen image digest is stored separately in `tag.env` after the new container becomes healthy. From the server directory, later operations use both files:

```sh
docker compose -f docker-compose.yml --env-file .env --env-file tag.env ps
docker compose -f docker-compose.yml --env-file .env --env-file tag.env logs -f --tail 100
docker compose -f docker-compose.yml --env-file .env --env-file tag.env up -d --no-build --wait
```

For a manual first deployment, put `TEMPO_IMAGE=ghcr.io/owner/repository@sha256:<digest>` in `tag.env`, copy `compose.prod.yaml` to the server as `docker-compose.yml`, and use those same options with `pull` and then `up`. Use the standalone production file by itself; combining it with local `compose.yaml` would inherit local port publishing.

## Engine and image contents

The Dockerfile builds unmodified full Stockfish 18 from commit `cb3d4ee9b47d0c5aae855b12379378ea1439675c`. Stockfish's own build downloads, verifies and embeds the complete default networks. The AMD64 build uses the portable `x86-64` target; ARM64 uses `armv8`. These are full engines with portable CPU instructions. Their performance differs from builds tuned to a specific newer processor.

The runtime contains:

- `/usr/local/bin/chess-review`: Rust web service and CLI.
- `/usr/local/bin/stockfish`: native Stockfish 18 with embedded networks.
- `/app/ui`: static React UI, local fonts and the real example review.
- `/usr/share/stockfish/COPYING.txt`: Stockfish GPLv3 licence.
- `/usr/share/stockfish/source.tar.gz`: corresponding source, build scripts and networks used to produce the bundled engine.

There is no Node runtime, WASM engine, browser worker, Cloudflare service or external database dependency. Building needs access to npm, crates.io, Debian packages, GitHub and Stockfish's network downloads. Running needs HTTPS access to the Chess.com public API.

## GitHub Actions

Three workflows are provided:

GitHub Actions is intentionally disabled on the repository for now. The workflow definitions are committed for later use, and none has a push or pull-request trigger. When the domain and server configuration are ready, enable Actions in the repository settings, then run the manual workflows below.

| Workflow            | Trigger                 | Behaviour                                                                                                             |
| ------------------- | ----------------------- | --------------------------------------------------------------------------------------------------------------------- |
| Check Tempo         | Manual or reusable call | Prettier, types, lint, frontend tests/build, Rust formatting/tests/Clippy, Docker build and runtime smoke test        |
| Publish Tempo image | Manual                  | Runs checks, builds and tests native AMD64 and ARM64 images, publishes to GHCR, and creates a combined image manifest |
| Deploy Tempo        | Manual                  | Pulls a chosen immutable image digest over SSH and starts it with Compose, waiting for a healthy container            |

Actions are pinned to commit hashes. The publishing workflow uses GitHub's `GITHUB_TOKEN` with package write permission. It prints the resulting image name and digest in its summary; use `ghcr.io/owner/repository@sha256:<digest>` for deployment. Building and publishing do not deploy anything. The deployment workflow accepts only an image from the current repository's GHCR package.

Before running deployment, configure a GitHub environment named `production`:

| Name                 | Kind     | Value                                                                    |
| -------------------- | -------- | ------------------------------------------------------------------------ |
| `DEPLOY_HOST`        | Variable | Server hostname or IPv4 address                                          |
| `DEPLOY_USER`        | Variable | SSH user with permission to run Docker; your documented user is `deploy` |
| `DEPLOY_PATH`        | Variable | Writable absolute directory, e.g. `/home/deploy/apps/tempo` (no spaces)  |
| `DEPLOY_PORT`        | Variable | Optional SSH port, default `22`                                          |
| `DEPLOY_SSH_KEY`     | Secret   | Private key for the deployment user                                      |
| `DEPLOY_KNOWN_HOSTS` | Secret   | Verified SSH known-hosts entry for this server and port                  |

The server needs Docker Engine with the Compose plugin. If the GHCR package is private, log the deployment user into GHCR on the server once with a token that can read that package. The workflow does not transfer registry tokens or disable SSH host verification.

Choose **Publish Tempo image → Run workflow**. Once it passes, choose **Deploy Tempo → Run workflow** and supply the image by digest. Deployment installs `compose.prod.yaml` as `docker-compose.yml`, loads the server's existing `.env`, pulls the selected image, and uses `up --no-build --wait`. It leaves the named volume intact and records the successful image in `tag.env` and `.deployed-image`. If startup fails, the prior `tag.env` remains available for an explicit rollback; the workflow does not automatically roll back a running container.

For rollback, run Deploy Tempo again using the previous digest. The initial database schema is additive; later releases should document migration and rollback compatibility before rollout.

## Server access

Traefik routes your configured hostname to Tempo's container on port `8080` over `web_proxy_net`. The UI and `/api` use the same origin, with polling over ordinary HTTP requests. No WebSocket or cross-origin isolation settings are required. The loopback binding in `compose.yaml` is for local development; production uses the standalone Traefik definition.

Preserve the server's cache headers: HTML, API responses and missing files use `no-store`; successful content-hashed `/assets/` files use a year-long immutable cache. If an open tab requests a chart chunk removed by an update, Tempo reloads the current review URL once. Persistent or offline failures show a chart retry panel while keeping the rest of the dashboard available.

This is a personal server application without its own login system. Reviews on the instance are shared. To use your existing proxy authentication, set `TEMPO_TRAEFIK_MIDDLEWARES` in the server's `.env` to its middleware name and provider, such as `auth@file`. Multiple existing middlewares can be comma-separated. TLS and domain configuration belong to Traefik.

## Configuration without Docker

```sh
npm --prefix web ci
npm --prefix web run build
cargo build --locked --release
./target/release/chess-review serve --engine /absolute/path/to/stockfish
```

| Environment variable | Default outside Docker | Purpose                                  |
| -------------------- | ---------------------- | ---------------------------------------- |
| `TEMPO_BIND`         | `127.0.0.1:8080`       | Rust listen address                      |
| `TEMPO_DATA_DIR`     | `tempo-data`           | SQLite and archive cache directory       |
| `TEMPO_UI_DIR`       | `web/dist/selfhost`    | Frontend build directory                 |
| `STOCKFISH_PATH`     | `stockfish`            | Native engine path or executable on PATH |

The corresponding `serve --bind`, `--data-dir`, `--ui-dir` and `--engine` flags override environment variables.

## API and verification

`GET /api/health` identifies the native engine and runtime. `POST /api/reviews` accepts `{ "username": "name", "pace": "rapid" }` and returns a persistent job ID. `GET /api/reviews/<id>` returns imported games, progress and the canonical Rust review as games complete. `GET /api/reviews` lists recent job summaries. `POST /api/reviews/<id>/cancel` accepts an empty JSON object and stops an active job. To extend a review, POST its `parent_id` and selected `game_urls` alongside the same username and pace.

The queue allows at most 32 active jobs and deduplicates active submissions for a player/time format. One worker processes games in order. Completed results are reused only when engine, Rust core, player and PGN identities match. Excluded games are counted; failed requests retain completed work and can be retried.

```sh
cargo fmt --all --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
STOCKFISH=/absolute/path/to/stockfish cargo test --locked server::tests::native_job_reuses -- --ignored --nocapture
bash scripts/smoke-container.sh tempo:local
python3 scripts/check-compose.py
```

The native integration test analyses a real completed game, validates the result, proves that cache reuse performs no searches, and checks invalidation after a core change. The container smoke test checks native Stockfish identity, static assets including the lazy chart chunk, cache headers, API errors, absence of browser engine assets, non-root execution and restart persistence. Compose checks render both configurations without starting containers and verify routing, HTTPS redirection, required values, optional middleware, persistent storage and production's lack of host ports.

References: [Stockfish 18 build](https://github.com/official-stockfish/Stockfish/tree/sf_18), [GitHub container publishing](https://docs.github.com/en/actions/tutorials/publish-packages/publish-docker-images), [Traefik Docker labels](https://doc.traefik.io/traefik/reference/routing-configuration/other-providers/docker/), [Compose environment-file interpolation](https://docs.docker.com/compose/how-tos/environment-variables/variable-interpolation/).
