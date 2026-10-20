// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! `nexus` command-line evidence runner.
//!
//! Metric reports print `metric,value` CSV by default or a flat JSON object
//! with `--json`. Tabular commands always print CSV with a header row.

#![forbid(unsafe_code)]

use std::env;
use std::hint::black_box;
use std::process::ExitCode;
use std::time::Instant;

use nexus::ve::atlas::canonical_census;
use nexus::ve::lattice::{self, DEFAULT_SEED};
use nexus::{
    audit, audit_parallel, canonical_ledger, effective_workers, event, run, run_parallel, sample, EventMap, Field,
    FieldSpec, Job, Lane, Ordering, StrandTable,
};

const HELP: &str = "\
nexus — deterministic centre-transit field evidence runner

USAGE:
  nexus audit [--workers <n>] [FIELD] [--json]
  nexus trace <ordinal> <steps> [FIELD]
  nexus events <ordinal> <steps> [--base-hz <f64>] [FIELD]
  nexus ledger <samples> [FIELD] [--json]
  nexus bench <iterations> [FIELD] [--json]
  nexus bench-exec <iterations> <batch> <workers> [FIELD]
  nexus sweep
  nexus atlas <samples> [--rounds <n>] [FIELD] [--json]
  nexus lattice-verify [cells] [--rounds <n>] [--seed <u32>] [--json]
  nexus lattice-run <cells> <rounds> [--seed <u32>] [--json]
  nexus version

FIELD OPTIONS:
  --cardinality <u64>   logical cardinality L (default 16777216)
  --strands <u32>       representative strands N (default 1024)
  --tracers <u32>       bounded tracer budget (default 512)
  --radius <f64>        receiver radius R (default 0.56)
  --phase <f64>         bearing phase in radians (default 0)
  --twist <f64>         twist per half-transit in turns (default 1.5)
  --ordering <name>     uniform (default) or golden

OUTPUT:
  --json                emit metric reports as a flat JSON object
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

/// Parsed command-line state shared by every subcommand.
struct Args {
    positional: Vec<String>,
    field: FieldSpec,
    ordering: Ordering,
    json: bool,
    workers: Option<usize>,
    rounds: Option<u32>,
    seed: Option<u32>,
    base_hz: Option<f64>,
}

fn parse_args(raw: &[String]) -> Result<Args, String> {
    let mut args = Args {
        positional: Vec::new(),
        field: FieldSpec::default(),
        ordering: Ordering::Uniform,
        json: false,
        workers: None,
        rounds: None,
        seed: None,
        base_hz: None,
    };
    let mut index = 0;
    while index < raw.len() {
        let token = raw[index].as_str();
        if token == "--json" {
            args.json = true;
            index += 1;
            continue;
        }
        if let Some(name) = token.strip_prefix("--") {
            let value = raw
                .get(index + 1)
                .ok_or_else(|| format!("missing value after --{name}"))?;
            match name {
                "cardinality" => args.field.cardinality = number(value, name)?,
                "strands" => args.field.strands = number(value, name)?,
                "tracers" => args.field.tracers = number(value, name)?,
                "radius" => args.field.radius = number(value, name)?,
                "phase" => args.field.phase = number(value, name)?,
                "twist" => args.field.twist = number(value, name)?,
                "ordering" => {
                    args.ordering = match value.as_str() {
                        "uniform" => Ordering::Uniform,
                        "golden" => Ordering::Golden,
                        other => return Err(format!("unknown ordering `{other}`")),
                    }
                }
                "workers" => args.workers = Some(number(value, name)?),
                "rounds" => args.rounds = Some(number(value, name)?),
                "seed" => args.seed = Some(number(value, name)?),
                "base-hz" => args.base_hz = Some(number(value, name)?),
                other => return Err(format!("unknown option `--{other}`")),
            }
            index += 2;
        } else {
            args.positional.push(token.to_string());
            index += 1;
        }
    }
    Ok(args)
}

fn number<T: std::str::FromStr>(value: &str, name: &str) -> Result<T, String> {
    value
        .parse()
        .map_err(|_| format!("invalid value `{value}` for `{name}`"))
}

fn positional<T: std::str::FromStr>(args: &Args, slot: usize, name: &str) -> Result<T, String> {
    let value = args.positional.get(slot).ok_or_else(|| format!("missing <{name}>"))?;
    number(value, name)
}

fn field_of(args: &Args) -> Result<Field, String> {
    args.field.build().map_err(|e| e.to_string())
}

