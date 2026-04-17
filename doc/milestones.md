# milestones

## v0.1.0: MVP (CURRENT)

- [x] code builds and runs
- [x] stable metadata, config format, backup file conventions
- [x] compression
- [x] support for local / S3 / GCS storage
- [ ] configurable incremental send policy
- [ ] configurable retention
- [ ] retention UX
  - [ ] `status` explains the retention plan
  - [ ] `ls` shows whether each archive would be pruned on the next `run`
- commands:
  - [ ] `status`
  - [ ] `ls`
  - [x] `run`
  - [x] `dryrun`
  - [x] `restore`
  - [ ] `prune`

- [ ] keep sudo session alive during long runs without re-prompting

### plan

1. Add retention/prune plan types to the model layer.
   - Add data structures for keep vs prune decisions and their reasons in `src/model/plan.rs`.
   - Keep retention planning separate from send planning so `status`, `ls`, `run`, and `prune` can share one source of truth.

2. Implement prune planning in `service::planner`.
   - Compute keep/prune decisions from destination archives plus resolved retention policy.
   - Preserve restore safety: if an incremental is kept, all required parents must also be kept.
   - Return explicit reasons such as policy-preserved, too-new, required ancestor, or prune candidate.

3. Extend `status` to explain the retention plan.
   - Print the effective retention policy for each sync.
   - Summarize how many archives would be kept, pruned, or retained only as dependencies.
   - Reuse the planner output rather than recomputing CLI-side.

4. Extend `ls` with prune visibility.
   - Add a prune-status column in normal listing mode.
   - Show whether each archived snapshot would be kept or pruned on the next `run`.
   - Distinguish ordinary keeps from snapshots retained only because newer incrementals depend on them.

5. Add `bbkar prune` as dry-run first.
   - Add the CLI command and print the planned deletions and reasons.
   - Use the same prune plan output already consumed by `status` and `ls`.
   - Add tests for dry-run output before any destructive behavior is introduced.

6. Add real prune execution.
   - Extend the executor with deletion primitives for archive chunks and metadata updates.
   - Delete only the archives selected by the prune planner.
   - Rewrite metadata after successful deletion and print before/after usage summaries.

7. Wire optional pruning into `run`.
   - Finish upload/sync first, then compute the final prune plan from the post-run destination state.
   - Keep send logic and prune logic separate so failures are easier to reason about.
   - Reuse the same planner and executor paths as `bbkar prune`.

8. Implement real sudo keepalive for long runs.
   - Replace the current between-operations refresh with a background keepalive for the whole privileged session.
   - Refresh credentials periodically during long send/receive operations so production runs do not re-prompt.
   - Stop the refresher cleanly when the run exits.

9. Close out release readiness.
   - Add unit tests for retention planning and dependency preservation.
   - Add CLI tests for `status`, `ls`, and `prune` output.
   - Verify end-to-end behavior on local, S3, and GCS example configs.
   - Update docs/examples and then cut `v0.1.0`.

## post-v0.1.0 (NOT NOW)

- [ ] encryption
- [ ] `bbkar scrub`
- [ ] rewrite with async
    - [ ] concurrency IO
