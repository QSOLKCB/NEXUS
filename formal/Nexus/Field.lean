-- SPDX-License-Identifier: MPL-2.0
import Std
namespace Nexus.Field

def uniform (cardinality strands ordinal : Nat) := cardinality * ordinal / strands

theorem uniform_lt (l n i : Nat) (hl : 0 < l) (hi : i < n) : uniform l n i < l := by
  apply (Nat.div_lt_iff_lt_mul (by omega : 0 < n)).2
  exact Nat.mul_lt_mul_of_pos_left hi hl

theorem uniform_zero (l n : Nat) : uniform l n 0 = 0 := by simp [uniform]

def golden (l i : Nat) : Nat := ((i * 0x9e3779b97f4a7c15) % 2^64) * l / 2^64

theorem golden_lt (l i : Nat) (hl : 0 < l) : golden l i < l := by
  apply (Nat.div_lt_iff_lt_mul (by decide : 0 < 2^64)).2
  have h := Nat.mul_lt_mul_of_pos_right (Nat.mod_lt (i * 0x9e3779b97f4a7c15) (by decide : 0 < 2^64)) hl
  simpa [Nat.mul_comm] using h

-- Same carry branch as Cursor::advance; both registers are mathematical naturals.
def advance (l n : Nat) (state : Nat × Nat) : Nat × Nat :=
  let r := state.2 + l % n
  if n ≤ r then (state.1 + l / n + 1, r - n) else (state.1 + l / n, r)
def cursor (l n : Nat) : Nat → Nat × Nat
  | 0 => (0, 0)
  | i+1 => advance l n (cursor l n i)

theorem cursor_exact (l n i : Nat) (hn : 0 < n) :
    cursor l n i = (l * i / n, l * i % n) := by
  induction i with
  | zero => simp [cursor]
  | succ i ih =>
    rw [cursor, ih]
    simp only [advance, Nat.mul_add, Nat.mul_one]
    rw [Nat.add_div hn, Nat.add_mod_eq_sub]
    split <;> rename_i h
    · simp [h, Nat.not_lt.mpr h]
    · simp [h, Nat.lt_of_not_ge h]

theorem cursor_agrees (l n i : Nat) (hn : 0 < n) : (cursor l n i).1 = uniform l n i := by
  simp [cursor_exact l n i hn, uniform]

theorem cursor_remainder_lt (l n i : Nat) (hn : 0 < n) : (cursor l n i).2 < n := by
  rw [cursor_exact l n i hn]
  exact Nat.mod_lt _ hn

theorem floor_step (l n i : Nat) (hn : 0 < n) :
    uniform l n (i+1) = uniform l n i + l/n ∨
    uniform l n (i+1) = uniform l n i + l/n + 1 := by
  unfold uniform
  rw [Nat.mul_add, Nat.mul_one, Nat.add_div hn]
  split <;> omega

theorem uniform_strict_step (l n i : Nat) (hn : 0 < n) (hnl : n ≤ l) :
    uniform l n i < uniform l n (i+1) := by
  have hq : 1 ≤ l/n := (Nat.le_div_iff_mul_le hn).2 (by simpa using hnl)
  have h := floor_step l n i hn
  omega

theorem tracer_lt (strands tracers j : Nat) (hs : 0 < strands) (hj : j < tracers) :
    strands * j / tracers < strands := uniform_lt strands tracers j hs hj

-- The u128 product used by uniform_logical cannot overflow for u64 × u32 inputs.
theorem product_fits_u128 (l i : Nat) (hl : l < 2^64) (hi : i < 2^32) : l*i < 2^128 := by
  have h := Nat.mul_lt_mul_of_lt_of_lt hl hi
  have he : (2:Nat)^64 * 2^32 < 2^128 := by decide
  exact Nat.lt_trans h he
theorem cursor_logical_bound (l n i : Nat) (hn : 0 < n) (hi : i ≤ n) :
    (cursor l n i).1 ≤ l := by
  rw [cursor_exact l n i hn]
  apply Nat.div_le_of_le_mul
  simpa [Nat.mul_comm] using Nat.mul_le_mul_left l hi
theorem cursor_remainder_add_fits_u64 (l n i : Nat) (hn : 0 < n) (hw : n < 2^32) :
    (cursor l n i).2 + l % n < 2^64 := by
  have := cursor_remainder_lt l n i hn
  have := Nat.mod_lt l hn
  omega
end Nexus.Field
