// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Canonical sample encoding and lane-separated SHA-256 ledgers.
//!
//! Every [`Sample`] has one fixed 40-byte little-endian record. A
//! [`Ledger`] carries four streaming digests (one per lane plus the complete
//! ordered stream) and a root digest that binds them together with the
//! sample count. Ledgers prove byte identity of encoded evidence, nothing
//! more.

use crate::error::{Error, Result};
use crate::exec::{canonical_job, sample_strand, Sample, StrandTable};
use crate::field::{Field, Ordering};
use crate::sha256::{hex, Digest, Sha256};
use crate::ternary::Lane;

/// Size of one canonical record in bytes.
pub const RECORD_BYTES: usize = 40;
const RECORD_TAG: u8 = 0x4E;
const LANE_DOMAIN: &[u8] = b"nexus/ledger/lane/v1\n";
const STREAM_DOMAIN: &[u8] = b"nexus/ledger/stream/v1\n";
const ROOT_DOMAIN: &[u8] = b"nexus/ledger/root/v1\n";

/// Encode one sample as a canonical record.
pub fn encode(sample: &Sample) -> [u8; RECORD_BYTES] {
    let mut out = [0u8; RECORD_BYTES];
    out[0] = RECORD_TAG;
    out[1] = sample.trit.value() as u8;
    out[2] = sample.lane.index() as u8;
    out[3] = sample.ordering.code();
    out[4..8].copy_from_slice(&sample.ordinal.to_le_bytes());
    out[8..16].copy_from_slice(&sample.logical.to_le_bytes());
    out[16..24].copy_from_slice(&sample.progress.to_bits().to_le_bytes());
    out[24..32].copy_from_slice(&sample.pos.x.to_bits().to_le_bytes());
    out[32..40].copy_from_slice(&sample.pos.y.to_bits().to_le_bytes());
    out
}

/// Finished ledger digests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ledger {
    /// Digest over each lane's records in stream order.
    pub lanes: [Digest; Lane::COUNT],
    /// Samples absorbed into each lane.
    pub lane_counts: [u64; Lane::COUNT],
    /// Digest over the complete ordered stream.
    pub stream: Digest,
    /// Total samples absorbed.
    pub count: u64,
    /// Root binding lanes, stream, and counts.
    pub root: Digest,
}

impl Ledger {
    /// Bytes encoded into the stream digest.
    pub fn encoded_bytes(&self) -> Result<u64> {
        self.count
            .checked_mul(RECORD_BYTES as u64)
            .ok_or(Error::Lattice("ledger byte count overflow"))
    }

    /// Hex of the root.
    pub fn root_hex(&self) -> String {
        hex(&self.root)
    }

    /// Hex of a lane digest.
    pub fn lane_hex(&self, lane: Lane) -> String {
        hex(&self.lanes[lane.index()])
    }

    /// Hex of the stream digest.
    pub fn stream_hex(&self) -> String {
        hex(&self.stream)
    }
}

/// Streaming ledger builder. Absorb samples in order, then finish.
#[derive(Debug, Clone)]
pub struct LedgerBuilder {
    lanes: [Sha256; Lane::COUNT],
    lane_counts: [u64; Lane::COUNT],
    stream: Sha256,
    count: u64,
}

impl Default for LedgerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl LedgerBuilder {
    /// Empty builder with domain-separated states.
    pub fn new() -> Self {
        let mut lanes = [Sha256::new(), Sha256::new(), Sha256::new()];
        for (index, lane) in lanes.iter_mut().enumerate() {
            lane.update(LANE_DOMAIN);
            lane.update(&[index as u8]);
        }
        let mut stream = Sha256::new();
        stream.update(STREAM_DOMAIN);
        Self {
            lanes,
            lane_counts: [0; Lane::COUNT],
            stream,
            count: 0,
        }
    }

    /// Absorb one sample.
    pub fn absorb(&mut self, sample: &Sample) {
        let record = encode(sample);
        let lane = sample.lane.index();
        self.lanes[lane].update(&record);
        self.lane_counts[lane] += 1;
        self.stream.update(&record);
        self.count += 1;
    }

