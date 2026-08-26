# WTG Mixed-Adapter Preliminary Testing

This document tracks preliminary findings, edge cases, and potential remediations discovered while exercising WTG against mixed-vendor, mixed-generation, and multi-adapter GPU configurations.

This is an exploratory engineering log, not the final validation matrix or test plan. A structured mixed-adapter test methodology will be developed separately after preliminary hardware exploration is complete.

The intent is to separate WTG defects, validation-harness defects, provider/runtime limitations, Windows driver behavior, and hardware or firmware behavior. A test failure does not automatically imply a WTG defect.

## Status Legend

- **Observed**: Behavior reproduced and captured.
- **Potential WTG defect**: Evidence suggests WTG behavior may need correction.
- **Harness defect**: Validation logic does not match legitimate application behavior.
- **External behavior**: Driver, runtime, firmware, or operating-system behavior outside WTG.
- **Expected**: Behavior is consistent with provider/runtime capabilities and WTG design.
- **Needs reproduction**: More hardware or driver testing required.
- **Candidate fix**: Potential implementation change identified but not yet accepted.

## Preliminary Remediation Queue

The preliminary testing has identified three WTG or validation-harness behaviors worth carrying forward for remediation review:

1. **NVIDIA per-metric graceful degradation**: An enumerated NVIDIA device should not necessarily be discarded because one NVML metric is unsupported. Review the NVIDIA sampling path so unsupported metrics can be represented independently where possible.
2. **AMD absent-hardware validation semantics**: ADL can initialize and legitimately return cross-vendor topology when zero AMD physical adapters are installed. The harness should distinguish provider availability from AMD hardware presence.
3. **NVIDIA absent-hardware validation semantics**: The harness currently accepts an NVML permission failure as an absent-hardware PASS. Hardware absence should not be inferred solely from provider initialization failure when the failure reason indicates permissions or another runtime error.

The NVIDIA multi-adapter driver-binding failures observed below are currently classified as external behavior and do not justify a WTG code change.

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

### Potential WTG action

None currently. WTG should report the devices exposed by the active vendor runtime rather than infer vendor-runtime availability from PCI topology.

## Case 3: AMD-Absent Harness Assumption

### Configuration

No AMD GPU was installed. ADL nevertheless initialized successfully and returned topology records for NVIDIA adapters and the Intel UHD 730.

WTG reported zero AMD physical adapters while successfully returning non-AMD topology.

This behavior was reproduced across multiple NVIDIA mixed-adapter configurations.

### Observed validation failure

The validation harness expected `Provider status: unavailable` and a non-success condition when AMD hardware was absent.

Instead, the AMD ADL provider legitimately succeeded because ADL itself was available and returned cross-vendor topology.

### Classification

**Harness defect**

The harness currently conflates an unavailable AMD provider with an available AMD provider that has zero AMD physical adapters. These are different states.

### Candidate remediation

Change AMD validation logic to distinguish at least these states:

- ADL unavailable
- ADL available with zero AMD physical adapters
- ADL available with one or more AMD physical adapters

When ADL is available and reports `AMD physical adapters: 0`, successful execution and structurally valid topology output should be accepted rather than requiring `Provider status: unavailable`.

ADLX availability should continue to be evaluated independently.

### Status

**Candidate fix, repeatedly reproduced**

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

### Positive provider comparison

The later dual-AMD test demonstrated the behavior WTG already uses successfully in another provider. The Radeon HD 7500 Series exposed fewer ADL capabilities than the R9 M360, but WTG retained the HD 7500 device and reported unsupported fields individually as unavailable. The limited device did not invalidate the AMD provider or the other AMD GPU.

This supports treating partial telemetry as a normal provider capability condition rather than an all-or-nothing device failure where the underlying API permits it.

### Potential desired behavior

Conceptually:

```text
NVML device 0: NVIDIA GeForce GT 730
  UUID: <available>
  Temp: unavailable
  Util: unavailable
  VRAM: <available>
  Power: unavailable
```

### Candidate remediation

Review the NVIDIA collection path for all-or-nothing sampling behavior. Where practical:

- device enumeration failure -> device unavailable
- individual metric unsupported -> metric unavailable
- individual metric transient error -> metric error/unavailable without automatically discarding the device

A single unsupported metric should not necessarily invalidate the rest of the device.

The exact output/schema implications should be reviewed before implementation so existing stable NVIDIA behavior is not weakened.

### Follow-up

Test other legacy NVIDIA GPUs individually, especially GeForce 210, GT 730, GTX 745, and other pre-Pascal hardware available in the test pool.

Determine whether unsupported utilization, temperature, or power telemetry produces the same whole-device failure.

### Status

**Potential WTG fix, needs broader legacy reproduction**

## Case 5: GT 730 + GTX 745 + Intel UHD 730

### Configuration

The GT 730 remained bound to NVIDIA driver 475.14. The GTX 745 was physically enumerated as NVIDIA `DEV_1382` but fell back to Microsoft Basic Display Adapter.

### Observed behavior

Windows and ADL topology continued to see the GTX 745 physically. The active NVIDIA runtime exposed the GT 730.

