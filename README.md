# bbkar or btrbk archiver

Incrementally backup btrbk-cretead snapshots into object storage services.

## Convention used by `btrbk`

Snapshot subvolumes are named `NAME.TIMESTAMP[_ID]`

## How does it work?

### Backup

1. *parent* backups: btrfs subvolume copied as is, with `btrfs send SUBVOLUME`
    - named `NAME.TIMESTAMP[_ID].`
2. *backup* backup: btrfs subvolume sent incrementally, with `btrfs send -p BASE_SUBVOLUME SUBVOLUME`
    - named `NAME.TIMESTAMP_ID-BASE_TIMESTAMP_[ID]`

(in OSS there can be file splitting / encryption / compression. )

### Restore

0. user should specify `NAME.TIMESTAMP`
1a. if not found, fail
1b. if the backup is a full one, do a `oss-read | btrfs receive`
1c. if the backup is a incremental one, restore its parent
<!-- 3. TODO compress and encryption -->

## CONFIG FILE

```toml
[volumes]

[storage.x]
vendor=X # opendal-supported 


```

## Usage

```
# list stat
btrbkar status

btrbkar backup NAME_OR_PATH

btrbkar restore NAME DEST
```

## OPTIONS

-c CONFIG
-b BACKEND
--no-prune only rir `bbkar`
SNAPSHOT_PREFIX , `_btrbk_snapshots/NAME`

## bbl

The idea is inspired by [this comment](https://github.com/digint/btrbk/issues/123#issuecomment-1114320750), [btrfs-send-to-s3](https://github.com/kubrickfr/btrfs-send-to-s3) and others.

## FUTURE

- encryption (maybe with https://github.com/str4d/rage/) and compression
