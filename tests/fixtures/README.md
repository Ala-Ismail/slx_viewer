# 1. Fixture manifest

## 1.1 Saved models

The accepted saved SLX reference corpus consists only of `dc_motor_speed_control.slx` and SLX files under `microgrid/`. The repository folder `microgrid` is the micro_grid collection. Other local model files are excluded from reference tests and compatibility evidence. Synthetic archives remain available for isolated parser and failure tests.

| Fixture | Source release | SHA-256 | Provenance and redistribution |
| --- | --- | --- | --- |
| `dc_motor_speed_control.slx` | R2025b, recorded in the package release metadata | `f572a3a7bc3b8ff4a34eb09c8e39eb1464ea33f06a5c85521fb453345e8febf1` | Alpaslan Kilic, Git blob `979a8f5b1307d437239a2215b7172f80f93b4874`. MIT license included. [Upstream model](https://github.com/alpaslankilic/dc-motor-speed-control/blob/main/dc_motor_speed_control.slx), [fixture license](dc_motor_speed_control.LICENSE) |
| `microgrid/pvwindupfc11.slx` | R2016a, recorded in core-properties metadata | `c7558ca19656a4601647430b154092272201a5d4fdd8fdedfa1aab31a79f551b` | Copyright 2017 Venkatesh Kumar M; redistribution terms in [license](microgrid/license.txt), supplied description in [README](microgrid/README.md). Parser and native acceptance checks not run |


## 1.2 Synthetic archives

The integration tests generate their own archives for a three-level hierarchy, reused identifiers and names, embedded and referenced systems, Unicode, annotations, rotation and mirroring flags, stored code, unresolved endpoints and child systems, malformed inputs, and resource-limit boundaries. Synthetic Stateflow parts cover ambiguous bindings, chart-local endpoints, and recoverable chart errors. These archives supply parser expectations independently of the implementation.
