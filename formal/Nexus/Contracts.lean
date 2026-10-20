-- SPDX-License-Identifier: MPL-2.0
import Std
namespace Nexus.Contracts

-- Exact algebraic skeleton of the radial factor. Direction components are
-- arbitrary integers here; this does not model libm or IEEE-754 rounding.
def signedNumerator (p d : Int) : Int := 2*p-d
def radial (s : Int) (direction : Int × Int) : Int × Int :=
  (s * direction.1, s * direction.2)
theorem signed_centre (d : Int) : signedNumerator d (2*d) = 0 := by
  simp [signedNumerator]
theorem signed_range (p d : Int) (hp : 0 ≤ p) (hd : p ≤ d) :
    -d ≤ signedNumerator p d ∧ signedNumerator p d ≤ d := by
  unfold signedNumerator
  omega
theorem nexus_exact (direction : Int × Int) : radial 0 direction = (0,0) := by simp [radial]
theorem endpoints_antipodal (direction : Int × Int) :
    radial (-1) direction = (-(radial 1 direction).1, -(radial 1 direction).2) := by simp [radial]

def lane (ordinal : Nat) := ordinal % 3
theorem lane_bound (i : Nat) : lane i < 3 := Nat.mod_lt _ (by decide)
theorem lane_period (i : Nat) : lane (i+3) = lane i := by simp [lane]

-- Indexed scatter/gather specification: concatenating successful mapped
-- blocks in input order equals one sequential map, for any evaluator.
theorem ordered_blocks (f : α → β) (blocks : List (List α)) :
    (blocks.map (List.map f)).flatten = blocks.flatten.map f := by
  simp

def effectiveWorkers (jobs requested : Nat) : Nat := max 1 (min requested (min 256 jobs))
theorem workers_bound (jobs requested : Nat) :
    1 ≤ effectiveWorkers jobs requested ∧ effectiveWorkers jobs requested ≤ 256 := by
  unfold effectiveWorkers
  omega

-- Ideal fixed-denominator gain interpolation: floor/scale +
-- (1-floor/scale)*(radius/scale). IEEE-754 evaluation is a separate obligation.
def gainNumerator (floor scale radius : Nat) := floor*scale + (scale-floor)*radius
theorem gain_envelope (floor scale radius : Nat) (hf : floor ≤ scale) (hr : radius ≤ scale) :
    floor*scale ≤ gainNumerator floor scale radius ∧ gainNumerator floor scale radius ≤ scale*scale := by
  constructor
  · unfold gainNumerator; omega
  · have h := Nat.mul_le_mul_left (scale-floor) hr
    have e : floor + (scale-floor) = scale := by omega
    calc
      gainNumerator floor scale radius ≤ floor*scale + (scale-floor)*scale := Nat.add_le_add_left h _
      _ = (floor + (scale-floor))*scale := (Nat.add_mul _ _ _).symm
      _ = scale*scale := by rw [e]

-- Max reduction over finite nonnegative measurements is independent of
-- grouping/order; nonfinite floating-point inputs must first be rejected.
theorem audit_max_assoc (a b c : Nat) : max (max a b) c = max a (max b c) := Nat.max_assoc _ _ _
theorem audit_max_comm (a b : Nat) : max a b = max b a := Nat.max_comm _ _

-- Byte-level contracts treat f64 fields as their opaque 64-bit encodings.
def littleEndian : Nat → Nat → List Nat
  | 0, _ => []
  | n+1, x => x % 256 :: littleEndian n (x / 256)
def decodeLE : List Nat → Nat
  | [] => 0
  | b::bs => b + 256 * decodeLE bs

theorem littleEndian_length (n x : Nat) : (littleEndian n x).length = n := by
  induction n generalizing x with
  | zero => rfl
  | succ n ih => simp [littleEndian, ih]
theorem littleEndian_bytes (n x : Nat) : ∀ b ∈ littleEndian n x, b < 256 := by
  induction n generalizing x with
  | zero => simp [littleEndian]
  | succ n ih =>
    intro b hb
    simp only [littleEndian, List.mem_cons] at hb
    cases hb with
    | inl h => subst b; exact Nat.mod_lt _ (by decide)
    | inr h => exact ih _ b h

theorem littleEndian_roundtrip (n x : Nat) (hx : x < 256^n) : decodeLE (littleEndian n x) = x := by
  induction n generalizing x with
  | zero =>
    have : x = 0 := by simpa using hx
    simp [littleEndian, decodeLE, this]
  | succ n ih =>
    have hq : x/256 < 256^n := (Nat.div_lt_iff_lt_mul (by decide)).2 (by simpa [Nat.pow_succ] using hx)
    simp only [littleEndian, decodeLE, ih _ hq]
    exact Nat.mod_add_div x 256

def record (trit lane ordering ordinal logical progressBits xBits yBits : Nat) : List Nat :=
  [0x4e, trit, lane, ordering] ++ littleEndian 4 ordinal ++ littleEndian 8 logical ++
  littleEndian 8 progressBits ++ littleEndian 8 xBits ++ littleEndian 8 yBits

theorem record_length (t l o i j p x y : Nat) : (record t l o i j p x y).length = 40 := by
  simp [record, littleEndian_length]
theorem encoded_bytes_safe (count : Nat) (h : count ≤ (2^64-1)/40) : count*40 < 2^64 := by
  omega

-- Lane partition conservation in the mathematical ledger model.
def countLanes (xs : List Nat) : Nat × Nat × Nat :=
  ((xs.filter (fun i => lane i == 0)).length,
   (xs.filter (fun i => lane i == 1)).length,
   (xs.filter (fun i => lane i == 2)).length)
theorem lane_count_conservation (xs : List Nat) :
    (countLanes xs).1 + (countLanes xs).2.1 + (countLanes xs).2.2 = xs.length := by
  induction xs with
  | nil => rfl
  | cons i xs ih =>
    have h := lane_bound i
    have cases : lane i = 0 ∨ lane i = 1 ∨ lane i = 2 := by omega
    rcases cases with h | h | h <;> simp_all [countLanes, h] <;> omega

-- Workload/readback arithmetic for accepted GPU counts; this proves sizing,
-- not Vulkan execution, GPU atomics, timing, or driver correctness.
theorem gpu_census_fits (cells : Nat) (h : cells ≤ (2^32-1)/24) : 24*cells < 2^32 := by omega
theorem gpu_readback_sizes : 65*4*2 = 520 ∧ 65*4*3 = (780:Nat) := by decide
theorem workload_capacity (cells rounds : Nat) (hc : cells < 2^32) (hr : rounds ≤ 65536) :
    6*cells*rounds < 2^64 := by
  have h := Nat.mul_le_mul (Nat.mul_le_mul_left 6 (Nat.le_of_lt hc)) hr
  have hbound : 6*(2:Nat)^32*65536 < 2^64 := by decide
  exact Nat.lt_of_le_of_lt h hbound
end Nexus.Contracts
