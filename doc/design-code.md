# Design

## Algorithms

- configuration
    - remote volumes root
        - `(cred, bucket, dest)`
- archive local snapshots to remote (Cloud) storage
    - full backup: `(timestamp => bytes)`
        - `btrfs send VOLUME.TIMESTAMP | http-put`
    - incremental backup: `(timestamp1, timestamp2) => bytes`
        - `btrfs send VOLUME.T1 --parent VOLUME.T2 => bytes`

## CLI Commands

### `bbkar status`

Inspect the status of in local/remote locations.

### `bbkar run`

1. Inspect current states of Local Roots and Archive Roots.
2. Derive a plan that sync Local Roots to Archive Roots.
3. Execute the plan

### `bbkar dryrun`

Like `bbkar run` but only print the plan instead of executing.

<!--
TODO: more commands
### `bbkar prune`

Prune Archive Roots to save space.

- Identify unnecessary files
    - too old Archive Files (TBD: the condiditions)
    - Unreferenced files (maybe due to an incomplete execution)
- Print (maybe behind a `--dryrun` flag)
- Remove

### `bbkar restore`

Use Archive Files to restore (rebuild) a subvolume.

### `bbkar scrub`

Scrub Archive Files to make sure they matches.

### `bbkar cat-archive`

Stream bytes out of 1 Archive.

Should have similar arguments to `bbkar restore`

-->

## Code architecture

### Executor: `src/service/executor/`

The `Executor` trait is the IO dep for CLI.

### Planner: `src/service/planner`

The `Planner` struct is the IO-free dep for CLI.

### Models: `src/model`

Stateless models. Models should be named using terms listed in [concepts-config.md](concepts-config.md)

### CLI entrypoints: `src/cli/`

Each subcommands like `status` `dryrun` are implemented as a function, with simplar arguments.

Each command impl accepts `Executor` `Planner` and have few other dependencies. This is the sacred DI pattern to make everything pluggable and easier to test.

## Internal Routines

### Inspect

Gather information required, the state of Source and Dest.

### Build Archive Plan

build the plan

- for each source-only subvolume, ensure it is archived by:
    1. *full* backups: btrfs subvolume copied as is, with `btrfs send SUBVOLUME`
        - named `VOLUME.TIMESTAMP[_ID].`
    2. *incremental* backup: btrfs subvolume sent incrementally, with `btrfs send -p BASE_SUBVOLUME SUBVOLUME`
        - named `VOLUME.TIMESTAMP_ID-BASE_TIMESTAMP_[ID]`
### Execute Archive Plan

execute the plan

### Build Restore Plan

<!--
0. user should specify `VOLUME.TIMESTAMP`
1a. if not found, fail
1b. if the backup is a full one, do a `oss-read | btrfs receive`
1c. if the backup is a incremental one, restore its parent
-->

### Execute Restore Plan

### Build Prune Plan

<!--
- max_archive_interval: 86400 // in s
- policy
    - ref: btrbk doc
        - snapshot_preserve
        - snapshot_preserve_min
    - ours: more complicated than btrbk
        - min_full_backup_interval (DURATION)
        - `preserve_max` :: optional DURATION
        - preserve_min: do not 
- encryption (maybe with https://github.com/str4d/rage/) and compression
-->

### Execute Build Prune Plan

## Details

### Transport

Using `opendal` as unified storage API.

### Compression & Chunking

Byte streams are compressed  , then chunked into at-most-512M files.

### Transactional

Things can go wrong exporting subvolumes, sending to remote, and saving them. Therefore mutational operations should be organized into Transactions to prevent inconsistent state and data loss.

When backuping, each transaction should end with an atomical replacement Metadata json. This eliminates the requirement of atomicity in other parts, thus simplifies the design.

Simpliary, when pruning, a `TIMESTAMP` should be removed from metadata first, atomicially. Doing the opposite could leave archives in an inconsistent state.

### Synchronize

### Chunked Storage

This allows uploading to retry at a smaller transport unit.
