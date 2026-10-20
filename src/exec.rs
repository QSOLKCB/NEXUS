// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Scalar, sequential, and parallel evaluation of transit jobs.
//!
//! The parallel path hands fixed-size blocks of jobs to standard-library
//! scoped threads through one atomic cursor. Each completed block is tagged
//! with its starting offset and the blocks are stitched back together in
//! input order, so the parallel result is byte-identical to the sequential
//! result regardless of scheduling. No external runtime is used.

#[cfg(not(target_family = "wasm"))]
use std::sync::atomic::{AtomicUsize, Ordering as Atomic};
#[cfg(not(target_family = "wasm"))]
use std::sync::Mutex;

use crate::error::{Error, Result};
use crate::field::{Field, Ordering, Strand};
use crate::ternary::{Lane, Trit};
use crate::transit::{position_signed, signed, Pos};

/// Upper bound on worker threads NEXUS will spawn.
pub const MAX_WORKERS: usize = 256;

/// One evaluation request.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Job {
    /// Strand ordinal.
    pub ordinal: u32,
    /// Progress in `[0, 1]`.
    pub progress: f64,
}

/// One evaluated job with every derived quantity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    /// Strand ordinal.
    pub ordinal: u32,
    /// Exact logical index of the strand.
    pub logical: u64,
    /// Requested progress.
    pub progress: f64,
    /// Strand bearing in radians.
    pub bearing: f64,
    /// Position on the transit path.
    pub pos: Pos,
    /// Eversion sign (signed progress).
    pub eversion: f64,
    /// Transit leg classification.
    pub trit: Trit,
    /// Execution lane of the strand.
    pub lane: Lane,
    /// Ordering that selected the strand.
    pub ordering: Ordering,
}

impl Default for Sample {
    /// A placeholder: strand zero at the exact nexus.
    fn default() -> Self {
        Self {
            ordinal: 0,
            logical: 0,
            progress: 0.5,
            bearing: 0.0,
            pos: Pos::ORIGIN,
            eversion: 0.0,
            trit: Trit::Nexus,
            lane: Lane::of(0),
            ordering: Ordering::Uniform,
        }
    }
}

/// Evaluate one job against a known strand.
pub fn sample_strand(field: &Field, strand: &Strand, ordering: Ordering, progress: f64) -> Result<Sample> {
    let s = signed(progress)?;
    Ok(Sample {
        ordinal: strand.ordinal,
        logical: strand.logical,
        progress,
        bearing: strand.bearing,
        pos: position_signed(field, strand, s),
        eversion: s,
        trit: Trit::from_signed(s),
        lane: Lane::of(strand.ordinal),
        ordering,
    })
}

/// Evaluate one job, resolving the strand on the fly.
pub fn sample(field: &Field, ordering: Ordering, job: Job) -> Result<Sample> {
    let strand = field.strand(job.ordinal, ordering)?;
    sample_strand(field, &strand, ordering, job.progress)
}

/// Evaluate jobs sequentially in input order.
pub fn run(field: &Field, ordering: Ordering, jobs: &[Job]) -> Result<Vec<Sample>> {
    jobs.iter().map(|job| sample(field, ordering, *job)).collect()
}

/// Number of workers that will actually be used for `jobs` jobs.
///
/// Zero requested workers is an error. Otherwise the count is clamped to
/// `[1, min(jobs, MAX_WORKERS)]`. On WebAssembly the answer is always one.
pub fn effective_workers(jobs: usize, requested: usize) -> Result<usize> {
    if requested == 0 {
        return Err(Error::Zero("workers"));
    }
    if cfg!(target_family = "wasm") {
        return Ok(1);
    }
    Ok(requested.min(MAX_WORKERS).min(jobs).max(1))
}

/// Evaluate jobs with up to `workers` threads, returning input order.
pub fn run_parallel(field: &Field, ordering: Ordering, jobs: &[Job], workers: usize) -> Result<Vec<Sample>> {
    let workers = effective_workers(jobs.len(), workers)?;
    if workers == 1 || jobs.len() < 2 {
        return run(field, ordering, jobs);
    }
    scatter_gather(jobs, workers, |chunk, out| {
        fill(out, chunk, |job| sample(field, ordering, job))
    })
}

/// Evaluate `jobs` into `out` slot by slot.
fn fill(out: &mut [Sample], jobs: &[Job], mut evaluate: impl FnMut(Job) -> Result<Sample>) -> Result<()> {
    for (slot, job) in out.iter_mut().zip(jobs) {
        *slot = evaluate(*job)?;
    }
    Ok(())
}

/// Pre-resolved strands for repeated sweeps over the same field.
///
/// Memory is `O(N)`; the transit formula, ordering, and result encoding are
/// unchanged relative to the uncached entry points.
#[derive(Debug, Clone)]
pub struct StrandTable {
    field: Field,
    ordering: Ordering,
    strands: Vec<Strand>,
}

impl StrandTable {
    /// Resolve every strand once.
    pub fn new(field: Field, ordering: Ordering) -> Self {
        let strands = field.iter(ordering).collect();
        Self {
            field,
            ordering,
            strands,
        }
    }

    /// The field this table was built from.
    pub const fn field(&self) -> &Field {
        &self.field
    }

    /// The ordering used.
    pub const fn ordering(&self) -> Ordering {
        self.ordering
    }

    /// Resolved strands in ordinal order.
    pub fn strands(&self) -> &[Strand] {
        &self.strands
    }

    /// Strand by ordinal.
    pub fn strand(&self, ordinal: u32) -> Result<&Strand> {
        self.strands.get(ordinal as usize).ok_or(Error::Ordinal {
            kind: "strand",
            value: u64::from(ordinal),
            bound: u64::from(self.field.strands()),
        })
    }

