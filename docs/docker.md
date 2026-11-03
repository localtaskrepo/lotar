# LoTaR Docker Image

The official `mallox/lotar` image packages the LoTaR CLI in a small Alpine Linux image with
`git`. It pulls the statically linked musl Linux binary from the GitHub release, verifies its
checksum, and works in your repository mounted at `/workspace`.

## Supported Architectures
- `linux/amd64` (x86_64)
- `linux/arm64` (Apple Silicon / Graviton)

## Tags
- `mallox/lotar:<version>` (for example `mallox/lotar:0.11.2`): one release.
- `mallox/lotar:latest`: the newest stable release.
- `mallox/lotar:next`: the newest prerelease (for example `1.0.0-rc.1`). Prereleases never
  move `latest`, and a stable release does not move `next`, so after a stable release `next`
  can be older than `latest`.

## Quick Start
```bash
# Inspect version
docker run --rm mallox/lotar --version

# Work in the repository in the current directory
docker run --rm \
  --user "$(id -u):$(id -g)" \
  -v "$PWD":/workspace \
  mallox/lotar list --format table
```

The working directory is `/workspace`, and LoTaR finds the repository's `.tasks` folder there.
Because the tasks are inside the mounted repository, the commands that read git history work:
`task history`, `stats`, `changelog`, the web UI's history and Insights views, and
`scan --modified-only`. The image includes `git` and trusts mounted repositories whatever
their owner (`safe.directory = *` in its system git configuration).

### File Ownership
The image runs as UID and GID 1000 by default. On Linux, a bind mount keeps the host's file
owners, so LoTaR can only write to your repository if that is your UID, and new task files
would belong to UID 1000. Pass `--user "$(id -u):$(id -g)"` (as above) so the container runs
as you. Any UID works: `HOME` is `/home/lotar`, which every user can write to. Docker Desktop
on macOS and Windows maps file ownership itself, so the flag is optional there.

### Command Behavior
The image's entrypoint runs `lotar`, so pass subcommands directly (for example, `mallox/lotar list`).
Avoid prefixing the command with `lotar`, which would call `lotar` twice and fail. To get a
shell in the image, use `--entrypoint sh`.

If you want to avoid retyping the image name, define a Compose service or a local shell alias.

### Long-Running Server (Docker Compose)
To run the web server in a container, bind to all interfaces inside the container and publish the port on the host's loopback address only:

```yaml
services:
  lotar:
    image: mallox/lotar:latest
    command: ["serve", "--host", "0.0.0.0", "--port", "8080"]
    # On Linux, your UID and GID (`id -u`, `id -g`); see File Ownership.
    user: "1000:1000"
    ports:
      - "127.0.0.1:8080:8080"
    volumes:
      - ./:/workspace
```

`--open` is intentionally omitted in containers. Use your browser to visit `http://localhost:8080`.

If the browser does not update when you change task files on the host (common with bind mounts on macOS and Windows), add `LOTAR_ENABLE_POLL_WATCH=1` to the service's `environment`. The server then also checks for changes every 200 ms (`LOTAR_POLL_WATCH_INTERVAL_MS` changes this).

The API has no authentication, and automation rules run shell commands. Publishing the port as `"8080:8080"` makes it reachable from your whole network; keep the `127.0.0.1:` prefix unless the network is trusted or you put your own authentication in front. The server prints a warning when it binds to a non-loopback address, which inside a container is expected. If you reach it through a host name other than `localhost` or an IP address, add `--allow-host <name>` (see [serve](help/serve.md#host-checks)).

Automation `run` commands and agent profiles from the repository need [`lotar trust`](help/trust.md). The trust record lives in the container's home directory, which is lost with the container, so either mount one (and set `LOTAR_TRUST_FILE`) or set `LOTAR_TRUST_ALL=1` for a container that only sees your own checkout.

Automation `run` commands use the image's `/bin/sh` and have Alpine's base tools plus `git`.
Agent CLIs such as `claude` or `codex` are not included; to run agent jobs, build your own
image `FROM mallox/lotar` that adds them.

## Alternate Task Locations
A tasks folder outside any repository can be mounted at `/tasks`. The image uses `/tasks` when
it is mounted and neither the working directory nor a parent has an initialized `.tasks` folder:

```bash
docker run --rm \
  --user "$(id -u):$(id -g)" \
  -v "$HOME/shared/tasks":/tasks \
  mallox/lotar add "Plan container rollout"
```

To use `/tasks` next to a repository that has its own `.tasks` folder, choose it explicitly
with `-e LOTAR_TASKS_DIR=/tasks` (or `--tasks-dir /tasks`). Commands that read git history
only see tasks inside the mounted repository, so keep the tasks of a repository in its own
`.tasks` folder. Older instructions also mounted `"$PWD/.tasks":/tasks` next to the repository;
that still works, and LoTaR uses the repository's copy.

## Additional Environment Variables
All CLI environment variables still work:

```bash
docker run --rm \
  --user "$(id -u):$(id -g)" \
  -e LOTAR_DEFAULT_ASSIGNEE=ops@example.com \
  -v "$PWD":/workspace \
  mallox/lotar add "Containerized task"
```

## Documentation & Support
- Project homepage: https://github.com/localtaskrepo/lotar
- CLI docs: https://github.com/localtaskrepo/lotar/blob/main/docs/help/index.md
- Issues & support: https://github.com/localtaskrepo/lotar/issues
