// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Optional Vulkan/WGSL mirror of the reversible lattice (`gpu` feature).
//!
//! The GPU keeps the whole register state resident in two ping-pong storage
//! buffers and returns only a compact census (three 65-word regions). The
//! cuboctahedron tables are derived on the CPU and uploaded, so both engines
//! read the same combinatorics. Results are compared exactly against the CPU
//! reference in [`crate::ve::lattice`]; nothing here is canonical on its own.

use std::borrow::Cow;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use wgpu::util::DeviceExt;

use crate::error::{Error, Result};
use crate::ve::graph::{Cuboctahedron, EDGES, TRIANGLES, VERTICES};
use crate::ve::lattice::{self, validate_request, Census};

/// The lattice shader source.
pub const SHADER: &str = include_str!("lattice.wgsl");
/// Words per census region.
pub const REGION_WORDS: usize = 65;
/// Census regions: initial, final, compare.
pub const REGIONS: usize = 3;
const PARAM_BYTES: u64 = 16;
const GROUPS_X: u32 = 32_768;
const REGISTER_GROUP: u32 = 64;
const TABLE_WORDS: usize = 4 * EDGES + 3 * TRIANGLES;

fn validate_cells(cells: u32) -> Result<()> {
    validate_request(cells, 1)?;
    // The largest non-wrapping shader census is 24 disagreements per cell.
    // Folds wrap intentionally; census counts and register indices must not.
    if cells > u32::MAX / EDGES as u32 {
        return Err(Error::Lattice("cells exceed the shader's u32 census capacity"));
    }
    Ok(())
}

/// Flatten the derived cuboctahedron into the shader's table layout.
pub fn table_words(graph: &Cuboctahedron) -> [u32; TABLE_WORDS] {
    let mut words = [0u32; TABLE_WORDS];
    for (e, &(a, b)) in graph.edges.iter().enumerate() {
        words[e] = u32::from(a);
        words[EDGES + e] = u32::from(b);
        words[2 * EDGES + e] = u32::from(graph.matching[e]);
        words[3 * EDGES + e] = u32::from(graph.control_face[e]);
    }
    for (f, tri) in graph.triangles.iter().enumerate() {
        for (k, &v) in tri.iter().enumerate() {
            words[4 * EDGES + f * 3 + k] = u32::from(v);
        }
    }
    words
}

/// Decode one census region from readback words.
pub fn census_from_words(words: &[u32]) -> Result<Census> {
    if words.len() < REGION_WORDS {
        return Err(Error::Lattice("census readback is too short"));
    }
    let mut census = Census {
        vertex_state: [0; VERTICES * 4],
        totals: [0; 4],
        excited: [0; TRIANGLES],
        disagreement: u64::from(words[60]),
        fold: [words[61], words[62], words[63], words[64]],
    };
    for (slot, word) in census.vertex_state.iter_mut().zip(&words[0..48]) {
        *slot = u64::from(*word);
    }
    for (slot, word) in census.totals.iter_mut().zip(&words[48..52]) {
        *slot = u64::from(*word);
    }
    for (slot, word) in census.excited.iter_mut().zip(&words[52..60]) {
        *slot = u64::from(*word);
    }
    Ok(census)
}

/// Encode one parameter slot.
pub fn encode_params(cells: u32, seed: u32, round: u32, flags: u32) -> [u8; PARAM_BYTES as usize] {
    let mut out = [0u8; PARAM_BYTES as usize];
    out[0..4].copy_from_slice(&cells.to_le_bytes());
    out[4..8].copy_from_slice(&seed.to_le_bytes());
    out[8..12].copy_from_slice(&round.to_le_bytes());
    out[12..16].copy_from_slice(&flags.to_le_bytes());
    out
}

/// Flag word for a census pass into a region.
pub const fn census_flags(region: u32) -> u32 {
    region << 8
}

/// Workgroup grid for one workgroup per cell or per register group.
pub fn grid(groups: u32) -> (u32, u32) {
    if groups <= GROUPS_X {
        (groups.max(1), 1)
    } else {
        (GROUPS_X, groups.div_ceil(GROUPS_X))
    }
}

