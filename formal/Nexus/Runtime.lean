-- SPDX-License-Identifier: MPL-2.0
import Nexus.Lattice
namespace Nexus.Runtime
open Graph Lattice

-- Explicit UInt32 operations mirror Rust's wrapping operations. These plan
-- functions are executable specifications; reversibility does not assume
-- mixer injectivity or any statistical property of a seed.
def stir (x : UInt32) : UInt32 :=
  let x := x ^^^ (x >>> 16)
  let x := x * 0x21f0aaad
  let x := x ^^^ (x >>> 15)
  let x := x * 0x735a2d97
  x ^^^ (x >>> 15)
def rotate7 (x : UInt32) : UInt32 := (x <<< 7) ||| (x >>> 25)
def planWord (seed round : UInt32) := stir (rotate7 seed ^^^ (round * 0x2545f491) ^^^ 0x734c4154)
def seedRegister (seed cell vertex : UInt32) : Register :=
  ⟨(stir (stir (seed ^^^ 0x6e657875) + cell * 0x9e3779b1 + vertex * 0x7f4a7c15)).toNat % 4,
    Nat.mod_lt _ (by decide)⟩
def control (seed round cell face : UInt32) : UInt32 :=
  stir (seed + round * 0x27d4eb2f + cell * 0x165667b1 + face * 0x9e3779b1) % 3
def increment (value : Register) (word edge cell : UInt32) : Register :=
  ⟨1 + (UInt32.ofNat value.val + word + edge + cell).toNat % 3, by
    have := Nat.mod_lt (UInt32.ofNat value.val + word + edge + cell).toNat (by decide : 0 < 3)
    omega⟩

def plan (n : Nat) (seed : UInt32) (round : Nat) : Plan n :=
  let h := planWord seed (UInt32.ofNat round)
  let colour : Colour := ⟨h.toNat % 4, Nat.mod_lt _ (by decide)⟩
  { colour := colour
    flip := (h >>> 2).toNat % 2 == 1
    face := ⟨(h >>> 3).toNat % 8, Nat.mod_lt _ (by decide)⟩
    delta := fun cell v value =>
      let e := edgeAt colour v
      increment value (control seed (UInt32.ofNat round) (UInt32.ofNat cell.val)
        (UInt32.ofNat owners[e.val]!)) (UInt32.ofNat e.val) (UInt32.ofNat cell.val) }
def seeded (n : Nat) (seed : UInt32) : State n :=
  fun cell v => seedRegister seed (UInt32.ofNat cell.val) (UInt32.ofNat v.val)

-- A snapshot is stored data, so executable parity runs evaluate each step once.
abbrev Snapshot (n : Nat) := Vector (Vector Register 12) n
def snapshot (s : State n) : Snapshot n := Vector.ofFn (fun c => Vector.ofFn (s c))
def view (s : Snapshot n) : State n := fun c v => (s.get c).get v
theorem view_snapshot (s : State n) : view (snapshot s) = s := by
  funext c v
  simp [view, snapshot, Vector.get]

def run (n : Nat) (hn : 2 ≤ n) (seed : UInt32) : Nat → Snapshot n
  | 0 => snapshot (seeded n seed)
  | k+1 => snapshot (forward n hn k (plan n seed k) (view (run n hn seed k)))
theorem run_eq_evolve (n : Nat) (hn : 2 ≤ n) (seed : UInt32) (rounds : Nat) :
    view (run n hn seed rounds) = evolve n hn (plan n seed) rounds (seeded n seed) := by
  induction rounds with
  | zero => exact view_snapshot _
  | succ k ih => simp only [run, view_snapshot, evolve, ih]
theorem seeded_restoration (n : Nat) (hn : 2 ≤ n) (he : n % 2 = 0) (seed : UInt32) (rounds : Nat) :
    rewind n hn (plan n seed) rounds (view (run n hn seed rounds)) = seeded n seed := by
  rw [run_eq_evolve]
  exact rewind_evolve n hn he (plan n seed) rounds (seeded n seed)
end Nexus.Runtime
