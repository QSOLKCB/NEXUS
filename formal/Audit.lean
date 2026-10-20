-- SPDX-License-Identifier: MPL-2.0
import Nexus
import Lean
open Lean Elab Command

-- Inspect dependencies of every public Nexus theorem in the checked
-- environment. Only Lean's ordinary foundational axioms are permitted.
run_cmd do
  let env ← getEnv
  let json ← match Json.parse (← IO.FS.readFile "theorems.json") with
    | .ok value => pure value
    | .error error => throwError "invalid theorem inventory: {error}"
  let names ← match (fromJson? json : Except String (Array String)) with
    | .ok value => pure value
    | .error error => throwError "invalid theorem inventory: {error}"
  for text in names do
    match env.find? text.toName with
    | some (.thmInfo _) => pure ()
    | _ => throwError "required theorem missing from compiled library: {text}"
  let mut theorems := 0
  let mut declarations := 0
  for (name, info) in env.constants.toList do
    if (`Nexus).isPrefixOf name then
      declarations := declarations + 1
      if info matches .thmInfo _ then
        theorems := theorems + 1
        for ax in ← collectAxioms name do
          unless #[`propext, `Quot.sound, `Classical.choice].contains ax do
            throwError "{name} depends on forbidden axiom {ax}"
  if names.size < 71 then
    throwError "incomplete named theorem inventory: only {names.size} entries"
  logInfo m!"Checked {names.size} required named theorems. Audited {theorems} theorem declarations and {declarations} total Nexus declarations."
