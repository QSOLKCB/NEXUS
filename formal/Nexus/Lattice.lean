-- SPDX-License-Identifier: MPL-2.0
import Nexus.Graph
namespace Nexus.Lattice
open Graph
abbrev Register := Fin 4
abbrev Cell := Vertex → Register

def add4 (a b : Register) : Register := ⟨(a.val + b.val) % 4, Nat.mod_lt _ (by decide)⟩
def sub4 (a b : Register) : Register := ⟨(a.val + 4 - b.val) % 4, Nat.mod_lt _ (by decide)⟩
theorem sub_add4 : ∀ a b : Register, sub4 (add4 a b) b = a := by decide
theorem add_sub4 : ∀ a b : Register, add4 (sub4 a b) b = a := by decide

-- Delta can depend arbitrarily on the unchanged control register.
-- This includes the Rust wrapping increment, for every seed/cell/round.
def localForward (c : Colour) (flip : Bool) (delta : Vertex → Register → Register) (s : Cell) : Cell :=
  fun v => if target c flip v then add4 (s v) (delta v (s (mate c v))) else s v
def localInverse (c : Colour) (flip : Bool) (delta : Vertex → Register → Register) (s : Cell) : Cell :=
  fun v => if target c flip v then sub4 (s v) (delta v (s (mate c v))) else s v

theorem local_inverse_forward (c : Colour) (flip : Bool) (delta : Vertex → Register → Register) (s : Cell) :
    localInverse c flip delta (localForward c flip delta s) = s := by
  funext v
  cases h : target c flip v <;>
    simp [localInverse, localForward, h, mate_target, sub_add4]
theorem local_forward_inverse (c : Colour) (flip : Bool) (delta : Vertex → Register → Register) (s : Cell) :
    localForward c flip delta (localInverse c flip delta s) = s := by
  funext v
  cases h : target c flip v <;>
    simp [localInverse, localForward, h, mate_target, add_sub4]

-- Natural-number translation of partner, with the valid request preconditions.
def partner (n round cell : Nat) : Nat :=
  if (cell + round % 2) % 2 = 0 then (cell + 1) % n
  else if cell = 0 then n - 1 else cell - 1

theorem partner_lt (n r c : Nat) (hn : 2 ≤ n) (hc : c < n) : partner n r c < n := by
  unfold partner
  split
  · exact Nat.mod_lt _ (by omega)
  · split <;> omega

theorem partner_eq (n r c : Nat) (hc : c < n) :
    partner n r c = if (c + r % 2) % 2 = 0 then
      (if c+1 = n then 0 else c+1) else if c = 0 then n-1 else c-1 := by
  unfold partner
  split
  · split <;> rename_i h
    · simp [h]
    · exact Nat.mod_eq_of_lt (by omega)
  · rfl

theorem partner_involution (n r c : Nat) (hn : 2 ≤ n) (he : n % 2 = 0) (hc : c < n) :
    partner n r (partner n r c) = c := by
  have hr := Nat.mod_lt r (by decide : 0 < 2)
  rw [partner_eq n r (partner n r c) (partner_lt n r c hn hc), partner_eq n r c hc]
  repeat' first | split | omega

theorem partner_ne (n r c : Nat) (hn : 2 ≤ n) (he : n % 2 = 0) (hc : c < n) :
    partner n r c ≠ c := by
  have hr := Nat.mod_lt r (by decide : 0 < 2)
  rw [partner_eq n r c hc]
  repeat' first | split | omega

def partnerFin (n : Nat) (hn : 2 ≤ n) (r : Nat) (c : Fin n) : Fin n :=
  ⟨partner n r c.val, partner_lt n r c.val hn c.isLt⟩
theorem partnerFin_involution (n : Nat) (hn : 2 ≤ n) (he : n % 2 = 0) (r : Nat) (c : Fin n) :
    partnerFin n hn r (partnerFin n hn r c) = c := by
  apply Fin.ext
  exact partner_involution n r c.val hn he c.isLt

abbrev State (n : Nat) := Fin n → Cell

def couple (n : Nat) (hn : 2 ≤ n) (r : Nat) (face : Face) (s : State n) : State n :=
  fun c v => if onFace face v then s (partnerFin n hn r c) v else s c v

theorem couple_involution (n : Nat) (hn : 2 ≤ n) (he : n % 2 = 0) (r : Nat) (face : Face) (s : State n) :
    couple n hn r face (couple n hn r face s) = s := by
  funext c v
  cases h : onFace face v <;> simp [couple, h, partnerFin_involution n hn he]

structure Plan (n : Nat) where
  colour : Colour
  flip : Bool
  face : Face
  delta : Fin n → Vertex → Register → Register

def forward (n : Nat) (hn : 2 ≤ n) (r : Nat) (p : Plan n) (s : State n) : State n :=
  couple n hn r p.face (fun c => localForward p.colour p.flip (p.delta c) (s c))
def inverse (n : Nat) (hn : 2 ≤ n) (r : Nat) (p : Plan n) (s : State n) : State n :=
  let uncoupled := couple n hn r p.face s
  fun c => localInverse p.colour p.flip (p.delta c) (uncoupled c)

theorem inverse_forward (n : Nat) (hn : 2 ≤ n) (he : n % 2 = 0) (r : Nat) (p : Plan n) (s : State n) :
    inverse n hn r p (forward n hn r p s) = s := by
  simp only [inverse, forward, couple_involution n hn he]
  funext c
  exact local_inverse_forward _ _ _ _
theorem forward_inverse (n : Nat) (hn : 2 ≤ n) (he : n % 2 = 0) (r : Nat) (p : Plan n) (s : State n) :
    forward n hn r p (inverse n hn r p s) = s := by
  simp only [inverse, forward, local_forward_inverse]
  exact couple_involution n hn he r p.face s

def evolve (n : Nat) (hn : 2 ≤ n) (plans : Nat → Plan n) : Nat → State n → State n
  | 0, s => s
  | k+1, s => forward n hn k (plans k) (evolve n hn plans k s)
def rewind (n : Nat) (hn : 2 ≤ n) (plans : Nat → Plan n) : Nat → State n → State n
  | 0, s => s
  | k+1, s => rewind n hn plans k (inverse n hn k (plans k) s)

theorem rewind_evolve (n : Nat) (hn : 2 ≤ n) (he : n % 2 = 0) (plans : Nat → Plan n) (rounds : Nat) (s : State n) :
    rewind n hn plans rounds (evolve n hn plans rounds s) = s := by
  induction rounds with
  | zero => rfl
  | succ k ih => simp only [rewind, evolve, inverse_forward n hn he, ih]
theorem evolve_rewind (n : Nat) (hn : 2 ≤ n) (he : n % 2 = 0) (plans : Nat → Plan n) (rounds : Nat) (s : State n) :
    evolve n hn plans rounds (rewind n hn plans rounds s) = s := by
  induction rounds generalizing s with
  | zero => rfl
  | succ k ih => simp only [rewind, evolve, ih, forward_inverse n hn he]
theorem evolution_injective (n : Nat) (hn : 2 ≤ n) (he : n % 2 = 0) (plans : Nat → Plan n) (rounds : Nat)
    (a b : State n) (h : evolve n hn plans rounds a = evolve n hn plans rounds b) : a = b := by
  have e := congrArg (rewind n hn plans rounds) h
  simpa only [rewind_evolve n hn he] using e
end Nexus.Lattice
