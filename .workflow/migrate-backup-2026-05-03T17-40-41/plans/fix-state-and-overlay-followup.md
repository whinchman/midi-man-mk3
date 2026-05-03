# Plan: fix-state-and-overlay-followup

## Overview

Fix two review findings on BUG-014 and BUG-017 in engine/src/state.rs and engine/src/ui_render.rs.

## Step 1: BUG-014 — Split Loop param into loop_in and loop_out slots

Files to modify:
- `engine/src/ui_render.rs` — expand REGULAR_PARAMS from 7 to 8 entries, add loop_out at index 5
- `engine/src/state.rs` — update committed_param_value, clamped_param_value, apply_param_value, ParamSelect/ParamSelectDelta wrap modulus, param_value_string

Changes:
- REGULAR_PARAMS becomes 8 entries: Key(0), Mode(1), Swing(2), Step Size(3), Loop In(4), Loop Out(5), Pause(6), Stop/Start(7)
- committed_param_value(5) => loop_out as i64
- committed_param_value(6) => paused as i64
- committed_param_value(7) => playing as i64
- clamped_param_value: index 4 and 5 => clamp(0,15); 6|7 => clamp(0,1)
- apply_param_value: index 5 => loop_out, 6 => paused, 7 => playing
- ParamSelect: n.min(7)
- ParamSelectDelta: rem_euclid(8)

## Step 2: BUG-017 — Clear paused when apply_param_value sets playing=true

Files to modify:
- `engine/src/state.rs` — in apply_param_value index 7 arm, add `if self.playing { self.paused = false; }`

## Step 3: Tests

Add tests in engine/src/state.rs:
- loop_out edit path: confirm param index 5 with a value, assert loop_out changes
- BUG-017: set paused=true, confirm param index 7 with value 1, assert playing==true && paused==false
