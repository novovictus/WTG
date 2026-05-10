# WTG Regression Research: A Mobile Ampere Memory-Utilization Counter Goes to 100 Percent at Idle

Windows GPU telemetry is layered. Task Manager can show that a GPU is busy, and vendor tools can expose selected counters, but the CUDA-relevant truth layer lives closer to the NVIDIA driver. WTG was built to operate at that layer. It is a Windows-native NVML reference tool intended to answer a direct question: what does the NVIDIA driver report when queried for GPU utilization, memory-controller activity, VRAM allocation, power, process attribution, and runtime context?

During WTG regression testing, one signal separated itself from normal variance. On a consumer mobile Ampere platform, the NVIDIA GeForce RTX 3080 Laptop GPU began reporting 100 percent memory-controller utilization at idle or near-idle beginning with the 580.88 driver branch. The rest of the telemetry did not agree with real saturation: GPU utilization stayed near zero, VRAM allocation stayed low, thermals stayed normal, and power remained in an idle or light-load range.

This report is not a bug report against WTG. WTG is the reference instrument used to expose and compare the driver behavior. The finding is a driver-facing telemetry regression candidate: a reproducible NVML memory-utilization anomaly on a specific consumer mobile GPU class and driver branch, bounded by earlier-driver controls, mobile negative controls, and a stable desktop bench.

## What the counter means

The key field is NVML memory utilization, represented in WTG's beta 4 probe output as `util.mem_controller_pct`. This field is not VRAM occupancy. It is a utilization counter for memory read/write activity during the sample interval. VRAM occupancy is captured separately as `vram.used_mib` and `vram.total_mib`.

That distinction is essential. A GPU can have low VRAM allocation while still performing memory reads and writes, and it can also have VRAM allocated while doing little active work. The anomaly here is not that the GPU has allocated memory. The anomaly is that the memory-controller activity counter is pinned at 100 percent when the rest of the telemetry indicates idle or near-idle behavior.

## The regression signal

The affected system class is the ASUS ROG Strix G533QS_G533QS with the NVIDIA GeForce RTX 3080 Laptop GPU under Windows WDDM. The RTX 3080 Laptop GPU evidence was not collected from a single machine only; the regression set includes samples from two identical ASUS G533QS systems with the same GPU class. Earlier drivers form the baseline. With driver 566.07, the RTX 3080 Laptop GPU reported 0 percent GPU utilization and 0 percent memory-controller utilization, with low VRAM allocation and normal power. With driver 577.00, the same ASUS RTX 3080 Laptop GPU platform again reported normal memory-controller behavior: low GPU activity, 0 percent memory-controller utilization, modest VRAM allocation, and normal power.

The behavior changes at driver 580.88. In the baseline capture, the RTX 3080 Laptop GPU reported 0 percent GPU utilization and 100 percent memory-controller utilization while using only 292 MiB of 16,384 MiB of VRAM. Power was approximately 17.5 W against a 130 W limit, and the parallel NVIDIA-SMI snapshot showed 0 percent GPU utilization and only 86 MiB of 16,384 MiB frame-buffer memory in use. That is not the shape of a saturated memory workload.

The same pattern persists on driver 591.59. WTG reported 4 percent GPU utilization and 100 percent memory-controller utilization, with 606 MiB of 16,384 MiB VRAM allocated and approximately 20.9 W of power draw. NVIDIA-SMI again showed low activity, with 1 percent GPU utilization and 401 MiB of 16,384 MiB frame-buffer memory in use. The counter remained pinned while every surrounding signal remained in the idle or light-load envelope.

The broader test matrix confirms that this was not treated as an isolated one-off. The observed regression line begins at 580.88 and is tracked across later 59x branch testing for the RTX 3080 Laptop GPU. The same matrix records stable idle memory-utilization behavior on other tested NVIDIA classes, including desktop Ampere, professional Ampere, and other mobile GPUs. That cross-SKU contrast is what turns the observation from a confusing single capture into a defensible regression finding.

## The desktop bench as a control

The desktop bench matters because it demonstrates what coherent telemetry looks like when the same class of counters is exercised under real compute load. The reference desktop system uses a mixed-adapter WDDM configuration: Intel integrated graphics is the primary display adapter, and a GeForce RTX 3060 Ti is installed as a headless compute device. Its baseline records stable idle behavior around P8 and approximately 6 W, and stable load behavior around P2 and approximately 200 W, with a 200 W exposed power limit.

A WTG stats capture from that bench shows the expected load shape. NVIDIA-SMI reported the RTX 3060 Ti at 54 C, P2, approximately 199 W of 200 W, 5,421 MiB of 8,192 MiB frame-buffer memory used, and 87 percent GPU utilization. WTG reported 54 C, 89 percent GPU utilization, 95 percent memory-controller utilization, 5,588 MiB of 8,192 MiB VRAM used, and 199.4 W of 200 W.

