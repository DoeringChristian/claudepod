#!/bin/bash

# Syntax: run-claude.sh <command, arguments>

# Run Claude code in YOLO mode if no command was specified
if [ $# -eq 0 ]; then
  set -- claude --dangerously-skip-permissions --max-turns 99999999
fi

# Mount `pwd` to `/home/code/work` in a Docker container, and ensure the user
# `code` has the right UID/GID to be able to access files there. Mount a
# read-only tmpfs to '/home/code/work/build' subdirectory to prevent Claude
# from accessing it (I am usually working with that directory). Claude was
# instructed to put its builds byproducts into the 'build-claude' subdirectory.

docker run --rm -it \
  -e UID=$(id -u) \
  -e GID=$(id -g) \
  -v "$(pwd):/home/code/work" \
  --tmpfs /home/code/work/build:ro,size=1m \
  -v "$HOME/.claude:/home/code/.claude" \
  -v "$HOME/.claude.json:/home/code/.claude.json" \
  -t \
  --gpus all \
  claude "$@"
