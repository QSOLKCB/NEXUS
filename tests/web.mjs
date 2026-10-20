// SPDX-License-Identifier: MPL-2.0
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { graph, Lattice } from '../site/model.mjs';
const result = spawnSync(process.env.CARGO || 'cargo', ['run', '--quiet', '--locked', '--example', 'web_reference'], { encoding: 'utf8' });
assert.equal(result.status, 0, result.stderr || String(result.error));
const reference = JSON.parse(result.stdout);
for (const field of ['vertices', 'edges', 'triangles', 'matching', 'controlFace']) assert.deepEqual(graph[field], reference[field], field);
assert.equal(graph.squares.length, 6);
for (const square of graph.squares) {
  assert.equal(new Set(square).size, 4);
  square.forEach((a, i) => assert.ok(graph.edges.some(([x, y]) => (a === x && square[(i + 1) % 4] === y) || (a === y && square[(i + 1) % 4] === x))));
}
for (const fixture of reference.cases) {
  const lattice = new Lattice(fixture.cells, fixture.seed);
  const initial = [...lattice.registers];
  lattice.seek(fixture.round);
  assert.deepEqual([...lattice.registers], fixture.registers, JSON.stringify({ ...fixture, registers: undefined }));
  lattice.seek(0);
  assert.deepEqual([...lattice.registers], initial, 'inverse restores exact registers');
}
console.log(`Browser parity: graph + ${reference.cases.length} Rust states and inverse restorations passed.`);