That is what high memory-controller utilization looks like when it is backed by workload evidence. GPU utilization is high. VRAM allocation is substantial. Power is at the board limit. The performance state is elevated. The memory-controller counter fits the rest of the picture.

The beta 4 desktop probe adds the complementary idle control. On the same desktop GPU class and driver branch, WTG v0.2.0-beta4 reported P8, 39 C, 0 percent GPU utilization, 0 percent memory-controller utilization, 298 MiB of 8,192 MiB VRAM used, and 5.4 W of 200 W. The same probe run also produced structured CSV and JSONL sink output with the same values. In other words, the newer probe path does not manufacture the 100 percent idle memory-controller condition. On the desktop control, it reports normal idle telemetry.

This is why the desktop bench is more than a comparison point. It proves the reference tool can distinguish idle from load on the same driver family, and it shows that high memory-controller utilization is not inherently suspicious when the surrounding evidence supports it. The suspicious part of the mobile RTX 3080 Laptop GPU captures is the mismatch: memory-controller utilization is pegged while the rest of the telemetry says the GPU is not doing corresponding work.

## The mobile negative control

The Acer Nitro AN517-42 provides the key mobile negative control. It is also a Windows mobile NVIDIA platform tested on driver 580.88, but it uses a GeForce RTX 3060 Laptop GPU rather than a GeForce RTX 3080 Laptop GPU. In that capture, NVIDIA-SMI reported 0 percent GPU utilization and no running GPU processes. WTG reported 0 percent GPU utilization, 0 percent memory-controller utilization, 148 MiB of 6,144 MiB VRAM used, and approximately 23.7 W of 105 W.

That result prevents the finding from being overstated. Driver 580.88 alone is not sufficient to produce the symptom across every tested mobile NVIDIA GPU. The regression is narrower: it is observed on the tested consumer mobile Ampere RTX 3080 Laptop GPU configuration beginning with the 580.88 branch, while another mobile Ampere GPU on the same branch reports normal idle memory-controller utilization.

## The defensible finding

The finding can be stated narrowly:

WTG regression testing identified a probable NVIDIA driver/NVML memory-utilization reporting regression affecting the ASUS ROG Strix G533QS_G533QS with GeForce RTX 3080 Laptop GPU beginning with driver 580.88. The symptom is 100 percent NVML memory-controller utilization at idle or near-idle while GPU utilization, VRAM allocation, power, thermals, and process evidence remain normal. The behavior is not present on earlier tested RTX 3080 Laptop GPU drivers across the ASUS G533QS sample set, is not reproduced on the Acer Nitro AN517-42 with GeForce RTX 3060 Laptop GPU on driver 580.88, and is not reproduced on the desktop RTX 3060 Ti reference bench under either sustained load characterization or beta 4 idle probing.

This does not show VRAM exhaustion. It does not show memory bandwidth saturation in the practical workload sense. It shows a driver-reported memory-utilization counter returning a value that is inconsistent with adjacent NVML and NVIDIA-SMI telemetry on a specific mobile GPU and driver branch.

That is the value of WTG as a reference tool. It does not replace the driver. It interrogates the driver. It does not infer a story from Windows' higher-level GPU abstractions. It captures the low-level telemetry as reported and makes those results comparable across GPU models, driver versions, power states, and output formats.

## Publication posture

This is publishable as regression research because the finding is bounded. There is a before state: RTX 3080 Laptop GPU behavior is normal on 566.07 and 577.00. There is an after state: the same ASUS G533QS RTX 3080 Laptop GPU platform reports 100 percent memory-controller utilization at idle or near-idle on 580.88 and 591.59. There are controls: a desktop RTX 3060 Ti bench that shows coherent idle and load telemetry, and an Acer Nitro RTX 3060 Laptop GPU sample on 580.88 that does not show the memory-controller symptom.

The appropriate follow-on is a vendor bug report using the published evidence package: affected GPU class, unaffected controls, driver branch boundary, WDDM context, WTG probe output, NVIDIA-SMI comparison output, and structured beta 4 probe artifacts. The public post should establish the research trail first; the NVIDIA report can then refer to a clean, reproducible, already-documented finding rather than a single ambiguous screenshot.

## Final note

This report does not claim that the GPU is actually saturating memory bandwidth, that applications are affected, or that WTG has identified the internal NVIDIA driver cause. The evidence supports a narrower conclusion: on the tested ASUS G533QS RTX 3080 Laptop GPU platforms, NVML's memory-utilization counter begins returning an idle or near-idle 100 percent value on later driver branches while adjacent telemetry remains inconsistent with real saturation. The next step is vendor-side confirmation using the captured WTG, NVIDIA-SMI, driver, GPU, and structured probe artifacts.