The GT 730 then reproduced Case 4: NVIDIA-SMI enumerated the device with limited telemetry while WTG discarded the device after `utilization_rates(0)` returned unsupported.

### Classification

Two independent observations:

- **External behavior**: GTX 745 driver binding failure.
- **Potential WTG defect**: GT 730 partial NVML support causing whole-device sampling failure.

### Potential WTG action

No remediation is proposed for the GTX 745 binding failure. Preserve the distinction between physical topology and vendor-runtime visibility.

The GT 730 behavior remains covered by the Case 4 candidate remediation.

## Case 6: GT 730 + GT 1030 + Intel UHD 730

### Configuration

Windows identified both NVIDIA devices by their proper names and retained separate installed driver versions:

- GT 730: 475.14
- GT 1030: 582.42

ADL topology identified both NVIDIA adapters.

### Observed behavior

The GT 730 failed in Device Manager with Code 31:

> Windows cannot load the drivers required for this device. (Code 31)
>
> The I/O device is configured incorrectly or the configuration parameters to the driver are incorrect.

The GT 1030 successfully owned the NVIDIA 582.42 runtime. NVIDIA-SMI and WTG both reported the GT 1030 normally. WTG returned temperature, utilization, UUID, and VRAM while independently reporting power as unavailable.

### Classification

**External behavior** for the GT 730 driver-binding failure.

**Expected WTG behavior** for the surviving GT 1030.

### Significance

This configuration produced a different Windows failure mode from the Code 10 POSTed-adapter error while still leaving only one functional NVIDIA runtime device.

It also demonstrates that WTG's NVIDIA output can already tolerate at least some unavailable metrics, such as GT 1030 power, without invalidating the device. The GT 730 failure is therefore specifically tied to the current handling of the unsupported utilization call rather than a universal requirement that every NVIDIA metric succeed.

### Potential WTG action

None for the driver-binding failure. Case 4 remains the relevant application remediation.

## Case 7: GTX 745 + GT 1030 + Intel UHD 730

### Configuration

The GT 730 was removed to test whether it was the source of the NVIDIA coexistence problem.

The resulting configuration contained:

- GTX 745
- GT 1030
- Intel UHD 730

### Observed behavior

The GTX 745 again fell back to Microsoft Basic Display Adapter and Device Manager returned the Code 10 POSTed-display-adapter mismatch:

> The driver trying to start is not the same as the driver for the POSTed display adapter.

The GT 1030 successfully bound to NVIDIA 582.42 and remained healthy through NVIDIA-SMI and WTG.

ADL topology continued to see the failed GTX 745 as a physical adapter.

### Classification

**External behavior**

### Significance

Removing the GT 730 did not eliminate the NVIDIA multi-adapter driver-binding problem. The preliminary evidence therefore does not support treating the GT 730 itself as the sole cause.

Across the tested combinations, multiple NVIDIA adapters can remain physically visible to Windows/ADL while only one successfully participates in the active NVIDIA runtime. Windows has produced both Code 10 and Code 31 failures depending on the combination.

### Potential WTG action

None currently. WTG/NVML correctly reports the NVIDIA runtime-visible subset.

A formal test matrix can later determine whether adapter order, POST selection, driver branch, VBIOS/subsystem identity, or generation is predictive.

## Case 8: Dual AMD + Intel UHD 730

### Configuration

Adapters:

- AMD Radeon HD 7500 Series, `DEV_675D`
- AMD Radeon R9 M360, `DEV_682B`
- Intel UHD Graphics 730

Both AMD devices used Windows driver `27.20.20913.2000`.

### Observed behavior

All three devices loaded normally in Windows.

ADL reported:

```text
Physical adapter groups: 3
AMD physical adapters: 2
Non-AMD physical adapters seen through ADL: 1
Extended AMD discovery ran: 2
```

The Radeon HD 7500 Series was retained as a valid AMD physical adapter despite limited telemetry:

```text
Active: no
Unavailable: overdrive caps, temp, fan, memory info
```

The R9 M360 simultaneously returned richer telemetry including activity, engine clock, memory clock, bus state, and VBIOS.

The Intel UHD 730 remained topology-only through ADL and continued to provide Intel telemetry through Level Zero.

The provider validation completed with `RESULT: PASS`.

### Classification

**Expected / positive multi-adapter control**

### Significance

This is a clean heterogeneous same-vendor multi-GPU control. It demonstrates that WTG's AMD provider can:

- group multiple physical AMD adapters correctly
- retain devices with different telemetry capabilities
- degrade unsupported fields independently
- continue cross-vendor topology reporting
- coexist with the Intel provider on the same system

This is also a useful architectural comparison for the NVIDIA GT 730 behavior in Case 4.

### Potential WTG action

None for AMD multi-adapter handling based on this run.

Use this behavior as a reference when evaluating per-metric degradation for NVIDIA.

## Case 9: NVIDIA-Absent Harness Accepts Permission Failure

### Configuration

No NVIDIA GPU was installed during the dual-AMD test.

CIM reported no NVIDIA GPU. However, `nvidia-smi.exe` remained installed on the system.

