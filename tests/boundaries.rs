// SPDX-License-Identifier: MPL-2.0
use nexus::ve::{
    atlas::AtlasCensus,
    lattice::{increment, partner, Lattice},
};
use nexus::{audit, event, sample, EventMap, FieldSpec, Job, Ordering};

#[test]
fn overflowing_transit_angles_are_rejected_before_audit() {
    for (phase, twist) in [(0.0, 1e308), (0.0, -1e308), (1.7e308, 1e307)] {
        assert!(FieldSpec {
            phase,
            twist,
            ..FieldSpec::default()
        }
        .build()
        .is_err());
    }
    let field = FieldSpec {
        twist: 1e300,
        ..FieldSpec::default()
    }
    .build()
    .unwrap();
    assert_eq!(audit(&field, Ordering::Uniform).unwrap().endpoint_error, 0.0);
    for p in [0.0, 0.25, 0.5, 0.75, 1.0] {
        let s = sample(
            &field,
            Ordering::Uniform,
            Job {
                ordinal: 0,
                progress: p,
            },
        )
        .unwrap();
        assert!(s.pos.x.is_finite() && s.pos.y.is_finite());
    }
}

#[test]
fn audio_maps_reject_invalid_tables_and_frequency_underflow() {
    let field = FieldSpec::default().build().unwrap();
    let sample = sample(
        &field,
        Ordering::Uniform,
        Job {
            ordinal: 0,
            progress: 0.49,
        },
    )
    .unwrap();
    for gain_floor in [-0.1, 1.1, f64::NAN, f64::INFINITY] {
        assert!(event(
            &field,
            &EventMap {
                gain_floor,
                ..EventMap::default()
            },
            &sample
        )
        .is_err());
    }
    for offset in [f64::NAN, f64::INFINITY, -20000.0] {
        assert!(event(
            &field,
            &EventMap {
                lane_semitones: [offset; 3],
                ..EventMap::default()
            },
            &sample
        )
        .is_err());
    }
    for gain_floor in [0.0, 1.0] {
        let e = event(
            &field,
            &EventMap {
                gain_floor,
                ..EventMap::default()
            },
            &sample,
        )
        .unwrap();
        assert!((0.0..=1.0).contains(&e.gain) && e.hz > 0.0);
    }
}

#[test]
fn lattice_census_rejects_contradictory_totals_and_overflow() {
    let mut census = Lattice::seeded(2, 0).unwrap().census();
    census.validate(2).unwrap();
    census.totals = [24, 0, 0, 0];
    assert!(census.validate(2).is_err());
    census.vertex_state = [u64::MAX; 48];
    assert!(census.validate(2).is_err());
}

#[test]
fn atlas_census_rejects_underflow_and_inconsistent_marginals() {
    let mut census = AtlasCensus {
        nexus: 1,
        ..AtlasCensus::default()
    };
    assert!(census.validate().is_err());
    assert_eq!(census.coherence_ratio(), 0.0);
    census = AtlasCensus {
        count: 1,
        ..AtlasCensus::default()
    };
    census.initial[0] = 1;
    census.final_state[0] = 1;
    census.families[1] = 1;
    census.quadrants[0] = 1;
    assert!(census.validate().is_err());
    census.families = [1, 0, 0];
    census.validate().unwrap();
    census.quadrants = [0, 1, 0, 0];
    assert!(census.validate().is_err());
    census.initial = [u64::MAX; 12];
    assert!(census.validate().is_err());
}

#[test]
fn large_ring_partners_remain_in_range_and_symmetric() {
    let cells = u32::MAX - 1;
    for round in [0, 1, u32::MAX] {
        for cell in [0, 1, cells / 2, cells - 2, cells - 1] {
            let p = partner(cells, round, cell);
            assert!(p < cells && p != cell);
            assert_eq!(partner(cells, round, p), cell);
        }
    }
    let expected = (1 + (3u32.wrapping_add(2).wrapping_add(23).wrapping_add(cells - 1)) % 3) as u8;
    assert_eq!(increment(3, 2, 23, cells - 1), expected);
}
