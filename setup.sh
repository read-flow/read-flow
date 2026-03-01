#!/usr/bin/env bash
rm -f .env
mkdir -p ~/.local/share/read-flow
cat read-flow.template.toml | envsubst > read-flow.toml
