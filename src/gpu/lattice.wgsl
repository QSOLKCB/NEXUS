// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// GPU mirror of the NEXUS reversible cuboctahedral lattice.
//
// The cuboctahedron tables are NOT written into this shader. They are
// derived on the CPU and uploaded, so the GPU and CPU engines share one
// source of combinatorial truth. Integer arithmetic here is modular, which
// matches the wrapping arithmetic of the Rust reference.

struct Params {
    cells: u32,
    seed: u32,
    round: u32,
    // bit 0: inverse local round; bits 8..: census region index.
    flags: u32,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> state_in: array<u32>;
@group(0) @binding(2) var<storage, read_write> state_out: array<u32>;
@group(0) @binding(3) var<storage, read_write> census: array<atomic<u32>>;
@group(0) @binding(4) var<storage, read> tables: array<u32>;

const VERTICES: u32 = 12u;
const EDGES: u32 = 24u;
const FACES: u32 = 8u;
const REGION_WORDS: u32 = 65u;
const GROUPS_X: u32 = 32768u;
const REGISTER_GROUP: u32 = 64u;

// Table layout, in u32 words: edge_a[24], edge_b[24], matching[24],
// control_face[24], triangles[8 * 3].
fn edge_a(e: u32) -> u32 { return tables[e]; }
fn edge_b(e: u32) -> u32 { return tables[EDGES + e]; }
fn edge_matching(e: u32) -> u32 { return tables[2u * EDGES + e]; }
fn edge_control_face(e: u32) -> u32 { return tables[3u * EDGES + e]; }
fn triangle(f: u32, k: u32) -> u32 { return tables[4u * EDGES + f * 3u + k]; }

fn stir(v: u32) -> u32 {
    var x = v;
    x ^= x >> 16u;
    x *= 0x21f0aaadu;
    x ^= x >> 15u;
    x *= 0x735a2d97u;
    x ^= x >> 15u;
    return x;
}

fn rotl7(x: u32) -> u32 {
    return (x << 7u) | (x >> 25u);
}

fn plan_word(seed: u32, round: u32) -> u32 {
    return stir(rotl7(seed) ^ (round * 0x2545f491u) ^ 0x734c4154u);
}

fn seed_register(seed: u32, cell: u32, vertex: u32) -> u32 {
    let h = stir(seed ^ 0x6e657875u) + cell * 0x9e3779b1u + vertex * 0x7f4a7c15u;
    return stir(h) & 3u;
}

fn control(seed: u32, round: u32, cell: u32, face: u32) -> u32 {
    let h = seed + round * 0x27d4eb2fu + cell * 0x165667b1u + face * 0x9e3779b1u;
    return stir(h) % 3u;
}

fn increment(control_value: u32, word: u32, edge: u32, cell: u32) -> u32 {
    return 1u + (control_value + word + edge + cell) % 3u;
}

fn partner(cells: u32, round: u32, cell: u32) -> u32 {
    let offset = round & 1u;
    if ((cell + offset) % 2u == 0u) {
        return (cell + 1u) % cells;
    }
    return (cell + cells - 1u) % cells;
}

fn linear_group(wg: vec3<u32>) -> u32 {
    return wg.y * GROUPS_X + wg.x;
}

var<workgroup> regs: array<u32, 12>;
var<workgroup> controls: array<u32, 8>;
var<workgroup> wg_disagreement: atomic<u32>;

// One invocation per register.
@compute @workgroup_size(64)
fn seed_state(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>) {
    let i = linear_group(wg) * REGISTER_GROUP + lid.x;
    if (i >= params.cells * VERTICES) {
        return;
    }
    state_in[i] = seed_register(params.seed, i / VERTICES, i % VERTICES);
}

// One 32-invocation workgroup per cell: 24 edge lanes, 8 face-control lanes.
@compute @workgroup_size(32)
fn local_round(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>) {
    let cell = linear_group(wg);
    let lane = lid.x;
    if (cell >= params.cells) {
        return;
    }
    if (lane < VERTICES) {
        regs[lane] = state_in[cell * VERTICES + lane];
    }
    if (lane >= EDGES) {
        controls[lane - EDGES] = control(params.seed, params.round, cell, lane - EDGES);
    }
    workgroupBarrier();

    let pw = plan_word(params.seed, params.round);
    let active_matching = pw & 3u;
    let flip = (pw >> 2u) & 1u;
    var is_active = false;
    var tgt = 0u;
    var delta = 0u;
    if (lane < EDGES && edge_matching(lane) == active_matching) {
        is_active = true;
        var ctrl = edge_a(lane);
        tgt = edge_b(lane);
        if (flip == 1u) {
            let swap = ctrl;
            ctrl = tgt;
            tgt = swap;
        }
        delta = increment(regs[ctrl], controls[edge_control_face(lane)], lane, cell);
    }
    workgroupBarrier();

    if (is_active) {
        if ((params.flags & 1u) == 1u) {
            regs[tgt] = (regs[tgt] + 4u - delta) & 3u;
        } else {
            regs[tgt] = (regs[tgt] + delta) & 3u;
        }
    }
    workgroupBarrier();

    if (lane < VERTICES) {
        state_in[cell * VERTICES + lane] = regs[lane];
    }
}

// One invocation per register: exchange the selected face with the partner.
@compute @workgroup_size(64)
fn couple_round(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>) {
    let i = linear_group(wg) * REGISTER_GROUP + lid.x;
    if (i >= params.cells * VERTICES) {
        return;
    }
    let cell = i / VERTICES;
    let vertex = i % VERTICES;
    let face = (plan_word(params.seed, params.round) >> 3u) & 7u;
    let in_face = vertex == triangle(face, 0u) || vertex == triangle(face, 1u) || vertex == triangle(face, 2u);
    if (in_face) {
        let other = partner(params.cells, params.round, cell);
        state_out[i] = state_in[other * VERTICES + vertex];
    } else {
        state_out[i] = state_in[i];
    }
}

// One 32-invocation workgroup per cell. Region layout (65 words):
// [0,48) vertex/state, [48,52) totals, [52,60) face excitation,
// [60] edge disagreement, [61,65) order-independent fold.
@compute @workgroup_size(32)
fn census_pass(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>) {
    let cell = linear_group(wg);
    let lane = lid.x;
    if (cell >= params.cells) {
        return;
    }
    let base = (params.flags >> 8u) * REGION_WORDS;
    if (lane == 0u) {
        atomicStore(&wg_disagreement, 0u);
    }
    if (lane < VERTICES) {
        let value = state_in[cell * VERTICES + lane];
        regs[lane] = value;
        atomicAdd(&census[base + lane * 4u + value], 1u);
        atomicAdd(&census[base + 48u + value], 1u);
    }
    workgroupBarrier();

    if (lane >= EDGES) {
        let f = lane - EDGES;
        let sum = (regs[triangle(f, 0u)] + regs[triangle(f, 1u)] + regs[triangle(f, 2u)]) & 3u;
        if (sum != 0u) {
            atomicAdd(&census[base + 52u + f], 1u);
        }
    } else {
        if (regs[edge_a(lane)] != regs[edge_b(lane)]) {
            atomicAdd(&wg_disagreement, 1u);
        }
    }
    workgroupBarrier();

    if (lane == 0u) {
        atomicAdd(&census[base + 60u], atomicLoad(&wg_disagreement));
        var word = 0u;
        for (var v = 0u; v < VERTICES; v++) {
            word |= regs[v] << (2u * v);
        }
        atomicAdd(&census[base + 61u + (cell & 3u)], stir(word ^ stir(cell)));
    }
}

// One invocation per register: count registers that differ from the seed.
@compute @workgroup_size(64)
fn compare_pass(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_id) lid: vec3<u32>) {
    let i = linear_group(wg) * REGISTER_GROUP + lid.x;
    if (i >= params.cells * VERTICES) {
        return;
    }
    let base = (params.flags >> 8u) * REGION_WORDS;
    if (state_in[i] != seed_register(params.seed, i / VERTICES, i % VERTICES)) {
        atomicAdd(&census[base], 1u);
    }
}
