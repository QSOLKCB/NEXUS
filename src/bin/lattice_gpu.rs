// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `nexus-lattice-gpu`: Vulkan/WGSL mirror of the reversible lattice.

#![forbid(unsafe_code)]

use std::env;
use std::process::ExitCode;
use std::time::Instant;

use nexus::gpu::{self, Lattice};
use nexus::ve::lattice::{Workload, DEFAULT_SEED};

const HELP: &str = "\
nexus-lattice-gpu — Vulkan/WGSL mirror of the reversible cuboctahedral lattice

USAGE:
  nexus-lattice-gpu info
  nexus-lattice-gpu verify [cells] [--rounds <n>] [--seed <u32>]
  nexus-lattice-gpu run <cells> <rounds> [--seed <u32>]

Select an adapter with NEXUS_GPU_ADAPTER=\"<substring>\" when several exist.
Verification compares the GPU census exactly with the CPU reference and
replays the inverse schedule on both. This is classical computation.
";

fn main() -> ExitCode {
    match dispatch() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}\n\n{HELP}");
            ExitCode::FAILURE
        }
    }
}

struct Options {
    positional: Vec<String>,
    supplied: Vec<String>,
    rounds: Option<u32>,
    seed: u32,
}

fn parse(raw: &[String]) -> Result<Options, String> {
    let mut options = Options {
        positional: Vec::new(),
        supplied: Vec::new(),
        rounds: None,
        seed: DEFAULT_SEED,
    };
    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "--rounds" => {
                let v = raw.get(i + 1).ok_or("missing value after --rounds")?;
                options.supplied.push("rounds".into());
                options.rounds = Some(v.parse().map_err(|_| format!("invalid rounds `{v}`"))?);
                i += 2;
            }
            "--seed" => {
                let v = raw.get(i + 1).ok_or("missing value after --seed")?;
                options.supplied.push("seed".into());
                options.seed = v.parse().map_err(|_| format!("invalid seed `{v}`"))?;
                i += 2;
            }
            other if other.starts_with("--") => return Err(format!("unknown option `{other}`")),
            other => {
                options.positional.push(other.to_string());
                i += 1;
            }
        }
    }
    Ok(options)
}

fn validate_command(command: &str, options: &Options) -> Result<(), String> {
    let (min, max, allowed): (usize, usize, &[&str]) = match command {
        "verify" => (0, 1, &["rounds", "seed"]),
        "run" | "lattice" | "benchmark" => (2, 2, &["seed"]),
        "info" | "help" | "--help" | "-h" => (0, 0, &[]),
        _ => return Ok(()),
    };
    if !(min..=max).contains(&options.positional.len()) {
        return Err(format!("{command} expects {min}..={max} positional arguments"));
    }
    for name in &options.supplied {
        if !allowed.contains(&name.as_str()) {
            return Err(format!("--{name} is not supported by {command}"));
        }
    }
    Ok(())
}

fn number<T: std::str::FromStr>(options: &Options, slot: usize, name: &str) -> Result<T, String> {
    let v = options
        .positional
        .get(slot)
        .ok_or_else(|| format!("missing <{name}>"))?;
    v.parse().map_err(|_| format!("invalid {name} `{v}`"))
}

fn print_adapter(context: &Lattice) {
    let info = context.info();
    println!("metric,value");
    println!("engine,vulkan-wgsl");
    println!("adapter_name,{}", info.name.replace(',', " "));
    println!("backend,{:?}", info.backend);
    println!("device_type,{:?}", info.device_type);
    println!("vendor_id,{}", info.vendor);
    println!("device_id,{}", info.device);
    println!("driver,{}", info.driver.replace(',', " "));
    println!("driver_info,{}", info.driver_info.replace(',', " "));
    println!("dynamic_uniform_stride,{}", context.stride());
}

fn print_census(prefix: &str, census: &nexus::ve::lattice::Census, cells: u32) {
    println!("{prefix}_total_cv,{:.9}", census.total_cv());
    println!("{prefix}_vertex_state_cv,{:.9}", census.vertex_state_cv());
    println!(
        "{prefix}_edge_disagreement_ratio,{:.9}",
        census.disagreement_ratio(cells)
    );
    for (i, value) in census.excited.iter().enumerate() {
        println!("{prefix}_face_{i}_excited,{value}");
    }
    println!("{prefix}_fold,{}", census.fold_hex());
}

