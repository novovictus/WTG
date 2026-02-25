# Known Issues

## RTX 3080 Laptop GPU -- Driver 580.88+

Under Windows WDDM, driver versions beginning at 580.88 and confirmed through 591.86 may report:

`utilization.memory = 100%` at idle

This behavior:
- Is visible in both WTG and `nvidia-smi`
- Does not correlate with thermal or power saturation
- Has not been reproduced on desktop Ampere or professional Ampere SKUs
