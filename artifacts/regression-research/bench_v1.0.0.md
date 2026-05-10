# WTG Bench v1 Hardware and Software Baseline

## Status

Active mixed adapter WDDM configuration Intel iGPU is primary display
RTX 3060 Ti is installed as headless compute device

Firmware validated via GPU-Z: Above 4G Decoding: Enabled (firmware
managed, not user configurable) Resizable BAR: Not supported by current
GPU / VBIOS / driver combination BAR1 Aperture: 256 MiB (legacy mode)

------------------------------------------------------------------------

## Platform

### System Lineage

Dell Inspiron 3891 Service Tag: 6N2R0M3 Open air 2020 T rail style bench
ATX PSU conversion

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
