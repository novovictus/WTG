# WTG Bench v1.1 Hardware and Software Baseline

## Status

Active mixed adapter WDDM configuration Intel iGPU is primary display
RTX 3060 Ti is installed as headless compute device

Bench v1.1 extends the original Bench v1.0 baseline with a powered PCIe riser path for secondary adapter enumeration, device identity checks, provider probing, and telemetry validation.

Firmware validated via GPU-Z: Above 4G Decoding: Enabled (firmware
managed, not user configurable) Resizable BAR: Not supported by current
GPU / VBIOS / driver combination BAR1 Aperture: 256 MiB (legacy mode)

------------------------------------------------------------------------

## Platform

### System Lineage

Dell Inspiron 3891 Service Tag: 6N2R0M3 Open air 2020 T rail style bench
ATX PSU conversion

### Bench Iteration

Bench v1.1 preserves the v1.0 baseline as a historical artifact and adds the mixed adapter riser expansion as the current update surface. This file may receive additional updates as riser-path testing, mixed adapter enumeration, AMD provider exploration, and Intel provider exploration mature.

### Build Album

https://photos.app.goo.gl/QV5WbYFcLYBPVLTj8

### Motherboard

Manufacturer: Dell Inc. Model: 0YF8P5 Revision: A00

### BIOS

Version: 1.33.0 Release Date: 2025-03-02

------------------------------------------------------------------------

## CPU

Model: Intel Core i5-11400 Cores: 6 Threads: 12 Base Clock: 2.60 GHz
Integrated Graphics: Intel UHD 730 active

------------------------------------------------------------------------

## Memory

Total Installed: 16 GB Configuration: 2 x 8 GB Mode: Dual Channel

  -------------------------------------------------------------------------
  Slot   Manufacturer   Part Number        Rated (MT/s)  Configured (MT/s)
  ------ -------------- ------------------ ------------- ------------------
  A      Hynix          HMAA1GU6CJR6N-XN   3200          2933

  B      Samsung        M378A1G44BB0-CWE   3200          2933
  -------------------------------------------------------------------------

------------------------------------------------------------------------

## Storage

### Primary Disk

Model: SK hynix BC711 NVMe 256GB Partition Style: GPT Capacity:
256060514304 bytes

------------------------------------------------------------------------

## Operating System

Product Name: Windows 10 Home Display Version: 25H2 ReleaseId: 2009
Build: 26200.7840

------------------------------------------------------------------------

## Graphics Configuration

Mixed adapter configuration under WDDM.

### Primary Display Adapter

Intel UHD Graphics 730 Driver Version: 32.0.101.7077

### Discrete Compute Adapter

NVIDIA GeForce RTX 3060 Ti Architecture: Ampere Driver Model: WDDM
NVIDIA Driver Version: 581.95 Win32 Driver Version: 32.0.15.8195 CUDA
Version: 13.0 VBIOS Version: 94.04.38.80.9f

VRAM Total: 8192 MiB VRAM Reserved: 168 MiB BAR1 Total: 256 MiB

Power Limit: 200 W Minimum Power Limit: 100 W Maximum Power Limit: 200 W

Display Attached: No Display Active: Disabled

Idle State Observed Performance State: P8 Power Draw: approximately 6 W PCIe Link Current: Gen1 x8
Load State Observed Performance State: P2 Power Draw: approximately 200 W PCIe Link Current: Gen4 x8

### Mixed Adapter Expansion

Bench v1.1 adds a powered PCIe riser path so the bench can host additional adapters alongside the primary NVIDIA reference card. The intent is full mixed-adapter test cycling with Intel, AMD, and NVIDIA devices installed in the same Windows host.

The RTX 3060 Ti remains the primary NVIDIA / NVML reference device. The riser path is intended for secondary adapter enumeration, device identity checks, provider probing, and telemetry validation. It should not be treated as equivalent to a native motherboard x16 slot for performance or sustained-load claims unless separately validated.

Current intended provider roles:

- NVIDIA discrete adapter: primary NVML reference path.
- Intel integrated graphics: Intel Level Zero / Sysman exploration path.
- AMD secondary adapter: AMD ADL or adjacent provider exploration path.

------------------------------------------------------------------------

## Powered PCIe Riser Expansion

### Riser Kit

Rosewill RCRC-18001 powered PCIe riser adapter.


Package contents:

- PCIe 16x riser board.
- PCIe 1x adapter card.
- 60 cm USB 3.0 cable.
- 6-pin PCIe power to 15-pin SATA power cable.

Listed features:

