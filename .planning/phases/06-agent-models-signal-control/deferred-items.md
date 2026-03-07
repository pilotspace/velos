# Deferred Items -- Phase 6

## Pre-existing Issues (Not Caused by Phase 6)

1. **velos-net test compilation failure** -- `crates/velos-net/src/cleaning.rs:322` references `RoadEdge` without importing it. Tests fail with `cannot find struct RoadEdge in this scope`. This is a pre-existing issue unrelated to Phase 6 changes.
