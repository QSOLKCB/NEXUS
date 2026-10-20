// SPDX-License-Identifier: MPL-2.0
import { graph, Lattice, DEFAULT_SEED, plan, partner } from './model.mjs';
import { Viewer } from './view.mjs';
const $ = id => document.getElementById(id);
let model = new Lattice(6, DEFAULT_SEED), selected = 0, mode = 'structure', timer = null, viewer;
$('seed').value = String(DEFAULT_SEED);
for (let i = 0; i < 12; i++) { const option = document.createElement('option'); option.value = String(i); option.textContent = `Vertex ${String(i).padStart(2, '0')}`; $('vertex').append(option); }
function error(message) { $('webgl-error').textContent = message; $('webgl-error').hidden = !message; $('renderer-status').textContent = message ? 'TEXT INSPECTOR AVAILABLE' : 'NATIVE WEBGL'; }
function selectVertex(value) { selected = value; $('vertex').value = String(value); render(); }
try { viewer = new Viewer($('canvas'), $('labels'), selectVertex, error); } catch (e) { $('labels').replaceChildren(); error(e.message); }
function stop() { clearInterval(timer); timer = null; $('play').textContent = 'Play'; $('play').setAttribute('aria-pressed', 'false'); }
function setMode(value) {
  mode = value; for (const name of ['structure', 'lattice']) $(name).setAttribute('aria-pressed', String(name === value));
  if (value === 'lattice') $('matching').value = 'round';
  render();
}
function render(message) {
  const cell = Number($('cell').value), p = plan(model.seed, model.round), values = model.registers.slice(cell * 12, cell * 12 + 12);
  const matching = $('matching').value === 'round' ? p.matching : Number($('matching').value);
  viewer?.draw({ selected, matching, face: mode === 'lattice' ? p.face : -1, faces: $('faces').checked, values, mode });
  $('coordinates').textContent = `(${graph.vertices[selected].join(', ')})`;
  $('neighbours').textContent = graph.edges.filter(e => e.includes(selected)).map(e => e.find(v => v !== selected)).join(' · ');
  $('register').textContent = `${values[selected]} / modulo 4`;
  $('round').value = String(model.round); $('round-value').textContent = `${String(model.round).padStart(3, '0')} / 128`;
  $('back').disabled = model.round === 0; $('forward').disabled = model.round === 128;
  $('plan').textContent = `NEXT: MATCH ${p.matching} / FACE ${p.face}`;
  $('partner').textContent = `CELL ${cell} ↔ ${partner(model.cells, model.round, cell)}`;
  $('scene-caption').textContent = mode === 'lattice' ? `Cell ${cell} / round ${model.round} / node labels = register values` : 'Coordinate-derived geometry / node labels = vertex indices';
  $('status').textContent = message || (model.round === 0 ? '72 registers · seeded state' : `${model.mismatches()} / 72 registers differ from seed`);
}
for (const name of ['structure', 'lattice']) $(name).addEventListener('click', () => setMode(name));
$('vertex').addEventListener('change', () => selectVertex(Number($('vertex').value)));
for (const id of ['cell', 'matching', 'faces']) $(id).addEventListener('change', () => render());
$('reset-view').addEventListener('click', () => { viewer?.reset(); render(); });
$('forward').addEventListener('click', () => { stop(); setMode('lattice'); model.seek(Math.min(128, model.round + 1)); render(); });
$('back').addEventListener('click', () => { stop(); setMode('lattice'); model.backward(); render(); });
$('round').addEventListener('input', event => { const round = Number(event.target.value); stop(); setMode('lattice'); model.seek(round); render(); });
$('play').addEventListener('click', () => {
  if (timer) { stop(); return; }
  setMode('lattice'); if (model.round === 128) model.seek(0);
  $('play').textContent = 'Pause'; $('play').setAttribute('aria-pressed', 'true'); render();
  timer = setInterval(() => { model.forward(); if (model.round >= 128) stop(); render(); }, 450);
});
$('rewind').addEventListener('click', () => {
  stop(); const rounds = model.round; model.seek(0);
  render(`Reversed ${rounds} rounds · ${model.mismatches()} / 72 restoration mismatches`);
});
function applySeed() {
  const text = $('seed').value.trim(), value = Number(text);
  if (!/^\d+$/.test(text) || !Number.isInteger(value) || value < 0 || value > 0xffffffff) {
    $('seed').setAttribute('aria-invalid', 'true'); $('status').textContent = 'Enter an integer seed from 0 to 4294967295.'; return;
  }
  stop(); $('seed').removeAttribute('aria-invalid'); model = new Lattice(6, value); render();
}
$('apply-seed').addEventListener('click', applySeed);
$('seed').addEventListener('keydown', e => { if (e.key === 'Enter') applySeed(); });
document.addEventListener('visibilitychange', () => { if (document.hidden) stop(); });
render();