/// Score adapters so a discrete GPU wins over integrated, virtual, or CPU.
pub fn adapter_score(kind: wgpu::DeviceType) -> u8 {
    match kind {
        wgpu::DeviceType::DiscreteGpu => 5,
        wgpu::DeviceType::IntegratedGpu => 4,
        wgpu::DeviceType::VirtualGpu => 3,
        wgpu::DeviceType::Other => 2,
        wgpu::DeviceType::Cpu => 1,
    }
}

fn pick_adapter(adapters: Vec<wgpu::Adapter>) -> std::result::Result<wgpu::Adapter, String> {
    if adapters.is_empty() {
        return Err("no Vulkan adapter was found".into());
    }
    let wanted = std::env::var("NEXUS_GPU_ADAPTER").ok().map(|s| s.to_lowercase());
    let mut best: Option<(u8, wgpu::Adapter)> = None;
    for adapter in adapters {
        let info = adapter.get_info();
        if let Some(wanted) = &wanted {
            if !info.name.to_lowercase().contains(wanted.as_str()) {
                continue;
            }
        }
        let score = adapter_score(info.device_type);
        if best.as_ref().is_none_or(|(s, _)| score > *s) {
            best = Some((score, adapter));
        }
    }
    best.map(|(_, a)| a)
        .ok_or_else(|| "no Vulkan adapter matched NEXUS_GPU_ADAPTER".to_string())
}

/// Outcome of one GPU execution.
#[derive(Debug, Clone)]
pub struct GpuOutcome {
    /// Census before any round.
    pub initial: Census,
    /// Census after the forward schedule.
    pub final_state: Census,
    /// Registers that differed from the seed after inverse replay, when run.
    pub restoration_mismatches: Option<u64>,
    /// Wall time from submission to readback.
    pub elapsed: Duration,
}

/// A device, its pipelines, and resident state for a fixed cell count.
pub struct Lattice {
    _instance: wgpu::Instance,
    info: wgpu::AdapterInfo,
    device: wgpu::Device,
    queue: wgpu::Queue,
    layout: wgpu::BindGroupLayout,
    pipelines: Pipelines,
    state: [wgpu::Buffer; 2],
    census: wgpu::Buffer,
    staging: wgpu::Buffer,
    tables: wgpu::Buffer,
    cells: u32,
    stride: u64,
}

struct Pipelines {
    seed: wgpu::ComputePipeline,
    local: wgpu::ComputePipeline,
    couple: wgpu::ComputePipeline,
    census: wgpu::ComputePipeline,
    compare: wgpu::ComputePipeline,
}

fn storage_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

