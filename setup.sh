#!/usr/bin/env bash
rm -f .env
mkdir -p ~/.local/share/archive-organizer
cat archive-organizer.template.toml | envsubst > archive-organizer.toml
