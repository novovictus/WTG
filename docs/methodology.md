# Methodology

This document describes the empirical validation process used to characterize NVML telemetry behavior under Windows WDDM.

## Test Conditions

-   Tests performed on retail hardware with stock Windows driver installations.
-   Idle defined as Windows desktop state with no active compute workloads.
-   Load cases executed using controlled GPU workloads (e.g., LLM inference).
-   Sampling performed via:
    -   `wtg.exe --once`
    -   `wtg.exe --watch`
    -   `nvidia-smi -q`

## Metric of Interest

Memory utilization values reflect `nvmlDeviceGetUtilizationRates().memory` as reported by NVML.

## Constraints

-   WDDM mode only (no TCC testing conducted).
-   No kernel tracing or reverse engineering performed.
-   No attribution of root cause; observations only.
