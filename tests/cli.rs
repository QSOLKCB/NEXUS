// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::process::{Command, Output};

fn nexus(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nexus"))
        .args(args)
        .output()
        .expect("nexus binary should run")
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("UTF-8 output")
}

fn metric<'a>(text: &'a str, key: &str) -> &'a str {
    text.lines()
        .find_map(|line| line.strip_prefix(&format!("{key},")))
        .unwrap_or_else(|| panic!("metric {key} missing"))
}

#[test]
fn audit_reports_exact_centre_and_partition() {
    let output = nexus(&["audit"]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.starts_with("metric,value\n"));
    assert_eq!(metric(&text, "nexus_error"), "0.00000000000000000e0");
    assert_eq!(metric(&text, "antipodal_error"), "0.00000000000000000e0");
    assert_eq!(metric(&text, "endpoint_error"), "0.00000000000000000e0");
    assert_eq!(metric(&text, "gap_error"), "0");
    assert_eq!(metric(&text, "eversion_sign_ok"), "true");
}

#[test]
fn parallel_audit_matches_sequential_audit_output() {
    let sequential = stdout(&nexus(&["audit"]));
    let parallel = stdout(&nexus(&["audit", "--workers", "3"]));
    let strip = |text: &str| -> Vec<String> {
        text.lines()
            .filter(|l| !l.starts_with("elapsed_ns") && !l.contains("workers"))
            .map(str::to_string)
            .collect()
    };
    assert_eq!(strip(&sequential), strip(&parallel));
}

#[test]
fn json_output_is_well_formed() {
    let output = nexus(&["audit", "--json", "--strands", "64"]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert!(text.trim_start().starts_with('{'));
    assert!(text.trim_end().ends_with('}'));
    assert!(text.contains("\"ordering\": \"uniform\""));
    assert!(text.contains("\"strands\": 64"));
}

#[test]
fn ledger_is_identical_across_processes() {
    let first = nexus(&["ledger", "777"]);
    let second = nexus(&["ledger", "777"]);
    assert!(first.status.success() && second.status.success());
    let strip = |o: &Output| -> Vec<String> {
        stdout(o)
            .lines()
            .filter(|l| !l.starts_with("elapsed_ns"))
            .map(str::to_string)
            .collect()
    };
    assert_eq!(strip(&first), strip(&second));
    let text = stdout(&first);
    assert_eq!(metric(&text, "samples"), "777");
    assert_eq!(metric(&text, "root_sha256").len(), 64);
    for lane in ["lane-0", "lane-1", "lane-2"] {
        assert_eq!(metric(&text, &format!("{lane}_sha256")).len(), 64);
    }
}

#[test]
fn golden_ledger_differs_from_uniform_ledger() {
    let uniform = stdout(&nexus(&["ledger", "500"]));
    let golden = stdout(&nexus(&["ledger", "500", "--ordering", "golden"]));
    assert_ne!(metric(&uniform, "root_sha256"), metric(&golden, "root_sha256"));
}

#[test]
fn trace_passes_through_the_exact_origin() {
    let output = nexus(&["trace", "5", "4"]);
    assert!(output.status.success());
    let text = stdout(&output);
    let centre = text.lines().nth(3).unwrap();
    assert!(centre.starts_with("2,0.50000000000000000,0.00000000000000000,0.00000000000000000,"));
    assert!(centre.contains(",nexus,0,lane-2,"));
}

#[test]
fn execution_modes_share_one_checksum() {
    let output = nexus(&["bench-exec", "2", "512", "3", "--strands", "256"]);
    assert!(output.status.success());
    let text = stdout(&output);
    let checksums: Vec<&str> = text.lines().skip(1).map(|l| l.rsplit(',').next().unwrap()).collect();
    assert_eq!(checksums.len(), 3);
    assert!(checksums.windows(2).all(|w| w[0] == w[1]));
    assert!(text.lines().any(|l| l.starts_with("parallel-batch,3,")));
}

#[test]
fn lattice_verification_restores_every_register() {
    let output = nexus(&["lattice-verify", "256", "--rounds", "12"]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert_eq!(metric(&text, "restoration_mismatches"), "0");
    assert_eq!(metric(&text, "passed"), "true");
    assert_ne!(metric(&text, "initial_fold"), metric(&text, "final_fold"));
}

#[test]
fn atlas_census_is_consistent() {
    let output = nexus(&["atlas", "5005", "--rounds", "5"]);
    assert!(output.status.success());
    let text = stdout(&output);
    assert_eq!(metric(&text, "samples"), "5005");
    assert_eq!(metric(&text, "quantum_claim"), "false");
}

#[test]
fn invalid_input_fails_with_context() {
    let output = nexus(&["audit", "--cardinality", "0"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("cardinality must be greater than zero"));

    let output = nexus(&["lattice-verify", "3"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("even"));
}

#[test]
fn ignored_arguments_cannot_silently_change_an_experiment() {
    for args in [
        vec!["lattice-verify", "2", "64"],
        vec!["lattice-run", "2", "1", "--rounds", "64"],
        vec!["audit", "--seed", "7"],
        vec!["sweep", "--strands", "2"],
    ] {
        assert!(!nexus(&args).status.success(), "{args:?}");
    }
}

#[test]
fn nonfinite_benchmark_checksum_is_an_error_not_invalid_json() {
    let output = nexus(&["bench", "1", "--strands", "2", "--phase", "1e308", "--json"]);
    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .contains("checksum overflowed"));
}