- PCIe 16x to 1x riser adapter.
- 4 solid capacitors.
- Voltage regulation components.
- Overcurrent protection.
- Gold-plated contacts.
- No driver required.
- 4 mounting holes for attaching the riser card to a rig or frame.

### Added PSU Cable

Thsion 25 inch PCIe cable for EVGA modular power supplies.


Observed product framing:

- PSU side: 8-pin EVGA modular VGA / PCIe interface.
- GPU side: 6+2-pin PCIe male connector.
- Cable style: male-to-male GPU power cable.
- Listed for EVGA modular power supplies only.
- Listed compatibility includes EVGA SuperNOVA G5 650, 750, 850, and 1000 models.

Bench rationale:

- Bench PSU is an EVGA SuperNOVA 850 G5.
- Added cable provides an EVGA-G5-compatible PCIe power lead for the expanded bench configuration.
- Avoid depending on the riser kit's included SATA power adapter for bench riser/card power where a direct modular PCIe cable path is available.

### Mechanical Installation

The riser card is mounted to the rear of the open-air 2020 T-rail frame above the PSU using T-channel mounting hardware, nylon spacers, and M3 hardware.

A salvaged PCI slot expansion cover with a grill pattern was modified with a nibbler to create a card-retention opening. The lower lip of the cover was retained to provide mechanical support for installed test cards.

The riser is mounted as a bench fixture, not as a temporary loose mining-rig adapter. The goal is repeatable adapter installation and removal during provider and telemetry testing.

### Validation Posture

The powered riser path is useful for:

- Adapter enumeration.
- Device identity capture.
- Driver/provider discovery.
- Idle and light telemetry validation.
- Mixed-adapter Windows behavior checks.
- WTG provider surface exploration.

The powered riser path is not yet validated as equivalent to a native motherboard slot for:

- Sustained GPU compute load.
- High-confidence performance characterization.
- PCIe bandwidth-sensitive testing.
- Slot-power-limit conclusions.
- Thermal conclusions for production enclosures.

The direct motherboard slot remains the preferred path for primary NVIDIA load/reference testing unless a specific riser-path test is being performed.

------------------------------------------------------------------------

## Power Supply

Model: EVGA SuperNOVA 850 G5 Part Number: 220-G5-0850-X1 Rated Output:
850 W at 50 C Efficiency: 91 percent (115 VAC) / 92 percent (220-240 VAC
typical)

### Electrical Characteristics

+12V Rail: 70.8 A (849.2 W available) +3.3V Rail: 24 A +5V Rail: 24 A
Combined +3.3V and +5V: 120 W -12V Rail: 0.5 A +5Vsb Rail: 3 A

Input Voltage: 100 to 240 VAC Input Frequency: 50 / 60 Hz

### Protection Mechanisms

Over Voltage Protection (OVP) Under Voltage Protection (UVP) Over
Current Protection (OCP) Over Power Protection (OPP) Short Circuit
Protection (SCP) Over Temperature Protection (OTP)

------------------------------------------------------------------------

## ATX Breakout and Power Control

### Primary PSU

EVGA SuperNOVA 850 G5 850 W 80 Plus Gold Fully modular Part Number:
220-G5-0850-X1

### Motherboard Power Adaptation

Original Dell proprietary motherboard power connector removed from donor
Dell PSU Connector repurposed to allow standard ATX PSU integration with
Dell 0YF8P5 board

### ATX Breakout Module

Model: GeeekPi 24/20-pin ATX DC Power Supply Breakout Board Connector
Support: 20 / 20+4 / 24-pin ATX Form Factor: ATX compatible Integrated
latching power switch Independent LED indicators per rail Terminal block
breakout for voltage rails

------------------------------------------------------------------------

## CPU Cooler

Model: Cooler Master Hyper 212 Halo Black Product Number:
RR-S4KK-20PA-R1 Exterior Color: Black

### Mechanical

Dimensions: 124 x 73 x 154 mm Height: 154 mm Heat Sink: 4 heat pipes
with aluminum fins Top Cover: Aluminum

Socket Compatibility Intel LGA 1700 / 1200 / 1151 / 1150 / 1155 / 1156
AMD AM5 / AM4

Installation Note Installed without stock Dell chassis integrated
backplate Using standard LGA1200 compatible mounting hardware

### Fan

Size: 120 x 120 x 25 mm Quantity: 1 Speed: 650 to 2050 RPM Maximum
Airflow: 51.88 CFM Maximum Static Pressure: 2.89 mmH2O Maximum Noise: 27
dB(A) Bearing: Rifle Connector: 4 pin PWM Rated Voltage: 12 VDC Rated
Current: 0.28 A Power Consumption: 3.36 W MTTF: greater than 160000
hours

