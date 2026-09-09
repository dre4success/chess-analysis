"""Validate local and Traefik production configuration without starting containers."""

import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent.parent
IMAGE = "ghcr.io/example/tempo@sha256:" + "a" * 64
DOMAIN = "tempo.example.com"


def compose(filename, **settings):
    return subprocess.run(
        [
            "docker", "compose", "--env-file", os.devnull,
            "--project-name", "tempo", "--file", filename,
            "config", "--format", "json",
        ],
        cwd=ROOT,
        env={
            **os.environ,
            "TEMPO_IMAGE": IMAGE,
            "TEMPO_DOMAIN": DOMAIN,
            "TEMPO_HOST_BIND": "127.0.0.1",
            "TEMPO_PORT": "8080",
            "TEMPO_TRAEFIK_MIDDLEWARES": "",
            **settings,
        },
        capture_output=True,
        text=True,
    )


def config(filename, **settings):
    result = compose(filename, **settings)
    assert result.returncode == 0, result.stderr
    return json.loads(result.stdout)


local = config("compose.yaml")
prod = config("compose.prod.yaml")
service = prod["services"]["tempo"]
labels = service["labels"]
assert service["image"] == IMAGE
assert not service.get("ports"), "Production must route through Traefik"
assert "build" not in service, "Deploy the published image, without server builds"
assert prod["volumes"]["tempo-data"]["name"] == local["volumes"]["tempo-data"]["name"]
for field in ["volumes", "read_only", "tmpfs", "cap_drop", "security_opt", "init",
              "stop_grace_period", "mem_limit", "cpus", "restart"]:
    assert service[field] == local["services"]["tempo"][field], field
assert local["services"]["tempo"]["ports"][0]["host_ip"] == "127.0.0.1"
assert prod["networks"]["web_proxy_net"]["external"] is True
assert prod["networks"]["web_proxy_net"]["name"] == "web_proxy_net"
assert "web_proxy_net" in service["networks"]
assert labels["traefik.enable"] == "true"
assert labels["traefik.docker.network"] == "web_proxy_net"
assert labels["traefik.http.services.tempo.loadbalancer.server.port"] == "8080"
assert labels["traefik.http.routers.tempo.rule"] == f"Host(`{DOMAIN}`)"
assert labels["traefik.http.routers.tempo.entrypoints"] == "websecure"
assert labels["traefik.http.routers.tempo.tls"] == "true"
assert labels["traefik.http.routers.tempo.tls.certresolver"] == "myresolver"
assert labels["traefik.http.routers.tempo.service"] == "tempo"
assert labels["traefik.http.routers.tempo.middlewares"] == ""
assert labels["traefik.http.routers.tempo-http.rule"] == f"Host(`{DOMAIN}`)"
assert labels["traefik.http.routers.tempo-http.entrypoints"] == "web"
assert labels["traefik.http.routers.tempo-http.service"] == "tempo"
assert labels["traefik.http.routers.tempo-http.middlewares"] == "tempo-redirect-to-https"
assert labels["traefik.http.middlewares.tempo-redirect-to-https.redirectscheme.scheme"] == "https"
assert labels["traefik.http.middlewares.tempo-redirect-to-https.redirectscheme.permanent"] == "true"

secured = config("compose.prod.yaml", TEMPO_TRAEFIK_MIDDLEWARES="auth@file,headers@docker")
assert secured["services"]["tempo"]["labels"]["traefik.http.routers.tempo.middlewares"] == "auth@file,headers@docker"
for required in ["TEMPO_IMAGE", "TEMPO_DOMAIN"]:
    result = compose("compose.prod.yaml", **{required: ""})
    assert result.returncode != 0 and required in result.stderr, result.stderr

print("Compose checks passed: Traefik routing/TLS, required settings, optional middleware, no host ports, persistent volume and local defaults.")
