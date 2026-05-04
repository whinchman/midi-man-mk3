# Bug Tracker

## [OPEN] BUG-018 — SeedSet(0) produces degenerate xorshift64 seed
- **File:** engine/src/state.rs — SeedSet handler
- **Severity:** warning
- **Description:** When `seed = 0`, `rng_seed` becomes 0, which is a fixed
  point for xorshift64. All randomness silently stops working until restart.
- **Fix:** Guard: `self.rng_seed = if lo == 0 { 0x853C_49E6_748F_EA9B } else { lo | (lo << 32) };`
