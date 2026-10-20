// SPDX-License-Identifier: MPL-2.0
// Browser mirror of src/ve/{graph,lattice}.rs; tested against the Rust engine.
export const DEFAULT_SEED = 0x4e455853;
export function stir(value) {
  let x = value >>> 0;
  x ^= x >>> 16; x = Math.imul(x, 0x21f0aaad);
  x ^= x >>> 15; x = Math.imul(x, 0x735a2d97);
  return (x ^ (x >>> 15)) >>> 0;
}
export function derive() {
  const vertices = [], edges = [], triangles = [], squares = [];
  for (const x of [-1, 0, 1]) for (const y of [-1, 0, 1]) for (const z of [-1, 0, 1]) {
    if ([x, y, z].filter(v => v === 0).length === 1) vertices.push([x, y, z]);
  }
  const adjacent = (a, b) => vertices[a].reduce((s, v, i) => s + (v - vertices[b][i]) ** 2, 0) === 2;
  for (let a = 0; a < 12; a++) for (let b = a + 1; b < 12; b++) {
    if (adjacent(a, b)) edges.push([a, b]);
    for (let c = b + 1; c < 12; c++) if (adjacent(a, b) && adjacent(b, c) && adjacent(a, c)) triangles.push([a, b, c]);
  }
  for (let axis = 0; axis < 3; axis++) for (const sign of [-1, 1]) {
    const face = vertices.map((v, i) => v[axis] === sign ? i : -1).filter(i => i >= 0);
    const cycle = [face[0]];
    while (cycle.length < 4) cycle.push(face.find(i => !cycle.includes(i) && adjacent(cycle.at(-1), i)));
    squares.push(cycle);
  }
  const used = Array.from({ length: 12 }, () => [false, false, false, false]);
  const matching = Array(24).fill(-1);
  function colour(index) {
    if (index === edges.length) return true;
    const [a, b] = edges[index];
    for (let c = 0; c < 4; c++) {
      if (used[a][c] || used[b][c]) continue;
      used[a][c] = used[b][c] = true; matching[index] = c;
      if (colour(index + 1)) return true;
      used[a][c] = used[b][c] = false;
    }
    return false;
  }
  if (!colour(0)) throw new Error('No perfect matching partition');
  const controlFace = edges.map(([a, b]) => triangles.findIndex(t => t.includes(a) && t.includes(b)));
  return { vertices, edges, triangles, squares, matching, controlFace };
}
export const graph = derive();
export function plan(seed, round) {
  const h = stir(((seed << 7) | (seed >>> 25)) ^ Math.imul(round, 0x2545f491) ^ 0x734c4154);
  return { matching: h & 3, flip: Boolean((h >>> 2) & 1), face: (h >>> 3) & 7 };
}
export function seedRegister(seed, cell, vertex) {
  return stir((stir(seed ^ 0x6e657875) + Math.imul(cell, 0x9e3779b1) + Math.imul(vertex, 0x7f4a7c15)) >>> 0) & 3;
}
export function partner(cells, round, cell) {
  return ((cell + (round & 1)) % 2 === 0) ? (cell + 1) % cells : (cell === 0 ? cells - 1 : cell - 1);
}
export class Lattice {
  constructor(cells = 6, seed = DEFAULT_SEED) {
    if (!Number.isInteger(cells) || cells < 2 || cells % 2 || cells > 4096) throw new RangeError('Browser cell count must be even, 2–4096');
    if (!Number.isInteger(seed) || seed < 0 || seed > 0xffffffff) throw new RangeError('Seed must be a uint32');
    this.cells = cells; this.seed = seed; this.round = 0;
    this.registers = Uint8Array.from({ length: cells * 12 }, (_, i) => seedRegister(seed, Math.floor(i / 12), i % 12));
  }
  local(round, inverse) {
    const p = plan(this.seed, round);
    for (let cell = 0; cell < this.cells; cell++) graph.edges.forEach(([a, b], edge) => {
      if (graph.matching[edge] !== p.matching) return;
      const [ctrl, tgt] = p.flip ? [b, a] : [a, b];
      const word = stir((this.seed + Math.imul(round, 0x27d4eb2f) + Math.imul(cell, 0x165667b1) + Math.imul(graph.controlFace[edge], 0x9e3779b1)) >>> 0) % 3;
      const delta = 1 + ((this.registers[cell * 12 + ctrl] + word + edge + cell) >>> 0) % 3;
      const index = cell * 12 + tgt;
      this.registers[index] = (this.registers[index] + (inverse ? 4 - delta : delta)) & 3;
    });
  }
  couple(round) {
    const face = graph.triangles[plan(this.seed, round).face];
    for (let pair = 0; pair < this.cells / 2; pair++) {
      const left = (2 * pair + (round & 1)) % this.cells, right = (left + 1) % this.cells;
      for (const v of face) {
        const a = left * 12 + v, b = right * 12 + v;
        [this.registers[a], this.registers[b]] = [this.registers[b], this.registers[a]];
      }
    }
  }
  forward() { this.local(this.round, false); this.couple(this.round); this.round++; }
  backward() { if (!this.round) return; this.round--; this.couple(this.round); this.local(this.round, true); }
  seek(round) {
    if (!Number.isInteger(round) || round < 0 || round > 128) throw new RangeError('Round must be 0–128');
    while (this.round < round) this.forward();
    while (this.round > round) this.backward();
  }
  mismatches() { return this.registers.reduce((n, value, i) => n + (value !== seedRegister(this.seed, Math.floor(i / 12), i % 12)), 0); }
}
