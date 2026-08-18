Date: 2026-07-09

Target: golden / Acer Nitro AN517-42

Host/user: acer@golden

AMD GPU: AMD Radeon(TM) Graphics

NVIDIA GPU: RTX 3060 Laptop GPU

AMD driver after full AMD package: 32.0.21043.19003

NVIDIA driver after full AMD package: 512.74 / CIM 30.0.15.1274

WTG ADLX result:
- Probe attempted: yes
- Runtime/DLL: found
- Init: failed
- Devices returned: unknown
- Reason: ADLXInitialize failed: ADLX_FAIL

External control result:
- DLL load: ok
- DLL path: C:\WINDOWS\SYSTEM32\amdadlx64.dll
- ADLXInitialize2 export: missing
- ADLXInitialize export: present
- ADLXInitializeWithIncompatibleDriver2 export: missing
- ADLXInitializeWithIncompatibleDriver export: present
- ADLXTerminate export: present
- Helper.Initialize(): 3 / ADLX_FAIL
- IADLXSystem returned: no
- Helper.Terminate(): 0 / ADLX_OK

Conclusion:
- WTG and AMD-helper-based external control fail the same way.
- Evidence supports golden driver/runtime/hardware state rejecting ADLX initialization.
- Evidence does not support a WTG-specific ADLX initialization bug.
- ADL remains topology-only on golden.
- ADLX telemetry is unavailable on golden in this driver/software state.

This is lab/artifact material only and is not intended for public-facing documentation.
