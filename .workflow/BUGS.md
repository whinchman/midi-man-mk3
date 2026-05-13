# Bug Tracker

## [CLOSED] BUG-018 — SeedSet(0) produces degenerate xorshift64 seed
- **File:** engine/src/state.rs:600 — SeedSet handler
- **Severity:** warning
- **Resolution:** Guard already in place since the ui-refactor merge (29f9717,
  PR #93). When `seed == 0` the handler substitutes the non-zero constant
  `0x853C_49E6_853C_49E6u64`, avoiding the xorshift64 zero fixed-point.
  Regression test: `state::tests::seed_set_zero_uses_fallback_nonzero_rng_seed`
  (engine/src/state.rs:925). Verified passing 2026-05-13.