/// Accumulates `(metric, value)` pairs and renders them as CSV or JSON.
struct Report {
    rows: Vec<(String, String, bool)>,
}

impl Report {
    fn new() -> Self {
        Self { rows: Vec::new() }
    }

    fn num(&mut self, key: &str, value: impl std::fmt::Display) {
        self.rows.push((key.to_string(), value.to_string(), true));
    }

    fn float(&mut self, key: &str, value: f64) {
        self.rows.push((key.to_string(), format!("{value:.17e}"), true));
    }

    fn text(&mut self, key: &str, value: impl std::fmt::Display) {
        self.rows.push((key.to_string(), value.to_string(), false));
    }

    fn flag(&mut self, key: &str, value: bool) {
        self.rows.push((key.to_string(), value.to_string(), true));
    }

    fn print(&self, json: bool) {
        if json {
            println!("{{");
            for (i, (key, value, bare)) in self.rows.iter().enumerate() {
                let comma = if i + 1 == self.rows.len() { "" } else { "," };
                if *bare {
                    println!("  \"{key}\": {value}{comma}");
                } else {
                    println!("  \"{key}\": \"{}\"{comma}", value.replace('"', "\\\""));
                }
            }
            println!("}}");
        } else {
            println!("metric,value");
            for (key, value, _) in &self.rows {
                println!("{key},{value}");
            }
        }
    }
}

fn describe_field(report: &mut Report, field: &Field, ordering: Ordering) {
    report.num("cardinality", field.cardinality());
    report.num("strands", field.strands());
    report.num("tracers", field.tracers());
    report.float("radius", field.radius());
    report.float("phase", field.phase());
    report.float("twist", field.twist());
    report.text("ordering", ordering.label());
}

