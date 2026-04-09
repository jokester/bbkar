# bbkar

A CLI tool to export btrbk-created snapshot subvolumes into backup locations. Backup locations can be object-storage-like services, or local directory.

bbkar is short for btrbk archiver or btrfs backup archiver.

## Features

- Export btrfs snapshot subvolumes, full or incremental, to compressed files.
- Configurable policy for incremental send and archive retention
- Support multiple storage backends via [Apache Opendal](https://opendal.apache.org/)

## Config

`bbkar` is configured in toml. This example toml will back up btrbk snapshots to s3:

```toml
[source.rootdisk]
path = "/media/rootdisk/_btrbk_snap"

[dest.s3]
driver = "s3"
bucket = "test-s3-bucket"
path = '/hostname/rootdisk/'
region = "us-east-1"
access_key_id = 'aabbcc'
secret_access_key = 'abcccc'

[sync.rootdisk-to-s3]
source = "rootdisk"
dest = "s3"
```

For explanation and more examples see [examples/](examples/) and [examples/full.toml](examples/full.toml).

## Commands

All commands require `--config <CONFIG_FILE>`.

### `status`

Shows the local snapshot range and the archived range for each matching volume.

```sh
$ bbkar --config examples/local.toml status --name 1
bbkar status
[sync.1]
  volume: /media/rootdisk/_btrbk_snap/@rootfs.* -> /media/sata-256g/bbkar-temp/@rootfs.*
    3 local snapshots: 20230101 - 20230103
    remote range (see `bbkar ls` for details): 20230101 - 20230102
    remote usage: 2, 0 bytes (full: 2, 0 bytes; incremental: 0, 0 bytes)
```

### `ls`

Lists each snapshot/archive name and whether it only exists locally, only exists remotely, or is already synced.

```sh
$ bbkar --config examples/local.toml ls --name 1
bbkar ls
[sync.1]
  volume: /media/rootdisk/_btrbk_snap/@rootfs.* -> /media/sata-256g/bbkar-temp/@rootfs.*
    snapshot         state
    root.20230101    synced
    root.20230102    local-only
    root.20230103    remote-only
    root.20230104    synced
```

With `--restore-root`, `ls` compares archived snapshots with restored subvolumes:

```sh
$ bbkar --config examples/local.toml ls --name 1 --restore-root /mnt/restore
bbkar ls
[sync.1]
  volume: /media/rootdisk/_btrbk_snap/root.* -> /media/sata-256g/bbkar-temp/root.*
    snapshot         state
    root.20230101    restored
    root.20230102    not-restored
```

### `dryrun`

Prints the plan without writing archives. Add `--measure-send` to estimate compressed and raw sizes.

```sh
$ bbkar --config examples/local.toml dryrun --name 1 --measure-send
bbkar dryrun
[sync.1]
  volume: /media/rootdisk/_btrbk_snap/@rootfs.* -> /media/sata-256g/bbkar-temp/@rootfs.*
    would send incremental: 20230102 (parent: 20230101) (512 bytes compressed, 1.00 KiB raw)
    would send incremental: 20230103 (parent: 20230102) (512 bytes compressed, 1.00 KiB raw)
  sync would send: 2, 1.00 KiB (full: 0, 0 bytes; incremental: 2, 1.00 KiB)
  sync remote usage: 3, 1.00 KiB (full: 1, 0 bytes; incremental: 2, 1.00 KiB)
```

### `run`

Exports missing snapshots and writes archive metadata to the configured destination.

```sh
$ bbkar --config examples/local.toml run --name 1
bbkar run
[sync.1]
  volume: /media/rootdisk/_btrbk_snap/@rootfs.* -> /media/sata-256g/bbkar-temp/@rootfs.*
    sending full: 20230101
    done: 20230101 (sent 1.00 KiB compressed, 2.00 KiB raw)
    sending incremental: 20230102 (parent: 20230101)
    done: 20230102 (sent 1.00 KiB compressed, 2.00 KiB raw)
  sync sent: 2, 2.00 KiB (full: 1, 1.00 KiB; incremental: 1, 1.00 KiB)
  sync remote usage: 2, 2.00 KiB (full: 1, 1.00 KiB; incremental: 1, 1.00 KiB)
```

When a volume is already current:

```sh
$ bbkar --config examples/local.toml run --name 1
bbkar run
[sync.1]
  volume: /media/rootdisk/_btrbk_snap/@rootfs.* -> /media/sata-256g/bbkar-temp/@rootfs.*
    (up to date)
  sync sent: 0, 0 bytes (full: 0, 0 bytes; incremental: 0, 0 bytes)
  sync remote usage: 2, 2.00 KiB (full: 1, 1.00 KiB; incremental: 1, 1.00 KiB)
```

### `restore`

Replays archived full and incremental sends into a local btrfs receive root.

Restore a specific snapshot (and its dependencies):

```sh
$ bbkar --config examples/local.toml restore --name 1 --root /mnt/restore --volume root --snapshot 20230103
bbkar restore
  source: /media/sata-256g/bbkar-temp/root
  target: /mnt/restore
  target timestamp: 20230103
  restore chain: 3 step(s)
    20230101 (full)
    20230102 (incremental, parent: 20230101)
    20230103 (incremental, parent: 20230102)
  receiving full 20230101...
  done: 20230101
  receiving incremental 20230102...
  done: 20230102
  receiving incremental 20230103...
  done: 20230103
restore complete
```

Restore multiple snapshots at once:

```sh
$ bbkar --config examples/local.toml restore --name 1 --root /mnt/restore --volume root \
    --snapshot 20230102 --snapshot 20230104
```

Restore all snapshots in a timestamp range:

```sh
$ bbkar --config examples/local.toml restore --name 1 --root /mnt/restore --volume root \
    --min-timestamp 20230101 --max-timestamp 20230115
```

### `dry-restore`

Like `restore`, but only prints what would be done without actually receiving any data.

```sh
$ bbkar --config examples/local.toml dry-restore --name 1 --root /mnt/restore --volume root --snapshot 20230103
bbkar dryrestore
  source: /media/sata-256g/bbkar-temp/root
  target: /mnt/restore
  target timestamp: 20230103
  restore chain: 3 step(s)
    20230101 (full)
    20230102 (incremental, parent: 20230101)
    20230103 (incremental, parent: 20230102)
  would receive full 20230101
  would receive incremental 20230102
  would receive incremental 20230103
dryrestore complete
```

## Other works

The idea is inspired by [this comment](https://github.com/digint/btrbk/issues/123#issuecomment-1114320750), [btrfs-send-to-s3](https://github.com/kubrickfr/btrfs-send-to-s3) and others.

## License

MIT
