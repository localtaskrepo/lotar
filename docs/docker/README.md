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