    /// Evaluate one job from the table.
    pub fn sample(&self, job: Job) -> Result<Sample> {
        let strand = self.strand(job.ordinal)?;
        sample_strand(&self.field, strand, self.ordering, job.progress)
    }

    /// Evaluate jobs sequentially from the table.
    pub fn run(&self, jobs: &[Job]) -> Result<Vec<Sample>> {
        jobs.iter().map(|job| self.sample(*job)).collect()
    }

    /// Evaluate jobs in parallel from the table, in input order.
    pub fn run_parallel(&self, jobs: &[Job], workers: usize) -> Result<Vec<Sample>> {
        let workers = effective_workers(jobs.len(), workers)?;
        if workers == 1 || jobs.len() < 2 {
            return self.run(jobs);
        }
        scatter_gather(jobs, workers, |chunk, out| fill(out, chunk, |job| self.sample(job)))
    }
}

/// Block size chosen so each worker sees several blocks.
#[cfg(not(target_family = "wasm"))]
fn block_size(jobs: usize, workers: usize) -> usize {
    let target_blocks = workers.saturating_mul(4).max(1);
    jobs.div_ceil(target_blocks).max(1)
}

#[cfg(not(target_family = "wasm"))]
fn scatter_gather<T, F>(jobs: &[Job], workers: usize, evaluate: F) -> Result<Vec<T>>
where
    T: Default + Copy + Send,
    F: Fn(&[Job], &mut [T]) -> Result<()> + Sync,
{
    let block = block_size(jobs.len(), workers);
    let mut out = vec![T::default(); jobs.len()];
    // Workers claim (input block, output block) pairs from one shared queue
    // and write results in place, so no gather copy is needed afterwards and
    // the output order is the input order by construction.
    let queue: Mutex<std::vec::IntoIter<(&[Job], &mut [T])>> = Mutex::new(
        jobs.chunks(block)
            .zip(out.chunks_mut(block))
            .collect::<Vec<_>>()
            .into_iter(),
    );
    let failure: Mutex<Option<Error>> = Mutex::new(None);
    let claimed = AtomicUsize::new(0);

    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let next = queue.lock().unwrap_or_else(|p| p.into_inner()).next();
                let Some((input, output)) = next else { break };
                claimed.fetch_add(1, Atomic::Relaxed);
                if let Err(error) = evaluate(input, output) {
                    *failure.lock().unwrap_or_else(|p| p.into_inner()) = Some(error);
                    break;
                }
            });
        }
    });

    if let Some(error) = failure.into_inner().map_err(|_| Error::Worker)? {
        return Err(error);
    }
    if claimed.load(Atomic::Relaxed) != jobs.len().div_ceil(block) {
        return Err(Error::Worker);
    }
    Ok(out)
}

#[cfg(target_family = "wasm")]
fn scatter_gather<T, F>(jobs: &[Job], _workers: usize, evaluate: F) -> Result<Vec<T>>
where
    T: Default + Copy,
    F: Fn(&[Job], &mut [T]) -> Result<()>,
{
    let mut out = vec![T::default(); jobs.len()];
    evaluate(jobs, &mut out)?;
    Ok(out)
}

/// Deterministic job stream used by benchmarks and ledgers:
/// ordinal `i mod N`, progress `(i mod 1001) / 1000`.
pub fn canonical_job(field: &Field, index: u64) -> Job {
    Job {
        ordinal: (index % u64::from(field.strands())) as u32,
        progress: (index % 1_001) as f64 / 1_000.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::FieldSpec;

    fn jobs(field: &Field, count: u64) -> Vec<Job> {
        (0..count).map(|i| canonical_job(field, i)).collect()
    }

    #[test]
    fn parallel_matches_sequential_exactly() {
        let field = FieldSpec::default().build().unwrap();
        let jobs = jobs(&field, 5_000);
        let sequential = run(&field, Ordering::Uniform, &jobs).unwrap();
        for workers in [2, 3, 7, 64] {
            let parallel = run_parallel(&field, Ordering::Uniform, &jobs, workers).unwrap();
            assert_eq!(sequential, parallel);
        }
    }

    #[test]
    fn table_matches_direct_evaluation() {
        let field = FieldSpec::default().build().unwrap();
        let jobs = jobs(&field, 3_003);
        for ordering in [Ordering::Uniform, Ordering::Golden] {
            let direct = run(&field, ordering, &jobs).unwrap();
            let table = StrandTable::new(field, ordering);
            assert_eq!(table.run(&jobs).unwrap(), direct);
            assert_eq!(table.run_parallel(&jobs, 5).unwrap(), direct);
        }
    }

    #[test]
    fn invalid_jobs_fail_in_parallel_too() {
        let field = FieldSpec::default().build().unwrap();
        let mut jobs = jobs(&field, 100);
        jobs[57].progress = 2.0;
        assert!(matches!(
            run_parallel(&field, Ordering::Uniform, &jobs, 4),
            Err(Error::Progress(_))
        ));
        jobs[57].progress = 0.5;
        jobs[3].ordinal = u32::MAX;
        assert!(matches!(
            run_parallel(&field, Ordering::Uniform, &jobs, 4),
            Err(Error::Ordinal { .. })
        ));
    }

    #[test]
    fn worker_count_is_bounded() {
        assert!(effective_workers(10, 0).is_err());
        assert_eq!(effective_workers(10, 1_000).unwrap(), 10);
        assert_eq!(effective_workers(100_000, 1_000).unwrap(), MAX_WORKERS);
        assert_eq!(effective_workers(0, 4).unwrap(), 1);
    }
}
