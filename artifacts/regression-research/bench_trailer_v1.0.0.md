## Riser Hardware:

```text
Rosewill RCRC-17001
PCIe x1-to-x16 powered riser adapter
```
## Riser Power Cable:

```text
Thsion B0DRP72QHD
Item model: EVGA-THSION-PCIE25
Length: 25 inches
Connector layout: EVGA PSU-side 8-pin to PCIe 6+2-pin
Color: black
Compatibility target: EVGA modular G5 series power supplies
```
## Bench Wiring:

```text
EVGA SuperNOVA 850 G5 VGA port
  -> Thsion EVGA-compatible 8-pin to 6+2 PCIe cable
  -> use 6-pin portion of 6+2 connector
  -> Rosewill RCRC-17001 riser board 6-pin PCIe input
```

Do not use the SATA-to-6-pin adapter bundled with mining-style risers. A full-size PCIe graphics slot can supply up to 75 W to a card, while a standard SATA power connector’s 12 V side is commonly treated as a 54 W maximum path based on three 12 V pins rated at 1.5 A each. That leaves no safe margin for GPU slot-power behavior, adapter losses, cheap connector tolerances, or startup/load spikes. SATA power was designed for drives, not powered GPU risers. Using a SATA adapter on a GPU riser is a known melt/fire-risk pattern. Power the riser directly from the EVGA PSU with the EVGA-compatible PCIe/VGA cable into the riser’s 6-pin PCIe input.

## Bench Use:

```text
Dell Inspiron 3891 PCIe x1 slot
  -> Rosewill RCRC-17001 PCIe x1 adapter
  -> riser cable
  -> powered PCIe x16 riser board
  -> rotating test GPU
```

The riser is for adapter presence, driver behavior, provider enumeration, and telemetry-path testing. It is not a performance lane. Any card installed through this riser should be treated as a low-bandwidth test adapter.

The powered x16 riser board will be mounted as a rear-side trailer bay using the prior T-channel hardware. The riser board will not hang from the motherboard slot or from the riser cable.

## Mechanical mounting:

```text
T-channel rail mounted on rear side of bench frame
Riser board attached to insulated standoffs
GPU weight supported by bracket and rail-mounted support
USB/riser cable service loop with Velcro
PCIe x1 motherboard adapter left mechanically unloaded
GPU fans unobstructed
Power cables routed away from fans
Riser bay physically separated from RTX 3060 Ti airflow path and treated as a replaceable test fixture.
Riser cable will be labeled and kept with the trailer bay hardware
```