fn dispatch() -> Result<(), String> {
    let raw: Vec<String> = env::args().skip(1).collect();
    let Some(command) = raw.first() else {
        println!("{HELP}");
        return Ok(());
    };
    let args = parse_args(&raw[1..])?;
    match command.as_str() {
        "audit" => cmd_audit(&args),
        "trace" => cmd_trace(&args),
        "events" => cmd_events(&args),
        "ledger" => cmd_ledger(&args),
        "bench" => cmd_bench(&args),
        "bench-exec" => cmd_bench_exec(&args),
        "sweep" => cmd_sweep(),
        "atlas" => cmd_atlas(&args),
        "lattice-verify" => cmd_lattice_verify(&args),
        "lattice-run" => cmd_lattice_run(&args),
        "version" => {
            println!("nexus {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "help" | "--help" | "-h" => {
            println!("{HELP}");
            Ok(())
        }
        other => Err(format!("unknown command `{other}`")),
    }
}

fn cmd_audit(args: &Args) -> Result<(), String> {
    let field = field_of(args)?;
    let start = Instant::now();
    let (report_data, workers) = match args.workers {
        Some(workers) => (
            audit_parallel(&field, args.ordering, workers).map_err(|e| e.to_string())?,
            effective_workers(field.strands() as usize, workers).map_err(|e| e.to_string())?,
        ),
        None => (audit(&field, args.ordering).map_err(|e| e.to_string())?, 1),
    };
    let elapsed = start.elapsed();
    let mut report = Report::new();
    describe_field(&mut report, &field, args.ordering);
    report.num("requested_workers", args.workers.unwrap_or(1));
    report.num("effective_workers", workers);
    report.num("elapsed_ns", elapsed.as_nanos());
    report.float("radius_error", report_data.radius_error);
    report.float("antipodal_error", report_data.antipodal_error);
    report.float("nexus_error", report_data.nexus_error);
    report.float("endpoint_error", report_data.endpoint_error);
    report.num("gap_error", report_data.gap_error);
    report.float("eversion_before", report_data.eversion_before);
    report.float("eversion_at", report_data.eversion_at);
    report.float("eversion_after", report_data.eversion_after);
    report.flag("eversion_sign_ok", report_data.eversion_sign_ok());
    report.num("tracer_first", report_data.tracer_first);
    report.num("tracer_last", report_data.tracer_last);
    report.float("tracer_coverage", report_data.tracer_coverage());
    report.print(args.json);
    Ok(())
}

fn trace_steps(args: &Args) -> Result<(u32, u32), String> {
    let ordinal: u32 = positional(args, 0, "ordinal")?;
    let steps: u32 = positional(args, 1, "steps")?;
    if steps < 2 {
        return Err("steps must be at least 2".into());
    }
    Ok((ordinal, steps))
}

fn cmd_trace(args: &Args) -> Result<(), String> {
    let field = field_of(args)?;
    let (ordinal, steps) = trace_steps(args)?;
    println!("step,progress,x,y,norm,eversion,trit,trit_value,lane,logical,ordering");
    for step in 0..=steps {
        let progress = f64::from(step) / f64::from(steps);
        let s = sample(&field, args.ordering, Job { ordinal, progress }).map_err(|e| e.to_string())?;
        println!(
            "{step},{progress:.17},{:.17},{:.17},{:.17},{:.17},{},{},{},{},{}",
            s.pos.x,
            s.pos.y,
            s.pos.norm(),
            s.eversion,
            s.trit.label(),
            s.trit.value(),
            s.lane.label(),
            s.logical,
            s.ordering.label()
        );
    }
    Ok(())
}

fn cmd_events(args: &Args) -> Result<(), String> {
    let field = field_of(args)?;
    let (ordinal, steps) = trace_steps(args)?;
    let map = EventMap::at(args.base_hz.unwrap_or(432.0));
    println!("step,progress,x,y,norm,trit,lane,hz,gain,pan");
    for step in 0..=steps {
        let progress = f64::from(step) / f64::from(steps);
        let s = sample(&field, args.ordering, Job { ordinal, progress }).map_err(|e| e.to_string())?;
        let e = event(&field, &map, &s).map_err(|e| e.to_string())?;
        println!(
            "{step},{progress:.17},{:.17},{:.17},{:.17},{},{},{:.9},{:.9},{:.9}",
            s.pos.x,
            s.pos.y,
            s.pos.norm(),
            s.trit.label(),
            s.lane.label(),
            e.hz,
            e.gain,
            e.pan
        );
    }
    Ok(())
}

fn cmd_ledger(args: &Args) -> Result<(), String> {
    let field = field_of(args)?;
    let samples: u64 = positional(args, 0, "samples")?;
    let start = Instant::now();
    let ledger = canonical_ledger(&field, args.ordering, samples).map_err(|e| e.to_string())?;
    let elapsed = start.elapsed();
    let mut report = Report::new();
    describe_field(&mut report, &field, args.ordering);
    report.num("samples", ledger.count);
    report.num("encoded_bytes", ledger.encoded_bytes().map_err(|e| e.to_string())?);
    report.num("elapsed_ns", elapsed.as_nanos());
    for lane in Lane::ALL {
        report.num(&format!("{}_samples", lane.label()), ledger.lane_counts[lane.index()]);
        report.text(&format!("{}_sha256", lane.label()), ledger.lane_hex(lane));
    }
    report.text("stream_sha256", ledger.stream_hex());
    report.text("root_sha256", ledger.root_hex());
    report.print(args.json);
    Ok(())
}

fn cmd_bench(args: &Args) -> Result<(), String> {
    let field = field_of(args)?;
    let iterations: u64 = positional(args, 0, "iterations")?;
    if iterations == 0 {
        return Err("iterations must be greater than zero".into());
    }
    let table = StrandTable::new(field, args.ordering);
    let start = Instant::now();
    let mut checksum = 0.0_f64;
    for iteration in 0..iterations {
        for ordinal in 0..field.strands() {
            let progress = ((iteration + u64::from(ordinal)) % 1_001) as f64 / 1_000.0;
            let s = table.sample(Job { ordinal, progress }).map_err(|e| e.to_string())?;
            checksum += s.bearing + s.pos.x + s.pos.y;
        }
    }
    black_box(checksum);
    let elapsed = start.elapsed();
    let evaluations = u128::from(iterations) * u128::from(field.strands());
    let mut report = Report::new();
    describe_field(&mut report, &field, args.ordering);
    report.num("iterations", iterations);
    report.num("evaluations", evaluations);
    report.num("elapsed_ns", elapsed.as_nanos());
    report.float("ns_per_evaluation", elapsed.as_nanos() as f64 / evaluations as f64);
    report.float("checksum", checksum);
    report.print(args.json);
    Ok(())
}

#[derive(Clone, Copy)]
enum Mode {
    Scalar,
    Batch,
    Parallel,
}

fn cmd_bench_exec(args: &Args) -> Result<(), String> {
    let field = field_of(args)?;
    let iterations: u64 = positional(args, 0, "iterations")?;
    let batch: usize = positional(args, 1, "batch")?;
    let workers: usize = positional(args, 2, "workers")?;
    if iterations == 0 || batch == 0 || workers == 0 {
        return Err("iterations, batch, and workers must be greater than zero".into());
    }
    let evaluations = u128::from(iterations) * u128::from(field.strands());
    println!("mode,requested_workers,effective_workers_min,effective_workers_max,evaluations,elapsed_ns,ns_per_evaluation,checksum");
    for (mode, label) in [
        (Mode::Scalar, "scalar"),
        (Mode::Batch, "batch"),
        (Mode::Parallel, "parallel-batch"),
    ] {
        let start = Instant::now();
        let mut checksum = 0.0_f64;
        let mut min_workers = usize::MAX;
        let mut max_workers = 0;
        let mut jobs = Vec::with_capacity(batch);
        let mut flush = |jobs: &mut Vec<Job>| -> Result<(), String> {
            if jobs.is_empty() {
                return Ok(());
            }
            let effective = match mode {
                Mode::Parallel => effective_workers(jobs.len(), workers).map_err(|e| e.to_string())?,
                _ => 1,
            };
            min_workers = min_workers.min(effective);
            max_workers = max_workers.max(effective);
            let samples = match mode {
                Mode::Scalar => jobs
                    .iter()
                    .map(|job| sample(&field, args.ordering, *job))
                    .collect::<Result<Vec<_>, _>>(),
                Mode::Batch => run(&field, args.ordering, jobs),
                Mode::Parallel => run_parallel(&field, args.ordering, jobs, workers),
            }
            .map_err(|e| e.to_string())?;
            checksum += samples.iter().map(|s| s.bearing + s.pos.x + s.pos.y).sum::<f64>();
            jobs.clear();
            Ok(())
        };
        for iteration in 0..iterations {
            for ordinal in 0..field.strands() {
                jobs.push(Job {
                    ordinal,
                    progress: ((iteration + u64::from(ordinal)) % 1_001) as f64 / 1_000.0,
                });
                if jobs.len() == batch {
                    flush(&mut jobs)?;
                }
            }
        }
        flush(&mut jobs)?;
        black_box(checksum);
        let elapsed = start.elapsed();
        let requested = match mode {
            Mode::Parallel => workers,
            _ => 1,
        };
        println!(
            "{label},{requested},{min_workers},{max_workers},{evaluations},{},{:.6},{checksum:.17e}",
            elapsed.as_nanos(),
            elapsed.as_nanos() as f64 / evaluations as f64
        );
    }
    Ok(())
}

fn cmd_sweep() -> Result<(), String> {
    let cardinalities = [1_u64 << 16, 1 << 20, 1 << 24, 1 << 28, 1 << 32];
    let strand_counts = [64_u32, 256, 1_024, 4_096, 16_384, 65_536, 262_144];
    println!("cardinality,strands,tracers,elapsed_ns,radius_error,antipodal_error,nexus_error,gap_error,eversion_sign_ok,tracer_coverage");
    for cardinality in cardinalities {
        for strands in strand_counts {
            if u64::from(strands) > cardinality {
                continue;
            }
            let field = FieldSpec {
                cardinality,
                strands,
                tracers: strands.min(1_024),
                ..FieldSpec::default()
            }
            .build()
            .map_err(|e| e.to_string())?;
            let start = Instant::now();
            let a = audit(&field, Ordering::Uniform).map_err(|e| e.to_string())?;
            let elapsed = start.elapsed().as_nanos();
            println!(
                "{cardinality},{strands},{},{elapsed},{:.17e},{:.17e},{:.17e},{},{},{:.17}",
                field.tracers(),
                a.radius_error,
                a.antipodal_error,
                a.nexus_error,
                a.gap_error,
                a.eversion_sign_ok(),
                a.tracer_coverage()
            );
        }
    }
    Ok(())
}

fn cmd_atlas(args: &Args) -> Result<(), String> {
    let field = field_of(args)?;
    let samples: u64 = positional(args, 0, "samples")?;
    let rounds = args.rounds.unwrap_or(16);
    let start = Instant::now();
    let census = canonical_census(&field, args.ordering, samples, rounds).map_err(|e| e.to_string())?;
    let elapsed = start.elapsed();
    census.validate().map_err(|e| e.to_string())?;
    let (initial_cv, final_cv, family_cv, quadrant_cv) = census.balance();
    let mut report = Report::new();
    describe_field(&mut report, &field, args.ordering);
    report.num("samples", census.count);
    report.num("walk_rounds", rounds);
    report.num("elapsed_ns", elapsed.as_nanos());
    report.num("nexus_samples", census.nexus);
    report.num("classified_samples", census.count - census.nexus);
    report.num("coherence_count", census.coherence);
    report.float("coherence_ratio", census.coherence_ratio());
    report.float("initial_direction_cv", initial_cv);
    report.float("final_direction_cv", final_cv);
    report.float("final_family_cv", family_cv);
    report.float("final_quadrant_cv", quadrant_cv);
    for (i, label) in ["xy", "xz", "yz"].iter().enumerate() {
        report.num(&format!("final_family_{label}"), census.families[i]);
    }
    for (i, value) in census.quadrants.iter().enumerate() {
        report.num(&format!("final_quadrant_{i}"), *value);
    }
    for (i, value) in census.initial.iter().enumerate() {
        report.num(&format!("initial_direction_{i}"), *value);
    }
    for (i, value) in census.final_state.iter().enumerate() {
        report.num(&format!("final_direction_{i}"), *value);
    }
    report.flag("quantum_claim", false);
    report.print(args.json);
    Ok(())
}

fn lattice_common(report: &mut Report, cells: u32, rounds: u32, seed: u32, workload: &lattice::Workload) {
    report.num("cells", cells);
    report.num("rounds", rounds);
    report.num("seed", seed);
    report.num("edge_gates", workload.edge_gates);
    report.num("face_swaps", workload.face_swaps);
    report.num("configuration_bits", workload.configuration_bits);
    report.num("cpu_state_bytes", workload.cpu_state_bytes);
    report.num("gpu_model_resident_bytes", workload.gpu_resident_bytes);
}

fn census_rows(report: &mut Report, prefix: &str, census: &lattice::Census, cells: u32) {
    report.float(&format!("{prefix}_total_cv"), census.total_cv());
    report.float(&format!("{prefix}_vertex_state_cv"), census.vertex_state_cv());
    report.float(
        &format!("{prefix}_edge_disagreement_ratio"),
        census.disagreement_ratio(cells),
    );
    for (i, value) in census.excited.iter().enumerate() {
        report.num(&format!("{prefix}_face_{i}_excited"), *value);
    }
    report.text(&format!("{prefix}_fold"), census.fold_hex());
}

fn cmd_lattice_verify(args: &Args) -> Result<(), String> {
    let cells: u32 = match args.positional.first() {
        Some(_) => positional(args, 0, "cells")?,
        None => 1_024,
    };
    let rounds = args.rounds.unwrap_or(16);
    let seed = args.seed.unwrap_or(DEFAULT_SEED);
    let v = lattice::verify(cells, rounds, seed).map_err(|e| e.to_string())?;
    v.initial.validate(cells).map_err(|e| e.to_string())?;
    v.final_state.validate(cells).map_err(|e| e.to_string())?;
    let mut report = Report::new();
    report.text("engine", "cpu-reference");
    lattice_common(&mut report, cells, rounds, seed, &v.workload);
    report.num("elapsed_ns", v.elapsed.as_nanos());
    report.num("registers_verified", u64::from(cells) * 12);
    report.num("restoration_mismatches", v.restoration_mismatches);
    census_rows(&mut report, "initial", &v.initial, cells);
    census_rows(&mut report, "final", &v.final_state, cells);
    report.flag("fold_kind_cryptographic", false);
    report.flag("quantum_claim", false);
    report.flag("passed", v.passed());
    report.print(args.json);
    if v.passed() {
        Ok(())
    } else {
        Err("lattice verification failed".into())
    }
}

fn cmd_lattice_run(args: &Args) -> Result<(), String> {
    let cells: u32 = positional(args, 0, "cells")?;
    let rounds: u32 = positional(args, 1, "rounds")?;
    let seed = args.seed.unwrap_or(DEFAULT_SEED);
    let r = lattice::run(cells, rounds, seed).map_err(|e| e.to_string())?;
    let mut report = Report::new();
    report.text("engine", "cpu-reference");
    lattice_common(&mut report, cells, rounds, seed, &r.workload);
    report.num("elapsed_ns", r.elapsed.as_nanos());
    let cell_rounds = u64::from(cells) * u64::from(rounds);
    report.float("ns_per_cell_round", r.elapsed.as_nanos() as f64 / cell_rounds as f64);
    report.float(
        "edge_gates_per_second",
        r.workload.edge_gates as f64 / r.elapsed.as_secs_f64().max(f64::MIN_POSITIVE),
    );
    census_rows(&mut report, "initial", &r.initial, cells);
    census_rows(&mut report, "final", &r.final_state, cells);
    report.flag("fold_kind_cryptographic", false);
    report.flag("quantum_claim", false);
    report.print(args.json);
    Ok(())
}