fn dispatch() -> Result<(), String> {
    let raw: Vec<String> = env::args().skip(1).collect();
    let Some(command) = raw.first() else {
        println!("{HELP}");
        return Ok(());
    };
    let options = parse(&raw[1..])?;
    validate_command(command, &options)?;
    match command.as_str() {
        "info" => {
            let start = Instant::now();
            let context = pollster::block_on(Lattice::open(2))?;
            print_adapter(&context);
            println!("initialization_ns,{}", start.elapsed().as_nanos());
            println!("workgroup_invocations_per_cell,32");
            println!("edge_lanes,24");
            println!("face_control_lanes,8");
            println!("tables_source,cpu-derived");
            println!("quantum_claim,false");
            Ok(())
        }
        "verify" => {
            let cells: u32 = if options.positional.is_empty() {
                1_024
            } else {
                number(&options, 0, "cells")?
            };
            let rounds = options.rounds.unwrap_or(16);
            let start = Instant::now();
            let context = pollster::block_on(Lattice::open(cells))?;
            let initialization = start.elapsed();
            let v = gpu::verify(&context, rounds, options.seed)?;
            let workload = Workload::of(cells, rounds);
            print_adapter(&context);
            println!("cells,{cells}");
            println!("rounds,{rounds}");
            println!("seed,{}", options.seed);
            println!("initialization_ns,{}", initialization.as_nanos());
            println!("gpu_elapsed_ns,{}", v.first.elapsed.as_nanos());
            println!("cpu_elapsed_ns,{}", v.cpu.elapsed.as_nanos());
            println!("registers_verified,{}", u64::from(cells) * 12);
            println!("edge_gates,{}", workload.edge_gates);
            println!("face_swaps,{}", workload.face_swaps);
            println!("gpu_resident_bytes,{}", workload.gpu_resident_bytes);
            println!("repeatable,{}", v.repeatable);
            println!(
                "gpu_restoration_mismatches,{}",
                v.first.restoration_mismatches.unwrap_or(u64::MAX)
            );
            println!("cpu_restoration_mismatches,{}", v.cpu.restoration_mismatches);
            println!("initial_max_cpu_gpu_delta,{}", v.initial_parity.max_delta);
            println!("initial_l1_cpu_gpu_delta,{}", v.initial_parity.l1_delta);
            println!("initial_fold_cpu_gpu_mismatches,{}", v.initial_parity.fold_mismatches);
            println!("final_max_cpu_gpu_delta,{}", v.final_parity.max_delta);
            println!("final_l1_cpu_gpu_delta,{}", v.final_parity.l1_delta);
            println!("final_fold_cpu_gpu_mismatches,{}", v.final_parity.fold_mismatches);
            println!("initial_gpu_fold,{}", v.first.initial.fold_hex());
            println!("final_gpu_fold,{}", v.first.final_state.fold_hex());
            println!("quantum_claim,false");
            let passed = v.passed(cells);
            println!("passed,{passed}");
            if passed {
                Ok(())
            } else {
                Err("GPU lattice verification failed".into())
            }
        }
        "run" | "lattice" | "benchmark" => {
            let cells: u32 = number(&options, 0, "cells")?;
            let rounds: u32 = number(&options, 1, "rounds")?;
            let start = Instant::now();
            let context = pollster::block_on(Lattice::open(cells))?;
            let initialization = start.elapsed();
            let outcome = context.execute(rounds, options.seed, false)?;
            outcome.initial.validate(cells).map_err(|e| e.to_string())?;
            outcome.final_state.validate(cells).map_err(|e| e.to_string())?;
            let workload = Workload::of(cells, rounds);
            print_adapter(&context);
            println!("cells,{cells}");
            println!("rounds,{rounds}");
            println!("seed,{}", options.seed);
            println!("initialization_ns,{}", initialization.as_nanos());
            println!("elapsed_ns,{}", outcome.elapsed.as_nanos());
            let cell_rounds = u64::from(cells) * u64::from(rounds);
            println!(
                "ns_per_cell_round,{:.6}",
                outcome.elapsed.as_nanos() as f64 / cell_rounds as f64
            );
            println!("edge_gates,{}", workload.edge_gates);
            println!("face_swaps,{}", workload.face_swaps);
            println!(
                "edge_gates_per_second,{:.3}",
                workload.edge_gates as f64 / outcome.elapsed.as_secs_f64().max(f64::MIN_POSITIVE)
            );
            println!("configuration_bits,{}", workload.configuration_bits);
            println!("gpu_resident_bytes,{}", workload.gpu_resident_bytes);
            println!("compact_readback_bytes,{}", gpu::REGION_WORDS * 2 * 4);
            print_census("initial", &outcome.initial, cells);
            print_census("final", &outcome.final_state, cells);
            println!("fold_kind_cryptographic,false");
            println!("canonical,false");
            println!("quantum_claim,false");
            Ok(())
        }
        "help" | "--help" | "-h" => {
            println!("{HELP}");
            Ok(())
        }
        other => Err(format!("unknown command `{other}`")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ignored_options_and_extra_arguments_are_rejected_before_device_access() {
        for (command, raw) in [
            ("info", vec!["--seed", "1"]),
            ("verify", vec!["2", "64"]),
            ("run", vec!["2", "1", "--rounds", "64"]),
        ] {
            let args: Vec<String> = raw.into_iter().map(str::to_string).collect();
            assert!(validate_command(command, &parse(&args).unwrap()).is_err());
        }
    }
}