impl Lattice {
    /// Open the best Vulkan adapter and allocate resident state for `cells`.
    pub async fn open(cells: u32) -> std::result::Result<Self, String> {
        validate_cells(cells).map_err(|e| e.to_string())?;
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = wgpu::Backends::VULKAN;
        let instance = wgpu::Instance::new(descriptor);
        let adapter = pick_adapter(instance.enumerate_adapters(wgpu::Backends::VULKAN).await)?;
        let info = adapter.get_info();
        let limits = adapter.limits();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("nexus lattice"),
                required_features: wgpu::Features::empty(),
                required_limits: limits.clone(),
                ..Default::default()
            })
            .await
            .map_err(|e| format!("device request failed: {e}"))?;

        let state_bytes = u64::from(cells) * VERTICES as u64 * 4;
        if state_bytes > limits.max_storage_buffer_binding_size || state_bytes > limits.max_buffer_size {
            return Err(format!(
                "state buffer of {state_bytes} bytes exceeds adapter limits (binding {}, buffer {})",
                limits.max_storage_buffer_binding_size, limits.max_buffer_size
            ));
        }
        let state = [0, 1].map(|i| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(if i == 0 { "nexus state a" } else { "nexus state b" }),
                size: state_bytes,
                usage: wgpu::BufferUsages::STORAGE,
                mapped_at_creation: false,
            })
        });
        let census_bytes = (REGION_WORDS * REGIONS * 4) as u64;
        let census = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("nexus census"),
            size: census_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("nexus census readback"),
            size: census_bytes,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let words = table_words(&Cuboctahedron::derive());
        let mut table_bytes = Vec::with_capacity(TABLE_WORDS * 4);
        for word in words {
            table_bytes.extend_from_slice(&word.to_le_bytes());
        }
        let tables = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("nexus cuboctahedron tables"),
            contents: &table_bytes,
            usage: wgpu::BufferUsages::STORAGE,
        });

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("nexus lattice layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(PARAM_BYTES),
                    },
                    count: None,
                },
                storage_entry(1, false),
                storage_entry(2, false),
                storage_entry(3, false),
                storage_entry(4, true),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("nexus lattice pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("nexus lattice shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(SHADER)),
        });
        let pipeline = |entry: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let pipelines = Pipelines {
            seed: pipeline("seed_state"),
            local: pipeline("local_round"),
            couple: pipeline("couple_round"),
            census: pipeline("census_pass"),
            compare: pipeline("compare_pass"),
        };
        let stride = PARAM_BYTES.max(u64::from(limits.min_uniform_buffer_offset_alignment));
        Ok(Self {
            _instance: instance,
            info,
            device,
            queue,
            layout,
            pipelines,
            state,
            census,
            staging,
            tables,
            cells,
            stride,
        })
    }

    /// Adapter information.
    pub fn info(&self) -> &wgpu::AdapterInfo {
        &self.info
    }

    /// Cell count this context was opened for.
    pub const fn cells(&self) -> u32 {
        self.cells
    }

    /// Dynamic uniform stride in bytes.
    pub const fn stride(&self) -> u64 {
        self.stride
    }

    fn bind_group(&self, params: &wgpu::Buffer, input: usize) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("nexus lattice bind group"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: params,
                        offset: 0,
                        size: wgpu::BufferSize::new(PARAM_BYTES),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.state[input].as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.state[1 - input].as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.census.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.tables.as_entire_binding(),
                },
            ],
        })
    }

    /// Run the forward schedule, census both ends, and optionally replay the
    /// inverse schedule and count unrestored registers.
    pub fn execute(&self, rounds: u32, seed: u32, verify: bool) -> std::result::Result<GpuOutcome, String> {
        validate_request(self.cells, rounds).map_err(|e| e.to_string())?;
        let cells = self.cells;

        // Parameter slots: forward rounds, inverse rounds, then three census
        // or compare passes. Each slot is one dynamic uniform offset.
        let mut slots: Vec<[u8; PARAM_BYTES as usize]> = Vec::new();
        for round in 0..rounds {
            slots.push(encode_params(cells, seed, round, 0));
        }
        let inverse_base = slots.len();
        for round in 0..rounds {
            slots.push(encode_params(cells, seed, round, 1));
        }
        let census_base = slots.len();
        for region in 0..REGIONS as u32 {
            slots.push(encode_params(cells, seed, 0, census_flags(region)));
        }
        let mut param_bytes = vec![0u8; slots.len() * self.stride as usize];
        for (i, slot) in slots.iter().enumerate() {
            let at = i * self.stride as usize;
            param_bytes[at..at + PARAM_BYTES as usize].copy_from_slice(slot);
        }
        let params = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("nexus lattice params"),
            contents: &param_bytes,
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let groups = [self.bind_group(&params, 0), self.bind_group(&params, 1)];
        let offset = |slot: usize| -> std::result::Result<u32, String> {
            u32::try_from(slot as u64 * self.stride).map_err(|_| "dynamic offset exceeds u32".to_string())
        };

        let cell_grid = grid(cells);
        let register_grid = grid((cells * VERTICES as u32).div_ceil(REGISTER_GROUP));
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("nexus lattice schedule"),
        });
        encoder.clear_buffer(&self.census, 0, None);

        let mut current = 0usize;
        {
            let mut dispatch = |pipeline: &wgpu::ComputePipeline, group: usize, slot: usize, (x, y): (u32, u32)| {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: None,
                    timestamp_writes: None,
                });
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &groups[group], &[offset(slot)?]);
                pass.dispatch_workgroups(x, y, 1);
                Ok::<(), String>(())
            };
            dispatch(&self.pipelines.seed, current, 0, register_grid)?;
            dispatch(&self.pipelines.census, current, census_base, cell_grid)?;
            for round in 0..rounds as usize {
                dispatch(&self.pipelines.local, current, round, cell_grid)?;
                dispatch(&self.pipelines.couple, current, round, register_grid)?;
                current = 1 - current;
            }
            dispatch(&self.pipelines.census, current, census_base + 1, cell_grid)?;
            if verify {
                for round in (0..rounds as usize).rev() {
                    dispatch(&self.pipelines.couple, current, inverse_base + round, register_grid)?;
                    current = 1 - current;
                    dispatch(&self.pipelines.local, current, inverse_base + round, cell_grid)?;
                }
                dispatch(&self.pipelines.compare, current, census_base + 2, register_grid)?;
            }
        }
        let readback_bytes = (REGION_WORDS * if verify { REGIONS } else { 2 } * 4) as u64;
        encoder.copy_buffer_to_buffer(&self.census, 0, &self.staging, 0, readback_bytes);

        let start = Instant::now();
        self.queue.submit(Some(encoder.finish()));
        let (sender, receiver) = mpsc::channel();
        self.staging
            .map_async(wgpu::MapMode::Read, ..readback_bytes, move |result| {
                let _ = sender.send(result);
            });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| format!("device poll failed: {e}"))?;
        receiver
            .recv()
            .map_err(|_| "readback callback was dropped".to_string())?
            .map_err(|e| format!("readback failed: {e}"))?;
        let words: Vec<u32> = {
            let view = self
                .staging
                .get_mapped_range(..readback_bytes)
                .map_err(|e| format!("mapped range failed: {e}"))?;
            view.chunks_exact(4)
                .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect()
        };
        self.staging.unmap();
        let elapsed = start.elapsed();

        let initial = census_from_words(&words[0..REGION_WORDS]).map_err(|e| e.to_string())?;
        let final_state = census_from_words(&words[REGION_WORDS..2 * REGION_WORDS]).map_err(|e| e.to_string())?;
        let restoration_mismatches = verify.then(|| u64::from(words[2 * REGION_WORDS]));
        Ok(GpuOutcome {
            initial,
            final_state,
            restoration_mismatches,
            elapsed,
        })
    }
}

