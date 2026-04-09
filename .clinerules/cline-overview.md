# Project overview

This project, `bbkar`, or btrbk archiver, is a CLI tool to archive snapshots created by `btrbk`.

[btrbk](https://github.com/digint/btrbk) is another program to create readonly snapshot subvolumes from btrfs "live" subvolume. 

`bbkar` export snapsnot subvolumes as archive files, and save the archive files in local filesystem or object storage services. See `doc/design.md` for terms, concepts and planned features.

## Development Status

Current status: pre-MVP

## Features

- [ ] Full backup
- [ ] Incremental backup
- [ ] ZSTD compression
- [ ] Backup to local filesystem
- [ ] Backup to GCS
- [ ] Backup to S3
- [ ] Rebuild subvolume from Archives, by printing a series of commands using `bbkar cat-archive`.

<!--

NOT TODOs
- Encryption
- Performance
    - Parallel
- Rebuild subvolume as a whole program.
-->

### Internals

Data Structure:

- [ ] Config
- [ ] Remote Metadata
- [ ] Local Inspectation
- [ ] Remote Inspectation
- [ ] Backup Plan

Routines

- [ ] Local Inspection
- [ ] Remote Inspectation
- [ ] Derive Backup Plan
- [ ] Print Backup Plan
- [ ] Execute Backup Plan
- [ ] Derive restore Plan
- [ ] Print Restore Plan

## Code structure

All rust code should be in `src/`.

- `src/bin/bbkar.rs`: CLI entrypoint
- `src/lib.rs`: internal modules + subcommands. Please refer to this file for smaller internal units.
- `src/cmd/`: CLI subcommands
- `src/model/`: data classes and errors
- `src/routine/`: internal routines
- `src/deps`: shared utils and deps

## Tests

Test code should be written in private mod-s , in the same file of testee code.