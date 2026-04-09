# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## About this project

see README.md

## About the architecture, design and code convention

see doc/design-code.md

## Configuration

See examples/local.toml for a simplest configuration file.

See src/model/config.rs for supported configuration options.

## Development Status

Please check doc/milestone.md

## Checklist when making changes

- Ensure it compiles (`cargo build`)
- Ensure all tests pass (`cargo test`)
- Ensure the code matches documents in (doc/)
- Format the code (`cargo fmt`)
- Ensure the code is linted (`cargo clippy`)