/// Exact CPU/GPU comparison of two outcomes' censuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parity {
    /// Largest absolute metric difference.
    pub max_delta: u64,
    /// Sum of absolute metric differences.
    pub l1_delta: u64,
    /// Fold words that differ, out of four.
    pub fold_mismatches: u64,
}

/// Compare a GPU census with a CPU census.
pub fn parity(gpu: &Census, cpu: &Census) -> Parity {
    let (max_delta, l1_delta, fold_mismatches) = gpu.compare(cpu);
    Parity {
        max_delta,
        l1_delta,
        fold_mismatches,
    }
}

/// Everything a full verification reports.
#[derive(Debug, Clone)]
pub struct GpuVerification {
    /// First GPU execution.
    pub first: GpuOutcome,
    /// Whether a second GPU execution reproduced the first exactly.
    pub repeatable: bool,
    /// CPU reference result.
    pub cpu: lattice::Verification,
    /// Initial-census parity.
    pub initial_parity: Parity,
    /// Final-census parity.
    pub final_parity: Parity,
}

impl GpuVerification {
    /// Pass criteria: repeatable, structurally valid, fully restored on both
    /// engines, and zero CPU/GPU deltas with zero fold mismatches.
    pub fn passed(&self, cells: u32) -> bool {
        self.repeatable
            && self.first.initial.validate(cells).is_ok()
            && self.first.final_state.validate(cells).is_ok()
            && self.first.restoration_mismatches == Some(0)
            && self.cpu.restoration_mismatches == 0
            && self.initial_parity
                == Parity {
                    max_delta: 0,
                    l1_delta: 0,
                    fold_mismatches: 0,
                }
            && self.final_parity
                == Parity {
                    max_delta: 0,
                    l1_delta: 0,
                    fold_mismatches: 0,
                }
    }
}

