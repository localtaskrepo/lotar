#!/bin/sh
# Entrypoint of the mallox/lotar image (DEV-289).
#
# Tasks are found in the mounted repository as usual, so git-backed commands
# (history, stats, changelog) see them inside the repository. A directory
# mounted at /tasks is used only when the caller did not choose a tasks
# directory and the working directory has no initialized .tasks folder, which
# keeps `-v <tasks>:/tasks` working for a tasks folder outside any repository.
set -eu

repo_has_tasks() {
    dir=$PWD
    while :; do
        [ -f "$dir/.tasks/config.yml" ] && return 0
        [ "$dir" = / ] && return 1
        dir=$(dirname "$dir")
    done
}

if [ -z "${LOTAR_TASKS_DIR:-}" ] \
    && awk '$5 == "/tasks" { found = 1 } END { exit !found }' /proc/self/mountinfo 2>/dev/null \
    && ! repo_has_tasks; then
    export LOTAR_TASKS_DIR=/tasks
fi

exec /usr/local/bin/lotar "$@"
