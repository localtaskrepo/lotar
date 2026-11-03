# LoTaR Docker Image

The official `mallox/lotar` image packages the LoTaR CLI inside a minimal `scratch` container.
It pulls the statically linked musl Linux binary from the latest GitHub release, verifies its
checksum, and wires convenient mount points so you can map your repository and `.tasks` folder
with minimal flags.

## Supported Architectures
- `linux/amd64` (x86_64)
- `linux/arm64` (Apple Silicon / Graviton)

## Quick Start
```bash
# Inspect version
docker run --rm mallox/lotar --version

# Work against your current repo + tasks directory
docker run --rm \
  -v "$PWD":/workspace \
  -v "$PWD/.tasks":/tasks \
  -w /workspace \
  mallox/lotar list --format table
```

### Command Behavior
The image sets `ENTRYPOINT` to `lotar`, so pass subcommands directly (for example, `mallox/lotar list`).
Avoid prefixing the command with `lotar`, which would call `lotar` twice and fail.

If you want to avoid retyping the image name, define a Compose service or a local shell alias.

### Long-Running Server (Docker Compose)
To run the web server in a container, bind to all interfaces inside the container and publish the port on the host's loopback address only:

```yaml
services:
  lotar:
    image: mallox/lotar:latest
    command: ["serve", "--host", "0.0.0.0", "--port", "8080"]
    ports:
      - "127.0.0.1:8080:8080"
    volumes:
      - ./:/workspace
      - ./.tasks:/tasks
    working_dir: /workspace
```

`--open` is intentionally omitted in containers. Use your browser to visit `http://localhost:8080`.

The API has no authentication, and automation rules run shell commands. Publishing the port as `"8080:8080"` makes it reachable from your whole network; keep the `127.0.0.1:` prefix unless the network is trusted or you put your own authentication in front. The server prints a warning when it binds to a non-loopback address, which inside a container is expected. If you reach it through a host name other than `localhost` or an IP address, add `--allow-host <name>` (see [serve](help/serve.md#host-checks)).

Automation `run` commands and agent profiles from the repository need [`lotar trust`](help/trust.md). The container's home directory does not keep the trust record, so either mount one (and set `LOTAR_TRUST_FILE`) or set `LOTAR_TRUST_ALL=1` for a container that only sees your own checkout.

The image sets `LOTAR_TASKS_DIR=/tasks`, so mapping any task directory to `/tasks` is the
preferred workflow. Mounting the whole repository at `/workspace` keeps git metadata available
for commands that need it.

## Alternate Task Locations
Need to work on a shared tasks folder?

```bash
docker run --rm \
  -v "$HOME/shared/tasks":/tasks \
  -v "$HOME/projects/app":/workspace \
  -w /workspace \
  mallox/lotar add "Plan container rollout"
```

## Additional Environment Variables
All CLI environment variables still work:

```bash
docker run --rm \
  -e LOTAR_DEFAULT_ASSIGNEE=ops@example.com \
  -v "$PWD":/workspace \
  -v "$PWD/.tasks":/tasks \
  -w /workspace \
  mallox/lotar add "Containerized task"
```

## Documentation & Support
- Project homepage: https://github.com/localtaskrepo/lotar
- CLI docs: https://github.com/localtaskrepo/lotar/blob/main/docs/README.md
- Issues & support: https://github.com/localtaskrepo/lotar/issues
