"""Regressions for false-success evidence and quota overstatement."""
import importlib.util
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

def module(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'scripts' / (name + '.py'))
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod

provenance = module('provenance')
summary = module('summarize')

class EvidenceTests(unittest.TestCase):
    def test_fractional_and_ancestor_quotas_cap_affinity(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'team' / 'job').mkdir(parents=True)
            proc = root / 'membership'
            proc.write_text('0::/team/job\n')
            (root / 'cpu.max').write_text('max 100000\n')
            (root / 'team' / 'cpu.max').write_text('150000 100000\n')
            (root / 'team' / 'job' / 'cpu.max').write_text('400000 100000\n')
            evidence = {}
            self.assertEqual(provenance.cpu_capacity(root, proc, 32, evidence), 1.5)
            self.assertEqual(evidence['affinity_cpus'], 32)
            self.assertEqual(evidence['quota_files'][str(root / 'team' / 'cpu.max')], '150000 100000')
            (root / 'cpu.max').write_text('50000 100000\n')
            self.assertEqual(provenance.cpu_capacity(root, proc, 32), 0.5)

    def test_v1_quota_and_affinity(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'cpu').mkdir()
            (root / 'cpu' / 'cpu.cfs_quota_us').write_text('200000')
            (root / 'cpu' / 'cpu.cfs_period_us').write_text('100000')
            self.assertEqual(provenance.cpu_capacity(root, root / 'absent', 32), 2)
            self.assertEqual(provenance.cpu_capacity(root, root / 'absent', 1), 1)

    def test_failed_or_empty_audits_never_compare_as_success(self):
        with tempfile.TemporaryDirectory() as tmp:
            a, b = Path(tmp) / 'a.csv', Path(tmp) / 'b.csv'
            for p in (a, b):
                p.write_text('')
                p.with_suffix('.exit-status').write_text('0')
            self.assertFalse(summary.parallel_matches(a, b))
            data = 'metric,value\ncardinality,64\nstrands,64\nnexus_error,0\nendpoint_error,0\nantipodal_error,0\ngap_error,0\neversion_sign_ok,true\n'
            a.write_text(data)
            b.write_text(data)
            self.assertTrue(summary.parallel_matches(a, b))
            a.with_suffix('.exit-status').write_text('1')
            self.assertFalse(summary.parallel_matches(a, b))

    def test_failed_and_nonfinite_timings_are_excluded(self):
        with tempfile.TemporaryDirectory() as tmp:
            raw = Path(tmp) / '04-performance' / 'raw'
            raw.mkdir(parents=True)
            for i, (value, code) in enumerate([('100', '0'), ('1', '1'), ('nan', '0'), ('0', '0')]):
                p = raw / f'bench-{i}.csv'
                p.write_text(f'metric,value\nelapsed_ns,{value}\n')
                p.with_suffix('.exit-status').write_text(code)
            rows = summary.performance(Path(tmp))
            self.assertEqual(rows[0]['runs'], 1)
            self.assertEqual(rows[0]['median_elapsed_ns'], '100')

if __name__ == '__main__':
    unittest.main()