/// Run the complete verification contract.
pub fn verify(context: &Lattice, rounds: u32, seed: u32) -> std::result::Result<GpuVerification, String> {
    let first = context.execute(rounds, seed, true)?;
    let second = context.execute(rounds, seed, true)?;
    let repeatable = first.initial == second.initial
        && first.final_state == second.final_state
        && first.restoration_mismatches == second.restoration_mismatches;
    let cpu = lattice::verify(context.cells(), rounds, seed).map_err(|e| e.to_string())?;
    let initial_parity = parity(&first.initial, &cpu.initial);
    let final_parity = parity(&first.final_state, &cpu.final_state);
    Ok(GpuVerification {
        first,
        repeatable,
        cpu,
        initial_parity,
        final_parity,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shader_counts_cannot_wrap() {
        let largest = (u32::MAX / EDGES as u32) & !1;
        assert!(validate_cells(largest).is_ok());
        assert!(validate_cells(largest + 2).is_err());
        assert!(validate_cells(u32::MAX - 1).is_err());
    }

    #[test]
    fn shader_parses_and_validates() {
        let module = wgpu::naga::front::wgsl::parse_str(SHADER).expect("WGSL parses");
        let mut validator = wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::default(),
        );
        validator.validate(&module).expect("WGSL validates");
        let names: Vec<&str> = module.entry_points.iter().map(|e| e.name.as_str()).collect();
        for entry in [
            "seed_state",
            "local_round",
            "couple_round",
            "census_pass",
            "compare_pass",
        ] {
            assert!(names.contains(&entry), "missing entry point {entry}");
        }
    }

    #[test]
    fn table_layout_matches_shader_offsets() {
        let graph = Cuboctahedron::derive();
        let words = table_words(&graph);
        assert_eq!(words.len(), 120);
        for e in 0..EDGES {
            assert_eq!(words[e], u32::from(graph.edges[e].0));
            assert_eq!(words[EDGES + e], u32::from(graph.edges[e].1));
            assert_eq!(words[2 * EDGES + e], u32::from(graph.matching[e]));
            assert_eq!(words[3 * EDGES + e], u32::from(graph.control_face[e]));
        }
        assert_eq!(words[4 * EDGES], u32::from(graph.triangles[0][0]));
    }

    #[test]
    fn grid_covers_large_cell_counts() {
        assert_eq!(grid(1), (1, 1));
        assert_eq!(grid(32_768), (32_768, 1));
        let (x, y) = grid(1 << 20);
        assert!(u64::from(x) * u64::from(y) >= 1 << 20);
        assert!(x <= 65_535 && y <= 65_535);
    }

    #[test]
    fn census_decoding_round_trips() {
        let mut words = vec![0u32; REGION_WORDS];
        words[5] = 7;
        words[49] = 11;
        words[55] = 3;
        words[60] = 99;
        words[61..65].copy_from_slice(&[1, 2, 3, 4]);
        let census = census_from_words(&words).unwrap();
        assert_eq!(census.vertex_state[5], 7);
        assert_eq!(census.totals[1], 11);
        assert_eq!(census.excited[3], 3);
        assert_eq!(census.disagreement, 99);
        assert_eq!(census.fold, [1, 2, 3, 4]);
        assert!(census_from_words(&words[..10]).is_err());
    }

    #[test]
    fn params_are_little_endian_sixteen_bytes() {
        let p = encode_params(1024, 0x11223344, 7, census_flags(2));
        assert_eq!(&p[0..4], &1024u32.to_le_bytes());
        assert_eq!(&p[12..16], &(2u32 << 8).to_le_bytes());
    }

    #[test]
    fn discrete_adapters_score_highest() {
        assert!(adapter_score(wgpu::DeviceType::DiscreteGpu) > adapter_score(wgpu::DeviceType::IntegratedGpu));
        assert!(adapter_score(wgpu::DeviceType::IntegratedGpu) > adapter_score(wgpu::DeviceType::Cpu));
    }
}
