-- SPDX-License-Identifier: MPL-2.0
import Std
namespace Nexus.Atlas
structure Direction where
  family : Fin 3
  quadrant : Fin 4
  deriving DecidableEq, Repr

def index (d : Direction) : Nat := d.family.val * 4 + d.quadrant.val
theorem index_lt (d : Direction) : index d < 12 := by
  have := d.family.isLt
  have := d.quadrant.isLt
  unfold index
  omega

def walkResidue (d : Direction) (lane : Fin 3) (outbound : Bool) (r : Fin 12) : Direction :=
  let f := (lane.val + 1) % 3
  let df := if outbound then f else (3 - f) % 3
  let dq := if outbound then 1 else 3
  { family := ⟨(d.family.val + df * r.val) % 3, Nat.mod_lt _ (by decide)⟩
    quadrant := ⟨(d.quadrant.val + dq * r.val) % 4, Nat.mod_lt _ (by decide)⟩ }
def walk (d : Direction) (lane : Fin 3) (outbound : Bool) (rounds : Nat) : Direction :=
  walkResidue d lane outbound ⟨rounds % 12, Nat.mod_lt _ (by decide)⟩

theorem residue_inverse : ∀ f : Fin 3, ∀ q : Fin 4, ∀ lane : Fin 3, ∀ out : Bool, ∀ r : Fin 12,
    walkResidue (walkResidue ⟨f,q⟩ lane out r) lane (!out) r = ⟨f,q⟩ := by decide

theorem walk_inverse (d : Direction) (lane : Fin 3) (out : Bool) (rounds : Nat) :
    walk (walk d lane out rounds) lane (!out) rounds = d := by
  exact residue_inverse d.family d.quadrant lane out ⟨rounds % 12, Nat.mod_lt _ (by decide)⟩
theorem period_twelve (d : Direction) (lane : Fin 3) (out : Bool) (r : Nat) :
    walk d lane out (r+12) = walk d lane out r := by simp [walk]
end Nexus.Atlas
