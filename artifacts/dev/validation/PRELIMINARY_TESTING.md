# WTG Mixed-Adapter Preliminary Testing

This document tracks preliminary findings, edge cases, and potential fixes discovered while exercising WTG against mixed-vendor, mixed-generation, and multi-adapter GPU configurations.

This is an exploratory engineering log, not the final validation matrix or test plan. A structured mixed-adapter test methodology will be developed separately after preliminary hardware exploration is complete.

The intent is to separate WTG defects, validation-harness defects, provider/runtime limitations, Windows driver behavior, and hardware or firmware behavior. A test failure does not automatically imply a WTG defect.

## Status Legend

- **Observed**: Behavior reproduced and captured.
- **Potential WTG defect**: Evidence suggests WTG behavior may need correction.
- **Harness defect**: Validation logic does not match legitimate application behavior.
- **External behavior**: Driver, runtime, firmware, or operating-system behavior outside WTG.
- **Needs reproduction**: More hardware or driver testing required.
- **Candidate fix**: Potential implementation change identified but not yet accepted.

## Case 1: GT 1030 + RTX 3060 Ti + Intel UHD 730

### Configuration

Host: `INSPIRON3861`, Dell Inspiron 3891.

Adapters observed by Windows:

- NVIDIA GeForce GT 1030
- NVIDIA GeForce RTX 3060 Ti
- Intel UHD Graphics 730

The GT 1030 was bound to NVIDIA driver 582.42 while the RTX 3060 Ti retained a different installed NVIDIA driver version.

### Observed behavior

Windows and ADL topology could see both NVIDIA adapters. The NVIDIA runtime exposed only the GT 1030. `nvidia-smi` and WTG NVML enumeration agreed on the NVML-visible device set.

ADL continued to expose the RTX 3060 Ti as topology even though the NVIDIA runtime did not expose it through NVML.

### Classification

**External behavior**

The evidence does not currently indicate a WTG defect. WTG agrees with NVIDIA-SMI about the NVML-visible device set.

### Potential interpretation

The NVIDIA driver stack appears unable to expose both adapters correctly in this mixed-driver/mixed-generation state. This demonstrates an important distinction between PCI/Windows enumeration, provider-independent topology, and vendor runtime enumeration.

A GPU can remain visible to Windows and ADL topology while being absent from NVML.

### Potential WTG action

None currently. Preserve this configuration as a useful negative-control case for provider/runtime visibility.

## Case 2: Dual GT 730 + Intel UHD 730

### Configuration

Two NVIDIA adapters reported the same NVIDIA device ID, `VEN_10DE&DEV_1287`, but different subsystem IDs:

- `SUBSYS_3E561642`
- `SUBSYS_10831028`

One GT 730 successfully bound to NVIDIA driver 475.14. The second adapter became `Microsoft Basic Display Adapter`.

Windows Device Manager reported Code 10:

> This device cannot start. (Code 10)
>
> The driver trying to start is not the same as the driver for the POSTed display adapter.

ADL still detected both physical NVIDIA adapters plus the Intel UHD 730.

### Classification

**External behavior**

This weakens the hypothesis that the earlier failure was simply caused by mixing NVIDIA GPU generations. Two adapters with the same NVIDIA device ID can still fail to bind simultaneously.

### Potential interpretation

Possible factors include POSTed display adapter selection, VBIOS differences, subsystem/vendor differences, legacy NVIDIA driver behavior, Windows WDDM initialization, or platform PCIe initialization.

No WTG fix is currently indicated.

### Follow-up

Test additional duplicate or near-duplicate NVIDIA adapters where practical. Record device ID, subsystem ID, VBIOS, PCI bus, POSTed adapter, driver version, Device Manager status, NVIDIA-SMI visibility, and WTG visibility.

## Case 3: AMD-Absent Harness Assumption

### Configuration

No AMD GPU was installed. ADL nevertheless initialized successfully and returned topology records for NVIDIA adapters and the Intel UHD 730.

WTG reported zero AMD physical adapters while successfully returning non-AMD topology.

### Observed validation failure

The validation harness expected `Provider status: unavailable` and a non-success condition when AMD hardware was absent.

