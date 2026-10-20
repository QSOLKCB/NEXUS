// SPDX-License-Identifier: MPL-2.0
import { graph } from './model.mjs';
const VERTEX = `attribute vec3 position; attribute vec3 color;
uniform float yaw; uniform float pitch; uniform float aspect; uniform float zoom; uniform float pointSize;
varying vec3 tint;
void main(){
 float cy=cos(yaw), sy=sin(yaw), cp=cos(pitch), sp=sin(pitch);
 vec3 p=vec3(cy*position.x+sy*position.z, position.y, -sy*position.x+cy*position.z);
 p=vec3(p.x, cp*p.y-sp*p.z, sp*p.y+cp*p.z);
 float depth=5.0-p.z;
 gl_Position=vec4(p.x*zoom/aspect, p.y*zoom, ((20.0+0.1)/(20.0-0.1))*depth-(2.0*20.0*0.1)/(20.0-0.1), depth);
 gl_PointSize=pointSize; tint=color;
}`;
const FRAGMENT = `precision mediump float; varying vec3 tint; uniform bool points;
void main(){ if(points && distance(gl_PointCoord,vec2(0.5))>0.5) discard; gl_FragColor=vec4(tint,1.0); }`;
export class Viewer {
  constructor(canvas, labels, onSelect, onError) {
    this.canvas = canvas; this.labels = labels; this.onError = onError;
    this.yaw = -0.55; this.pitch = 0.35; this.zoom = 2.8; this.drag = null;
    this.nodes = graph.vertices.map((_, i) => {
      const b = document.createElement('button'); b.className = 'vertex';
      b.addEventListener('click', () => onSelect(i)); labels.append(b); return b;
    });
    this.gl = canvas.getContext('webgl', { antialias: true, alpha: false });
    if (!this.gl) throw new Error('WebGL is unavailable. The vertex inspector and lattice controls remain usable.');
    this.init();
    canvas.addEventListener('webglcontextlost', event => { event.preventDefault(); this.lost = true; onError('WebGL context lost. Waiting for the browser to restore it.'); });
    canvas.addEventListener('webglcontextrestored', () => {
      try { this.init(); this.lost = false; onError(''); this.draw(); } catch (e) { onError(e.message); }
    });
    canvas.addEventListener('pointerdown', e => { this.drag = { id: e.pointerId, x: e.clientX, y: e.clientY }; canvas.setPointerCapture(e.pointerId); });
    canvas.addEventListener('pointermove', e => {
      if (!this.drag || this.drag.id !== e.pointerId) return;
      this.yaw += (e.clientX - this.drag.x) * 0.008;
      this.pitch = Math.max(-1.5, Math.min(1.5, this.pitch + (e.clientY - this.drag.y) * 0.008));
      this.drag = { id: e.pointerId, x: e.clientX, y: e.clientY }; this.draw();
    });
    const release = () => { this.drag = null; };
    canvas.addEventListener('pointerup', release); canvas.addEventListener('pointercancel', release); canvas.addEventListener('lostpointercapture', release);
    canvas.addEventListener('wheel', e => { e.preventDefault(); this.zoom = Math.max(1.5, Math.min(4, this.zoom * Math.exp(-e.deltaY * 0.001))); this.draw(); }, { passive: false });
    canvas.addEventListener('keydown', e => {
      if (!['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', '+', '=', '-', 'Home'].includes(e.key)) return;
      e.preventDefault();
      if (e.key === 'ArrowLeft') this.yaw -= 0.12;
      if (e.key === 'ArrowRight') this.yaw += 0.12;
      if (e.key === 'ArrowUp') this.pitch = Math.max(-1.5, this.pitch - 0.12);
      if (e.key === 'ArrowDown') this.pitch = Math.min(1.5, this.pitch + 0.12);
      if (['+', '='].includes(e.key)) this.zoom = Math.min(4, this.zoom + 0.1);
      if (e.key === '-') this.zoom = Math.max(1.5, this.zoom - 0.1);
      if (e.key === 'Home') this.reset();
      this.draw();
    });
    this.observer = new ResizeObserver(() => this.draw()); this.observer.observe(canvas);
  }
  init() {
    const gl = this.gl;
    const shader = (type, code) => {
      const s = gl.createShader(type); gl.shaderSource(s, code); gl.compileShader(s);
      if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s));
      return s;
    };
    this.program = gl.createProgram();
    const vs = shader(gl.VERTEX_SHADER, VERTEX), fs = shader(gl.FRAGMENT_SHADER, FRAGMENT);
    gl.attachShader(this.program, vs); gl.attachShader(this.program, fs); gl.linkProgram(this.program);
    if (!gl.getProgramParameter(this.program, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(this.program));
    gl.deleteShader(vs); gl.deleteShader(fs); gl.useProgram(this.program);
    this.buffer = gl.createBuffer();
    this.uniforms = Object.fromEntries(['yaw', 'pitch', 'aspect', 'zoom', 'pointSize', 'points'].map(n => [n, gl.getUniformLocation(this.program, n)]));
    this.position = gl.getAttribLocation(this.program, 'position'); this.color = gl.getAttribLocation(this.program, 'color');
    gl.clearColor(0.055, 0.063, 0.068, 1); gl.enable(gl.DEPTH_TEST); gl.depthFunc(gl.LEQUAL);
  }
  reset() { this.yaw = -0.55; this.pitch = 0.35; this.zoom = 2.8; }
  project(v, width, height) {
    const cy = Math.cos(this.yaw), sy = Math.sin(this.yaw), cp = Math.cos(this.pitch), sp = Math.sin(this.pitch);
    const x = cy * v[0] + sy * v[2], z = -sy * v[0] + cy * v[2];
    const y = cp * v[1] - sp * z, depth = 5 - (sp * v[1] + cp * z);
    return [width / 2 + x * this.zoom / depth * height / 2, height / 2 - y * this.zoom / depth * height / 2, depth];
  }
  draw(state = this.state) {
    if (state) this.state = state;
    if (!this.state || this.lost) return;
    const gl = this.gl, c = this.canvas, width = c.clientWidth, height = c.clientHeight;
    if (!width || !height) return;
    const dpr = Math.min(devicePixelRatio || 1, 2);
    c.width = Math.round(width * dpr); c.height = Math.round(height * dpr);
    gl.viewport(0, 0, c.width, c.height); gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
    gl.useProgram(this.program);
    for (const [name, value] of Object.entries({ yaw: this.yaw, pitch: this.pitch, aspect: width / height, zoom: this.zoom, pointSize: 8 * dpr })) gl.uniform1f(this.uniforms[name], value);
    const draw = (data, mode, points = false) => {
      gl.bindBuffer(gl.ARRAY_BUFFER, this.buffer); gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(data), gl.DYNAMIC_DRAW);
      gl.enableVertexAttribArray(this.position); gl.vertexAttribPointer(this.position, 3, gl.FLOAT, false, 24, 0);
      gl.enableVertexAttribArray(this.color); gl.vertexAttribPointer(this.color, 3, gl.FLOAT, false, 24, 12);
      gl.uniform1i(this.uniforms.points, points); gl.drawArrays(mode, 0, data.length / 6);
    };
    const { selected, matching, face, faces, values, mode } = this.state;
    const surface = [], edges = [], nodes = [];
    if (faces) [...graph.triangles, ...graph.squares].forEach((f, index) => {
      const colour = index === face ? [0.39, 0.26, 0.105] : index < 8 ? [0.105, 0.12, 0.127] : [0.08, 0.092, 0.10];
      for (let i = 1; i < f.length - 1; i++) for (const v of [f[0], f[i], f[i + 1]]) surface.push(...graph.vertices[v], ...colour);
    });
    gl.enable(gl.POLYGON_OFFSET_FILL); gl.polygonOffset(1, 1); draw(surface, gl.TRIANGLES); gl.disable(gl.POLYGON_OFFSET_FILL);
    graph.edges.forEach(([a, b], i) => {
      const active = matching === graph.matching[i];
      const colour = active ? [1, 0.68, 0.30] : [0.34, 0.38, 0.40];
      for (const v of [a, b]) edges.push(...graph.vertices[v], ...colour);
    });
    draw(edges, gl.LINES);
    graph.vertices.forEach((v, i) => nodes.push(...v, ...(i === selected ? [1, 0.68, 0.30] : [0.8, 0.82, 0.80])));
    draw(nodes, gl.POINTS, true);
    graph.vertices.forEach((v, i) => {
      const [x, y, depth] = this.project(v, width, height), b = this.nodes[i];
      b.style.left = `${x}px`; b.style.top = `${y}px`; b.style.zIndex = String(Math.round(100 - depth * 10));
      b.textContent = mode === 'lattice' ? String(values[i]) : String(i).padStart(2, '0');
      b.setAttribute('aria-label', `Vertex ${i}, coordinates ${v.join(', ')}, register ${values[i]}`);
      b.setAttribute('aria-pressed', String(selected === i));
    });
  }
}
