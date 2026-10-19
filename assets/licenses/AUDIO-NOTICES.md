# Local audio dependencies

## RUNE's call sounds

`assets/sounds/*.ogg` are RUNE's own: tones synthesized from numbers by
`scripts/sounds.sh` (ffmpeg's `aevalsrc`, encoded with libopus), with no
sample or recording of anyone. They are part of RUNE and under its license
(MIT OR Apache-2.0). Discord's sounds are not extracted, copied or shipped;
a person may play their own files instead (docs/MEDIA.md, «carpeta de
sonidos»).

RUNE uses the following packages as dependencies, without copying Serein
implementation code. Versions below are the versions resolved for the local
capture/preprocessing change on 2026-10-14.

| Packages | License | Preserved text / source |
| --- | --- | --- |
| `sonora`, `sonora-aec3`, `sonora-agc2`, `sonora-common-audio`, `sonora-fft`, `sonora-ns`, `sonora-simd` 0.2.0 | BSD-3-Clause | [Sonora-BSD-3-Clause.txt](Sonora-BSD-3-Clause.txt), [upstream](https://github.com/dignifiedquire/sonora) |
| `rtrb` 0.4.0 | MIT OR Apache-2.0 | [MIT text](rtrb-MIT.txt), [upstream](https://github.com/mgeier/rtrb). Package authors: Stjepan Glavina and Matthias Geier. |
| `arc-swap` 1.9.2 | MIT OR Apache-2.0 | [MIT text](arc-swap-MIT.txt), [upstream](https://github.com/vorner/arc-swap). Copyright 2017 arc-swap developers. |
| `cpal` 0.18.2 (existing dependency) | Apache-2.0 | [Upstream license](https://github.com/RustAudio/cpal/blob/v0.18.2/LICENSE) |
| `nnnoiseless` 0.5.2 (2026-10-16, default features off) | BSD-3-Clause | [nnnoiseless-BSD-3-Clause.txt](nnnoiseless-BSD-3-Clause.txt), [upstream](https://github.com/jneem/nnnoiseless). Copyright Joe Neeman, Mozilla, Jean-Marc Valin, Xiph.Org Foundation, Mark Borgerding. Its RNNoise model weights are compiled in. |

nnnoiseless brings `rustfft` 6.4.1, `realfft` 3.5.0, `easyfft` 0.4.2,
`transpose` 0.2.3, `strength_reduce` 0.2.4, `primal-check` 0.3.4,
`array-init` 2.1.0, `generic_singleton` 0.5.3 (MIT OR Apache-2.0; `realfft`
MIT only, its package ships no license file: author HEnquist) and `anymap3`
1.1.0 (BlueOak-1.0.0 OR MIT OR Apache-2.0). Their texts are appended to
[AUDIO-DEPENDENCY-LICENSES.txt](AUDIO-DEPENDENCY-LICENSES.txt).

Sonora's retained notice credits the WebRTC Project Authors (2011), Arun
Raghavan and contributors (2016), and dignifiedquire (2026). The six published
Sonora packages that include `LICENSE` ship the same text (SHA-256
`d130affc26760004865ad4fb009f8daf7f6d4b1b738f01fb7623e99c1a275a4a`).
The `sonora-aec3` 0.2.0 package declares BSD-3-Clause but omits a standalone
license file; the shared upstream license is preserved here for it as well.
Do not use upstream project or contributor names as endorsements.

The complete `Cargo.lock` dependency closure rooted at these three new
choices was checked against the resolved package manifests (30 package/version
pairs, including existing workspace-unified macro dependencies). The license
texts are preserved in [AUDIO-DEPENDENCY-LICENSES.txt](AUDIO-DEPENDENCY-LICENSES.txt),
with per-text hashes and package attribution. MIT is selected where a choice
is offered; `unicode-ident` additionally requires Unicode-3.0, whose notice
is included. This audit is scoped to that Rust closure, not the whole RUNE
workspace, CPAL platform dependencies or native/system libraries.

| Resolved package | Declared license |
| --- | --- |
| `arc-swap` 1.9.2 | MIT OR Apache-2.0 |
| `bytemuck` 1.25.2 | Zlib OR Apache-2.0 OR MIT |
| `bytemuck_derive` 1.12.1 | Zlib OR Apache-2.0 OR MIT |
| `cpufeatures` 0.3.1 | MIT OR Apache-2.0 |
| `derive_more` 2.1.1 | MIT |
| `derive_more-impl` 2.1.1 | MIT |
| `libc` 0.2.190 | MIT OR Apache-2.0 |
| `log` 0.4.34 | MIT OR Apache-2.0 |
| `once_cell` 1.21.4 | MIT OR Apache-2.0 |
| `pin-project-lite` 0.2.17 | Apache-2.0 OR MIT |
| `proc-macro2` 1.0.107 | MIT OR Apache-2.0 |
| `quote` 1.0.47 | MIT OR Apache-2.0 |
| `rtrb` 0.4.0 | MIT OR Apache-2.0 |
| `rustc_version` 0.4.1 | MIT OR Apache-2.0 |
| `rustversion` 1.0.23 | MIT OR Apache-2.0 |
| `semver` 1.0.28 | MIT OR Apache-2.0 |
| `sonora` 0.2.0 | BSD-3-Clause |
| `sonora-aec3` 0.2.0 | BSD-3-Clause |
| `sonora-agc2` 0.2.0 | BSD-3-Clause |
| `sonora-common-audio` 0.2.0 | BSD-3-Clause |
| `sonora-fft` 0.2.0 | BSD-3-Clause |
| `sonora-ns` 0.2.0 | BSD-3-Clause |
| `sonora-simd` 0.2.0 | BSD-3-Clause |
| `syn` 2.0.119 | MIT OR Apache-2.0 |
| `syn` 3.0.9 | MIT OR Apache-2.0 |
| `tracing` 0.1.44 | MIT |
| `tracing-attributes` 0.1.31 | MIT |
| `tracing-core` 0.1.36 | MIT |
| `unicode-ident` 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 |
| `unicode-xid` 0.2.6 | MIT OR Apache-2.0 |

AEC and AGC subcrates are compile-time dependencies of Sonora; since
2026-10-16 RUNE enables echo cancellation and AGC2 when the user chooses
them, and nnnoiseless (with its built-in RNNoise weights) is included.
