-- SPDX-License-Identifier: MPL-2.0
import Std
namespace Nexus.Graph
set_option maxRecDepth 4096
set_option maxHeartbeats 4000000
abbrev Vertex := Fin 12
abbrev Edge := Fin 24
abbrev Colour := Fin 4
abbrev Face := Fin 8
def vertices : Array (Int × Int × Int) := #[(-1, -1, 0), (-1, 0, -1), (-1, 0, 1), (-1, 1, 0), (0, -1, -1), (0, -1, 1), (0, 1, -1), (0, 1, 1), (1, -1, 0), (1, 0, -1), (1, 0, 1), (1, 1, 0)]
def edges : Array (Nat × Nat) := #[(0, 1), (0, 2), (0, 4), (0, 5), (1, 3), (1, 4), (1, 6), (2, 3), (2, 5), (2, 7), (3, 6), (3, 7), (4, 8), (4, 9), (5, 8), (5, 10), (6, 9), (6, 11), (7, 10), (7, 11), (8, 9), (8, 10), (9, 11), (10, 11)]
def triangles : Array (Nat × Nat × Nat) := #[(0, 1, 4), (0, 2, 5), (1, 3, 6), (2, 3, 7), (4, 8, 9), (5, 8, 10), (6, 9, 11), (7, 10, 11)]
def colours : Array Nat := #[0, 1, 2, 3, 1, 3, 2, 0, 2, 3, 3, 2, 0, 1, 1, 0, 0, 1, 1, 0, 2, 3, 3, 2]
def owners : Array Nat := #[0, 1, 0, 1, 2, 0, 2, 3, 1, 3, 2, 3, 4, 4, 5, 5, 6, 6, 7, 7, 4, 5, 6, 7]
-- Finite certificate: each vertex selects its incident edge in each colour.
def incidentEdges : Array (Array Nat) := #[#[0, 0, 7, 7, 12, 15, 16, 19, 12, 16, 15, 19], #[1, 4, 1, 4, 13, 14, 17, 18, 14, 13, 18, 17], #[2, 6, 8, 11, 2, 8, 6, 11, 20, 20, 23, 23], #[3, 5, 9, 10, 5, 3, 10, 9, 21, 22, 21, 22]]
def vertex (v : Vertex) : Int × Int × Int := vertices[v.val]!
def endpoints (e : Edge) : Nat × Nat := edges[e.val]!
def edgeAt (c : Colour) (v : Vertex) : Edge :=
  ⟨incidentEdges[c.val]![v.val]! % 24, Nat.mod_lt _ (by decide)⟩
def mate (c : Colour) (v : Vertex) : Vertex :=
  let e := endpoints (edgeAt c v)
  ⟨(if e.1 == v.val then e.2 else e.1) % 12, Nat.mod_lt _ (by decide)⟩
def target (c : Colour) (flip : Bool) (v : Vertex) : Bool :=
  let e := endpoints (edgeAt c v)
  v.val == (if flip then e.1 else e.2)
def onFace (f : Face) (v : Vertex) : Bool :=
  let t := triangles[f.val]!
  v.val == t.1 || v.val == t.2.1 || v.val == t.2.2
def distance2 (a b : Vertex) : Int :=
  let p := vertex a
  let q := vertex b
  (p.1-q.1)^2 + (p.2.1-q.2.1)^2 + (p.2.2-q.2.2)^2
def adjacent (a b : Vertex) : Bool :=
  edges.any (fun e => (e.1 == a.val && e.2 == b.val) || (e.1 == b.val && e.2 == a.val))

def derivedVertices : List (Int × Int × Int) :=
  ([-1, 0, 1] : List Int).flatMap fun a =>
    ([-1, 0, 1] : List Int).flatMap fun b =>
      ([-1, 0, 1] : List Int).filterMap fun c =>
        if ([a,b,c].filter (fun x => x == 0)).length == 1 then some (a,b,c) else none
theorem vertices_derived : vertices.toList = derivedVertices := by decide
theorem edges_ordered : ∀ e : Edge,
    (endpoints e).1 < (endpoints e).2 ∧ (endpoints e).2 < 12 := by decide
theorem triangles_exact : ∀ a b c : Vertex,
    ((a.val,b.val,c.val) ∈ triangles.toList) ↔
    a.val < b.val ∧ b.val < c.val ∧ adjacent a b = true ∧ adjacent b c = true ∧ adjacent a c = true := by decide

theorem counts : vertices.size = 12 ∧ edges.size = 24 ∧ triangles.size = 8 := by decide
theorem coordinates : ∀ v : Vertex,
    let p := vertex v
    p.1^2 + p.2.1^2 + p.2.2^2 = 2 ∧
    (p.1 = 0 ∨ p.2.1 = 0 ∨ p.2.2 = 0) := by decide
theorem vertices_distinct : ∀ a b : Vertex, vertex a = vertex b → a = b := by decide
theorem adjacency_exact : ∀ a b : Vertex, adjacent a b = true ↔ distance2 a b = 2 := by decide
theorem degree_four : ∀ v : Vertex,
    (edges.toList.filter (fun e => e.1 == v.val || e.2 == v.val)).length = 4 := by decide
theorem matching_six_edges : ∀ c : Colour,
    (colours.toList.filter (fun x => x == c.val)).length = 6 := by decide
theorem matching_incident_unique : ∀ c : Colour, ∀ v : Vertex,
    ((List.finRange 24).filter (fun e => colours[e.val]! == c.val &&
      ((endpoints e).1 == v.val || (endpoints e).2 == v.val))).length = 1 := by decide
theorem edgeAt_incident : ∀ c : Colour, ∀ v : Vertex,
    colours[(edgeAt c v).val]! = c.val ∧
    (endpoints (edgeAt c v)).1 ≠ (endpoints (edgeAt c v)).2 ∧
    ((endpoints (edgeAt c v)).1 = v.val ∨ (endpoints (edgeAt c v)).2 = v.val) := by decide
theorem mate_involution : ∀ c : Colour, ∀ v : Vertex, mate c (mate c v) = v := by decide
theorem mate_different : ∀ c : Colour, ∀ v : Vertex, mate c v ≠ v := by decide
theorem mate_same_edge : ∀ c : Colour, ∀ v : Vertex, edgeAt c (mate c v) = edgeAt c v := by decide
theorem mate_target : ∀ c : Colour, ∀ f : Bool, ∀ v : Vertex,
    target c f (mate c v) = !(target c f v) := by decide
theorem face_has_three : ∀ f : Face,
    ((List.finRange 12).filter (onFace f)).length = 3 := by decide
theorem unique_control_face : ∀ e : Edge,
    let a : Vertex := ⟨(endpoints e).1 % 12, Nat.mod_lt _ (by decide)⟩
    let b : Vertex := ⟨(endpoints e).2 % 12, Nat.mod_lt _ (by decide)⟩
    ((List.finRange 8).filter (fun f => onFace f a && onFace f b)).map Fin.val = [owners[e.val]!] := by decide
end Nexus.Graph
