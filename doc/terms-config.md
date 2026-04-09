## Terms

### Snapshots

btrfs snapshot subvolumes. A snapshot name is the full subvolume name on disk: `VOLUME.TIMESTAMP[_ID]` (e.g. `@rootfs.20250101`).

### Timestamps

The suffix part of a snapshot name, without the volume prefix: `TIMESTAMP[_ID]` (e.g. `20250101`, `20250101T1531_1`). This is what btrbk generates and what bbkar uses internally to identify archives.

`bbkar` expects snapshot subvolumes to be readonly and named `VOLUME.TIMESTAMP[_ID]`, just like what `btrbk` creates.

### Series

1 or multiple snapshots creating from 1 "live" subvolume, and have the same `VOLUME` part in names.

### Source

Local directory containing snapshot volumes.

Corresponds to a `[source]` block in config file.

Multiple Series can share a directory.

### Destination

Local or remote location to store the backup files. Corresponds to a `[dest]` block in config file.

The dest location can be a local directory, or a directory-ish in storage service.

#### File structure

bbkar is a stateless program. Backuped bytes and their metadata are storaged in plain files in destination:

- `DEST/VOLUME/bbkar-meta.yaml` the metadata
    - list of other files, and the information to manage them.
- `DEST/VOLUME/TIMESTAMP[_ID]/part000001.btrfs.zstd` the bytes
    - created by streaming the output of `btrfs send`
    - could be compressed and/or encrypted.

## Configuration

bbkar configuration files are in TOML. A simplest configuration file to backup all subvolume families in a directory to local files is like:

```toml
[source.data-root]
path = "/media/data-root/_btrbk_snap"

[dest.data-root-archive]
path = "/media/archive-root/bbkar-backups"

[sync.1]
source = "data-root"
dest = "data-root-archive"
```

### Sending

Controls how new snapshots are sent to the destination. By default, bbkar sends incrementally when a suitable parent exists on the destination, and falls back to a full send otherwise. These options add guardrails:

- **min_full_send_interval** (duration, e.g. `"1m"`, `"1w"`): Ensures full sends happen at least this often. When the most recent full send for a series is older than this interval, the next backup is sent as full. Periodic full sends create independent restore points, limit the blast radius of storage corruption, and provide natural breakpoints for pruning. Default: `"1w"`.

- **max_incremental_depth** (integer, e.g. `30`): The maximum number of incremental sends stacked on top of a single full send. When this depth is reached, the next backup is sent as full. Prevents runaway chains when snapshot frequency is high (e.g. hourly snapshots with a monthly full interval). Default: `30`.

Whichever of the two triggers first wins.

### Retention

Controls which existing archives are kept on the destination when pruning. Follows btrbk's two-tier retention model:

- **archive_preserve_min** (duration or `"all"`): Keep **all** archives newer than this duration. Acts as a safety floor -- nothing younger gets thinned regardless of the schedule. Default: `"all"` (keep everything).

- **archive_preserve** (schedule string, e.g. `"30d 12w 6m *y"`): Schedule-based thinning for archives older than `archive_preserve_min`. Format: `[<daily>d] [<weekly>w] [<monthly>m] [<yearly>y]`. Within each time bucket, bbkar keeps the oldest snapshot in that period (the "first" of each day/week/month/year). Use `*` for "keep all" and `0` for "keep none" at each granularity. Only takes effect when `archive_preserve_min` is not `"all"`.

- **preserve_day_of_week** (weekday name, e.g. `"sunday"`): Which day starts a "week" for weekly retention bucketing. Default: `"sunday"`.

## Pruning and storage budget

bbkar will only prune old backup files in `bbkar run --prune` run. `bbkar dryrun --prune` can be used to preview what it will remove.

To help managing the budget, the storage in use is printed in `bbkar status` and `bbkar run`.
