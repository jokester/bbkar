# Design: Incremental Backup & Retention

## Background

btrbk snapshots are btrfs subvolumes that share internal data structures (CoW). Locally, multiple snapshots are cheap. But when sent to a remote dest, each snapshot must be serialized via one of:

- **Full send** (`btrfs send SUBVOLUME`): Self-contained, no dependencies, but large.
- **Incremental send** (`btrfs send -p PARENT SUBVOLUME`): Only the delta from a parent snapshot. Much smaller, but the parent must exist on the dest to restore.

## The Dependency Chain Problem

Incremental backups form a chain:

```
A (full) <- B (incr from A) <- C (incr from B) <- D (incr from C)
```

To restore D, the dest must have A, B, C, and D. Deleting B makes C and D unrestorable.

This creates a tension between:

- **Space efficiency**: Long incremental chains save storage and bandwidth.
- **Restore cost**: Shorter chains are faster to restore and have fewer failure points.
- **Pruning flexibility**: Intermediate archives in a chain cannot be deleted without breaking dependents.
- **File integrity risk**: Every link in a chain is a single point of failure. If any archive in the chain is corrupted (bit rot, storage errors, incomplete writes), all downstream snapshots become unrestorable. A chain of depth N has N times the exposure to corruption compared to a full send. Shorter chains and periodic full sends limit the blast radius of a single corrupted file.

## Two Separable Concerns

### 1. Sending Policy

When a new snapshot is backed up, should it be full or incremental?

**Default**: Incremental if a suitable parent exists on the dest, full otherwise.

Controls to layer on top:

- **`min_full_send_interval`** (duration, e.g. `"1m"`): The minimum interval between full sends -- full sends happen at least this often. Aligns breakpoints with calendar boundaries, pairing well with time-based retention. Default: derived from `archive_preserve` (e.g. if preserving monthlies, defaults to `"1m"`).
- **`max_incremental_depth`** (count, e.g. `30`): The maximum number of incrementals stacked on top of a single full. Bounds worst-case restore cost and prevents runaway chains when snapshot frequency is high. Default: a safe value like `30`.

These two can coexist -- whichever triggers first wins.

### 2. Retention Policy

Which existing archives should be kept on the dest?

We adopt btrbk's retention model since our users are likely btrbk users too. The familiar two-tier approach:

- **`archive_preserve_min`** (duration or `all`): Keep **all** archives newer than this duration. Acts as a safety floor -- nothing younger gets thinned regardless of the schedule. Default: `all` (keep everything, same as btrbk's default for `preserve_min`).
- **`archive_preserve`** (schedule string): Schedule-based thinning for archives older than `preserve_min`. Format: `[<daily>d] [<weekly>w] [<monthly>m] [<yearly>y]`, matching btrbk's syntax. Use `*` for "keep all" and `0` for "keep none" at each granularity.

Within each time bucket, bbkar keeps the **oldest** snapshot in that period (the "first" of each day/week/month/year), consistent with btrbk's behavior.

Grouping controls:

- **`preserve_day_of_week`** (default: `sunday`): Which day starts a "week" for weekly retention.

`preserve_min` takes precedence over `preserve` -- if `preserve_min` is `all`, the schedule has no effect.

The complication is that retention interacts with chains -- you cannot freely delete an archive in the middle of a chain.

## Handling the Chain Constraint: Chain-Aware Retention

1. Apply retention rules to determine the **wanted set** of snapshots.
2. Compute the **transitive dependency closure** -- every archive needed to support a wanted archive is also kept.
3. Everything outside this closure is prunable.

Users specify what they want to keep; the system figures out what's structurally required. This can result in keeping "unwanted" intermediate archives as structural overhead, but correctness is preserved.

## The Invariant

**Never delete an archive that is in the transitive parent chain of a retained archive.**

This is the single correctness invariant. All policy is built on top of it.

## Planner Phases

The planner produces a plan in phases, all pure functions of config + state (IO-free):

```
Phase 1 (send policy):   source snapshots + dest state + config
                          -> decide full vs incremental for each new snapshot

Phase 2 (retain policy):  dest archives + retention config
                          -> compute wanted set
                          -> compute dependency closure
                          -> mark remaining as prunable

Phase 3 (plan output):    SendFull / SendIncremental steps
                          + PruneArchive steps
```

## Config Sketch

```toml
[sync.1]
source = "data-root"
dest = "local-dir"

# sending policy
min_full_send_interval = "1m"       # at least one full send per month
max_incremental_depth = 30          # at most 30 incrementals after a full

# retention policy (btrbk-style)
archive_preserve_min = "7d"         # keep all archives for 7 days
archive_preserve = "30d 12w 6m *y"  # then thin: daily/30d, weekly/12w, monthly/6m, all yearly
preserve_day_of_week = "sunday"     # which day starts a week for weekly retention
```