    /// Absorb many samples in order.
    pub fn absorb_all(&mut self, samples: &[Sample]) {
        for sample in samples {
            self.absorb(sample);
        }
    }

    /// Finish into a [`Ledger`].
    pub fn finish(self) -> Ledger {
        let lanes = [
            self.lanes[0].clone().finish(),
            self.lanes[1].clone().finish(),
            self.lanes[2].clone().finish(),
        ];
        let stream = self.stream.finish();
        let mut root = Sha256::new();
        root.update(ROOT_DOMAIN);
        for (digest, count) in lanes.iter().zip(self.lane_counts) {
            root.update(digest);
            root.update(&count.to_le_bytes());
        }
        root.update(&stream);
        root.update(&self.count.to_le_bytes());
        Ledger {
            lanes,
            lane_counts: self.lane_counts,
            stream,
            count: self.count,
            root: root.finish(),
        }
    }
}

/// Ledger over materialised samples.
pub fn ledger(samples: &[Sample]) -> Ledger {
    let mut builder = LedgerBuilder::new();
    builder.absorb_all(samples);
    builder.finish()
}

/// Ledger over the first `count` canonical jobs of a field, streamed without
/// materialising the sample vector.
pub fn canonical_ledger(field: &Field, ordering: Ordering, count: u64) -> Result<Ledger> {
    if count == 0 {
        return Err(Error::Zero("samples"));
    }
    let table = StrandTable::new(*field, ordering);
    let mut builder = LedgerBuilder::new();
    for index in 0..count {
        let job = canonical_job(field, index);
        let strand = table.strand(job.ordinal)?;
        let sample = sample_strand(field, strand, ordering, job.progress)?;
        builder.absorb(&sample);
    }
    Ok(builder.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::run;
    use crate::field::FieldSpec;

    #[test]
    fn streamed_and_materialised_ledgers_agree() {
        let field = FieldSpec::default().build().unwrap();
        let jobs: Vec<_> = (0..2_500).map(|i| canonical_job(&field, i)).collect();
        let samples = run(&field, Ordering::Uniform, &jobs).unwrap();
        let direct = ledger(&samples);
        let streamed = canonical_ledger(&field, Ordering::Uniform, 2_500).unwrap();
        assert_eq!(direct, streamed);
        assert_eq!(direct.count, 2_500);
        assert_eq!(direct.lane_counts.iter().sum::<u64>(), 2_500);
    }

    #[test]
    fn ledger_detects_a_single_bit_change() {
        let field = FieldSpec::default().build().unwrap();
        let jobs: Vec<_> = (0..300).map(|i| canonical_job(&field, i)).collect();
        let mut samples = run(&field, Ordering::Uniform, &jobs).unwrap();
        let before = ledger(&samples);
        samples[150].pos.x = f64::from_bits(samples[150].pos.x.to_bits() ^ 1);
        let after = ledger(&samples);
        assert_ne!(before.root, after.root);
        assert_ne!(before.stream, after.stream);
        let touched = samples[150].lane.index();
        for lane in 0..3 {
            assert_eq!(before.lanes[lane] != after.lanes[lane], lane == touched);
        }
    }

    #[test]
    fn empty_sample_count_is_rejected() {
        let field = FieldSpec::default().build().unwrap();
        assert!(matches!(
            canonical_ledger(&field, Ordering::Uniform, 0),
            Err(Error::Zero("samples"))
        ));
    }

    #[test]
    fn record_layout_is_fixed() {
        let field = FieldSpec::default().build().unwrap();
        let sample = crate::exec::sample(&field, Ordering::Golden, canonical_job(&field, 17)).unwrap();
        let record = encode(&sample);
        assert_eq!(record[0], RECORD_TAG);
        assert_eq!(record[3], 1);
        assert_eq!(u32::from_le_bytes([record[4], record[5], record[6], record[7]]), 17);
    }
}
