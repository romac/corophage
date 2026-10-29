"""Checks for comparison parsing and coverage accounting (no compiler needed)."""

import unittest

from measure import check_shared_dependencies, comparisons, duration_ns, parse_divan, unsupported_reason


class ComparisonTests(unittest.TestCase):
    def test_duration_units(self):
        for text, expected in [("500 ps", 0.5), ("0 ns", 0), ("1.25 µs", 1250), ("2 ms", 2e6), ("1 s", 1e9)]:
            with self.subTest(text=text):
                self.assertEqual(duration_ns(text), expected)
        with self.assertRaises(ValueError):
            duration_ns("missing")

    def test_divan_tree_keeps_full_case_names_and_metrics(self):
        output = """execution_suite   fastest │ slowest │ median │ mean │ samples │ iters
├─ dispatch                        │         │        │      │         │
│  ├─ payload_size                 │         │        │      │         │
│  │  ╰─ flat                     │         │        │      │         │
│  │     ╰─ 4096  1 µs │ 3 µs │ 2 µs │ 2.1 µs │ 10 │ 100
│  ╰─ catching                     │         │        │      │         │
│     ╰─ single                    │         │        │      │         │
│        ╰─ 0  500 ps │ 2 ns │ 1 ns │ 1.2 ns │ 10 │ 200
╰─ lifecycle                       │         │        │      │         │
   ╰─ suspended_drop               │         │        │      │         │
      ╰─ flat_pending_body         │         │        │      │         │
         ╰─ 64  5 ns │ 9 ns │ 7 ns │ 7.1 ns │ 10 │ 10
"""
        rows = parse_divan(output)
        self.assertEqual([row["name"] for row in rows], [
            "dispatch::payload_size::flat::4096",
            "dispatch::catching::single::0",
            "lifecycle::suspended_drop::flat_pending_body::64",
        ])
        self.assertEqual(rows[0]["median_ns"], 2000)
        self.assertEqual(rows[1]["min_ns"], 0.5)
        self.assertEqual(rows[2]["iterations"], 10)

    def test_rejects_empty_duplicate_and_malformed_metrics(self):
        for output in [
            "header only",
            "├─ case 1 ns │ 2 ns",
            "   ╰─ orphan 1 ns │ 2 ns │ 1 ns │ 1 ns │ 1 │ 1",
            "├─ case 1 ns │ 2 ns │ 1 ns │ 1 ns │ 1 │ 1\n╰─ case 1 ns │ 2 ns │ 1 ns │ 1 ns │ 1 │ 1",
        ]:
            with self.subTest(output=output), self.assertRaises(ValueError):
                parse_divan(output)

    def test_repeated_comparisons_and_zero_baseline(self):
        baseline = [[{"name": "a", "median_ns": n}, {"name": "b", "median_ns": 0}] for n in [10, 20]]
        candidate = [[{"name": "a", "median_ns": n}, {"name": "b", "median_ns": 1}] for n in [5, 10]]
        rows = comparisons(baseline, candidate)
        self.assertEqual(rows[0]["baseline_median_ns"], 15)
        self.assertEqual(rows[0]["candidate_over_baseline"], 0.5)
        self.assertIsNone(rows[1]["candidate_over_baseline"])
        with self.assertRaises(ValueError):
            comparisons(baseline, [candidate[0], []])

    def test_shared_dependency_versions_must_match(self):
        def dependency(name, version):
            return {"name": name, "version": version, "source": "registry+example"}
        before = [dependency("tokio", "1.0.0"), dependency("fauxgen", "0.1.7")]
        check_shared_dependencies(before, [dependency("tokio", "1.0.0")])
        with self.assertRaises(ValueError):
            check_shared_dependencies(before, [dependency("tokio", "1.1.0")])

    def test_only_known_api_gaps_are_unsupported(self):
        for name in [
            "dispatch::catching::nested_forwarding::0",
            "dispatch::invocation::partially_handled::4",
            "lifecycle::cancellation::handled_async::8",
            "scheduling::fairness::short_handled_children::129",
            "scheduling::fairness::peer_first_poll_latency::2",
        ]:
            self.assertIn("v0.7.0", unsupported_reason(name))
        with self.assertRaises(ValueError):
            unsupported_reason("dispatch::payload_size::flat::8")


if __name__ == "__main__":
    unittest.main()
