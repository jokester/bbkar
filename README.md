# bbkar

bbkar for btrbk-archiver or btrfs backup archiver.

Save btrbk-cretead snapshots into object storage services.

## Convention used by `btrbk`

Snapshot subvolumes are named `NAME.TIMESTAMP[_ID]`

## How does it work?

- configuration
    - remote volumes root
        - `(cred, bucket, location)`
        - `BUCKET/LOCATION/VOLUME/TIMESTAMP.zstd`
        - `BUCKET/LOCATION/VOLUME/bbkar.json`
    - local volumes root
        - prefix
    - policy
        - ref: btrbk doc
            - snapshot_preserve
            - snapshot_preserve_min
        - ours: more complicated than btrbk
            - min_full_backup_interval (DURATION)
            - `preserve_max` :: optional DURATION
            - preserve_min: do not 
- assuming intergrity of btrbk-created snapshots, in a local-mounted filesystem
- archive local snapshots to remote (Cloud) storage
    - full backup: `(timestamp => bytes)`
        - `btrfs send VOLUME.TIMESTAMP | http-put`
    - incremental backup: `(timestamp1, timestamp2) => bytes`
        - `btrfs send VOLUME.T1 --parent VOLUME.T2 => bytes`

### concepts

- archives
- snapshots are identified by `(prefix, timestamp)` tuple
- snapshots is archived IFF it can be recovered from remote files
    - a full archive exists
    - an `(full-archive, incremental+)` chain exists

### routine: backup

- for each local volume
    - ignore, if it exceeds defined `preserve_max`
    - ignore, if it is already ARCHIVED
    - otherwise it should be archived
        - create 
- unless skipped, run 'prune' routine too to remove outdated/unnecessary archives

### routine: fsck


### routine: info

### routine: prune
### routine: store

### Backup

1. *full* backups: btrfs subvolume copied as is, with `btrfs send SUBVOLUME`
    - named `NAME.TIMESTAMP[_ID].`
2. *backup* backup: btrfs subvolume sent incrementally, with `btrfs send -p BASE_SUBVOLUME SUBVOLUME`
    - named `NAME.TIMESTAMP_ID-BASE_TIMESTAMP_[ID]`

(in OSS there can be file splitting / encryption / compression. )

### Restore

0. user should specify `NAME.TIMESTAMP`
1a. if not found, fail
1b. if the backup is a full one, do a `oss-read | btrfs receive`
1c. if the backup is a incremental one, restore its parent

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
