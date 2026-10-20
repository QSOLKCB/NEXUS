-- SPDX-License-Identifier: MPL-2.0
import Nexus
open Nexus

def emit (tag : String) (xs : List Nat) : IO Unit :=
  IO.println (tag ++ "," ++ String.intercalate "," (xs.map toString))
def emitInt (tag : String) (xs : List Int) : IO Unit :=
  IO.println (tag ++ "," ++ String.intercalate "," (xs.map toString))
def flattenState (s : Lattice.State n) : List Nat :=
  (List.finRange n).flatMap fun c => (List.finRange 12).map fun v => (s c v).val

def main : IO Unit := do
  for v in List.finRange 12 do
    let p := Graph.vertex v
    emitInt "vertex" [v.val,p.1,p.2.1,p.2.2]
  for e in List.finRange 24 do
    let p := Graph.endpoints e
    emit "edge" [e.val,p.1,p.2,Graph.colours[e.val]!,Graph.owners[e.val]!]
  for f in List.finRange 8 do
    let t := Graph.triangles[f.val]!
    emit "triangle" [f.val,t.1,t.2.1,t.2.2]
  for l in [1,10,97,1000003,2^32,2^64-1] do
    for n in [1,3,97,777] do
      if n ≤ l then
        for i in [0,n/2,n-1] do
          emit "field" [l,n,i,Field.uniform l n i,Field.golden l i,(Field.cursor l n i).1]
  for l in List.finRange 3 do
    for f in List.finRange 3 do
      for q in List.finRange 4 do
        for out in [false,true] do
          for r in [0,1,5,12,16,128,65536,2^32-1] do
            let d := Atlas.walk ⟨f,q⟩ l out r
            emit "atlas" [f.val,q.val,l.val,if out then 1 else 0,r,d.family.val,d.quadrant.val]
  for n in [2,6,10,2^32-2] do
    for r in [0,1,2,65535] do
      for c in [0,1,n-2,n-1] do
        emit "partner" [n,r,c,Lattice.partner n r c]
  for seed in [0,7,0x4e455853,2^32-1] do
    for r in [0,1,7,128,65535,2^32-1] do
      let p := Runtime.plan 2 (UInt32.ofNat seed) r
      emit "plan" [seed,r,p.colour.val,if p.flip then 1 else 0,p.face.val]
      for c in [0,1,2^32-2] do
        for f in [0,7] do
          emit "control" [seed,r,c,f,(Runtime.control (UInt32.ofNat seed) (UInt32.ofNat r)
            (UInt32.ofNat c) (UInt32.ofNat f)).toNat]
    for c in [0,1,2^32-2] do
      for v in List.finRange 12 do
        emit "seed" [seed,c,v.val,(Runtime.seedRegister (UInt32.ofNat seed) (UInt32.ofNat c) (UInt32.ofNat v.val)).val]
    for n in [2,6,10] do
      if hn : 2 ≤ n then
        let mut s := Runtime.snapshot (Runtime.seeded n (UInt32.ofNat seed))
        for r in List.range 129 do
          if [0,1,2,3,7,16,32,64,128].contains r then
            emit "state" ([seed,n,r] ++ flattenState (Runtime.view s))
            let mut restored := s
            for k in (List.range r).reverse do
              restored := Runtime.snapshot (Lattice.inverse n hn k (Runtime.plan n (UInt32.ofNat seed) k) (Runtime.view restored))
            emit "restored" ([seed,n,r] ++ flattenState (Runtime.view restored))
          s := Runtime.snapshot (Lattice.forward n hn r (Runtime.plan n (UInt32.ofNat seed) r) (Runtime.view s))
  for a in List.finRange 4 do
    for w in [0,1,2,2^32-1] do
      for e in [0,23] do
        for c in [0,2^32-2] do
          emit "increment" [a.val,w,e,c,(Runtime.increment a (UInt32.ofNat w) (UInt32.ofNat e) (UInt32.ofNat c)).val]
  for bits in [0,1,0x8000000000000000,0x3fe0000000000000,0x7ff0000000000000,0x7ff8000000000001,2^64-1] do
    for t in [255,0,1] do
      for l in [0,1,2] do
        for o in [0,1] do
          emit "record" ([bits,t,l,o] ++ Contracts.record t l o (2^32-1) (2^64-1) bits bits bits)