### Observed behavior

NVIDIA-SMI returned:

```text
NVIDIA-SMI has failed because you do not have suffient permissions. Please try running as an administrator.
```

WTG returned:

```text
Provider status: unavailable
Reason: NVML init failed: the current user does not have permission to perform this operation
```

The validation harness nevertheless returned:

```text
RESULT: PASS
NOTES:
- NVIDIA absent hardware accepted
```

### Classification

**Harness defect / ambiguous validation semantics**

The physical hardware was absent in this specific run, so the overall absence conclusion happened to be correct. The failure reason, however, did not establish hardware absence. It established an NVML permission failure.

The same harness logic could potentially accept a permission-broken NVIDIA installation on a system where NVIDIA hardware is actually present.

### Candidate remediation

Do not treat every NVML initialization failure as equivalent to absent hardware.

The harness should distinguish hardware evidence from provider failure reason. A safer model is:

- CIM shows no NVIDIA hardware + NVML reports a recognized no-device condition -> absent-hardware PASS
- CIM shows no NVIDIA hardware + NVML fails for permissions/runtime reasons -> record the provider failure explicitly and do not use that failure itself as evidence of absence
- CIM shows NVIDIA hardware + NVML permission/init failure -> validation failure or explicit degraded/error state, not absent-hardware PASS

The exact acceptance rules should be defined when the formal validation structure is designed.

### Status

**Candidate harness fix**

## Preliminary NVIDIA Multi-Adapter Pattern

The NVIDIA combinations tested so far suggest a recurring external-driver pattern:

| Combination | NVIDIA runtime survivor | Other NVIDIA adapter state | WTG result |
| --- | --- | --- | --- |
| GT 730 + GT 730 | GT 730 / 475.14 | second GT 730 Code 10 / Basic Display | surviving GT 730 fails WTG on unsupported utilization |
| GT 730 + GTX 745 | GT 730 / 475.14 | GTX 745 Basic Display / Code 10 class behavior | surviving GT 730 fails WTG on unsupported utilization |
| GT 730 + GT 1030 | GT 1030 / 582.42 | GT 730 Code 31 | GT 1030 PASS |
| GTX 745 + GT 1030 | GT 1030 / 582.42 | GTX 745 Code 10 / Basic Display | GT 1030 PASS |
| GT 1030 + RTX 3060 Ti | GT 1030 / 582.42 | RTX 3060 Ti not exposed through active NVML runtime | GT 1030 PASS |

Preliminary interpretation only: physical enumeration does not guarantee participation in the active NVIDIA runtime, and mixed NVIDIA configurations on this bench are exposing driver-binding limitations before WTG is involved. A formal test structure is required before drawing stronger conclusions about driver branches, GPU generations, POST order, or subsystem/VBIOS interactions.

## Preliminary Test Matrix

| Case | Configuration | Provider/Layer | Result | Classification |
| --- | --- | --- | --- | --- |
| 1 | GT 1030 + RTX 3060 Ti + UHD 730 | NVML | Only GT 1030 visible | External behavior |
| 1 | GT 1030 + RTX 3060 Ti + UHD 730 | ADL topology | All adapters visible | Expected |
| 2 | GT 730 + GT 730 + UHD 730 | Windows | One GT 730 Code 10 | External behavior |
| 2 | GT 730 + GT 730 + UHD 730 | ADL topology | Both NVIDIA adapters visible | Expected |
| 3 | No AMD GPU | AMD ADL | Successful topology-only result | Expected |
| 3 | No AMD GPU | Validation harness | Reported FAIL | Harness defect |
| 4 | GT 730 | NVML | Device discarded after unsupported utilization call | Potential WTG defect |
| 5 | GT 730 + GTX 745 + UHD 730 | Windows/NVIDIA | GTX 745 fails binding; GT 730 survives | External behavior |
| 6 | GT 730 + GT 1030 + UHD 730 | Windows/NVIDIA | GT 730 Code 31; GT 1030 survives | External behavior |
| 6 | GT 730 + GT 1030 + UHD 730 | WTG/NVML | GT 1030 telemetry PASS | Expected |
| 7 | GTX 745 + GT 1030 + UHD 730 | Windows/NVIDIA | GTX 745 Code 10; GT 1030 survives | External behavior |
| 7 | GTX 745 + GT 1030 + UHD 730 | WTG/NVML | GT 1030 telemetry PASS | Expected |
| 8 | HD 7500 Series + R9 M360 + UHD 730 | AMD ADL | Two AMD physical adapters handled independently | PASS / positive control |
| 8 | HD 7500 Series + R9 M360 + UHD 730 | Intel Level Zero | UHD 730 remains independently available | PASS |
| 9 | No NVIDIA GPU | NVIDIA NVML | Permission failure | Provider unavailable, reason not equivalent to absence |
| 9 | No NVIDIA GPU | Validation harness | Accepted permission failure as absent-hardware PASS | Harness defect |

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

#### Potential remediation

TBD.

#### Evidence

```text
Artifact names:
Relevant hashes:
Notes:
```

#### Status

`Observed / Needs reproduction / Candidate fix / Resolved`