Instead, the AMD ADL provider legitimately succeeded because ADL itself was available and returned cross-vendor topology.

### Classification

**Harness defect**

The harness currently conflates an unavailable AMD provider with an available AMD provider that has zero AMD physical adapters. These are different states.

### Candidate fix

Change AMD validation logic to accept successful provider execution when `AMD physical adapters: 0`, provided that the output is structurally valid.

Suggested expected states:

- ADL unavailable
- ADL available with zero AMD physical adapters
- ADL available with one or more AMD physical adapters

ADLX availability should continue to be evaluated independently.

### Status

**Candidate fix**

Do not modify until additional mixed-adapter cases confirm the expected behavior.

## Case 4: GT 730 Partial NVML Support Causes Whole-Device Failure

### Configuration

NVIDIA GeForce GT 730 using NVIDIA driver 475.14.

NVIDIA-SMI successfully enumerated the GPU but exposed limited telemetry. Temperature was reported as 0 C, while power and GPU utilization were unavailable.

WTG successfully reached NVML device 0 but failed while requesting utilization:

```text
Provider status: error
Reason: all NVIDIA device samples failed

NVML device 0: unavailable
  Reason: utilization_rates(0) failed: the requested operation is not available on the target device
```

### Classification

**Potential WTG defect**

The GPU exists and NVML can enumerate it. One unsupported telemetry function currently invalidates the entire WTG device sample.

### Potential desired behavior

Instead of making the entire NVML device unavailable, WTG should potentially return all telemetry that NVML can provide and degrade unsupported metrics individually.

Conceptually:

```text
NVML device 0: NVIDIA GeForce GT 730
  UUID: <available>
  Temp: unavailable
  Util: unavailable
  VRAM: <available>
  Power: unavailable
```

### Candidate fix

Review NVIDIA collection code for all-or-nothing sampling behavior. Where practical:

- device enumeration failure -> device unavailable
- individual metric unsupported -> metric unavailable
- individual metric transient error -> metric error/unavailable

A single unsupported metric should not necessarily invalidate the rest of the device.

### Follow-up

Test other legacy NVIDIA GPUs individually, especially GeForce 210, GT 730, GTX 745, and other pre-Pascal hardware available in the test pool.

Determine whether unsupported utilization, temperature, or power telemetry produces the same whole-device failure.

### Status

**Needs reproduction**

If reproduced across additional legacy devices, promote this to a defined v0.3.1 compatibility fix.

## Mixed-Adapter Test Matrix

| Case | Configuration | Provider/Layer | Result | Classification |
| --- | --- | --- | --- | --- |
| 1 | GT 1030 + RTX 3060 Ti + UHD 730 | NVML | Only GT 1030 visible | External behavior |
| 1 | GT 1030 + RTX 3060 Ti + UHD 730 | ADL topology | All adapters visible | Expected |
| 2 | GT 730 + GT 730 + UHD 730 | Windows | One GT 730 Code 10 | External behavior |
| 2 | GT 730 + GT 730 + UHD 730 | ADL topology | Both NVIDIA adapters visible | Expected |
| 3 | No AMD GPU | AMD ADL | Successful topology-only result | Expected |
| 3 | No AMD GPU | Validation harness | Reported FAIL | Harness defect |
| 4 | GT 730 | NVML | Device discarded after unsupported utilization call | Potential WTG defect |

## Findings Queue

Add new cases below as preliminary testing continues.

### Case Template

#### Configuration

```text
Host:
GPU 1:
GPU 2:
Integrated GPU:
Driver versions:
PCIe layout:
```

#### Windows state

```text
Device Manager:
CIM:
Driver binding:
```

#### Vendor runtime state

```text
nvidia-smi:
AMD runtime:
Intel runtime:
```

#### WTG behavior

```text
Command:
Exit code:
Provider:
Observed output:
```

#### Classification

Choose one or more as appropriate:

- WTG defect
- Harness defect
- External behavior
- Expected behavior
- Needs reproduction

#### Potential fix

TBD.

#### Evidence

```text
Artifact names:
Relevant hashes:
Notes:
```

#### Status

`Observed / Needs reproduction / Candidate fix / Resolved`
