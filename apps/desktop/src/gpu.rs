//! Which GPU RUNE draws with on a laptop with two.
//!
//! Measured on this project's machine (Radeon 780M integrated, RTX 5060
//! discrete): Vulkan's loader opens every installed driver when wgpu lists
//! adapters, and the NVIDIA driver alone took ~0.9 s to start and kept the
//! discrete GPU powered for as long as RUNE ran, whichever adapter wgpu then
//! chose. With the loader limited to the integrated GPU's Mesa driver the
//! first frame came in 80 ms and the discrete GPU stayed suspended. A chat
//! window has no use for the discrete GPU, so on such a machine RUNE limits
//! the loader to the GPU the display boots on, unless the person chose
//! otherwise: in the settings («Dibujar con», escritorio-linux), kept in
//! `~/.config/rune/gpu`, or with a variable of the environment, which
//! wins. See docs/ARCHITECTURE.md.

/// A display controller as sysfs describes it.
#[derive(Clone, Debug, PartialEq)]
pub struct Gpu {
    /// PCI vendor, such as 0x1002.
    pub vendor: u32,
    /// The firmware drew the boot console with it: the integrated one on a laptop.
    pub boot: bool,
}

const AMD: u32 = 0x1002;
const INTEL: u32 = 0x8086;
const NVIDIA: u32 = 0x10de;

/// Variables that already choose Vulkan drivers or the GPU; any of them set
/// means the person decided, and RUNE leaves the choice alone.
pub const CHOSEN_BY_PERSON: [&str; 4] = [
    "VK_LOADER_DRIVERS_SELECT",
    "VK_DRIVER_FILES",
    "VK_ICD_FILENAMES",
    "RUNE_GPU",
];

/// The Vulkan loader filter that keeps only the boot GPU's Mesa driver, when
/// there is a second GPU of another vendor to keep asleep.
pub fn integrated_only(gpus: &[Gpu], chosen: impl Fn(&str) -> bool) -> Option<&'static str> {
    if CHOSEN_BY_PERSON.iter().any(|name| chosen(name)) {
        return None;
    }
    let boot = gpus.iter().find(|gpu| gpu.boot)?;
    if !gpus.iter().any(|gpu| gpu.vendor != boot.vendor) {
        return None;
    }
    match boot.vendor {
        AMD => Some("*radeon*"),
        INTEL => Some("*intel*"),
        _ => None,
    }
}

// Linux desktop (escritorio-linux).
/// Each vendor whose GPUs RUNE can be told to draw with: its key in the
/// settings, its name, and the Vulkan loader filter that keeps only its
/// drivers (Mesa's and NVIDIA's own; NVK is Mesa's `nouveau`).
const VENDORS: [(u32, &str, &str, &str); 3] = [
    (AMD, "amd", "AMD", "*radeon*"),
    (INTEL, "intel", "Intel", "*intel*"),
    (NVIDIA, "nvidia", "NVIDIA", "*nvidia*,*nouveau*"),
];

/// The GPUs of `gpus` that can be chosen, by key and name, once per
/// vendor (a filter cannot tell two of one apart); the boot one first.
pub fn choices(gpus: &[Gpu]) -> Vec<(String, String)> {
    let mut ordered: Vec<&Gpu> = gpus.iter().filter(|gpu| gpu.boot).collect();
    ordered.extend(gpus.iter().filter(|gpu| !gpu.boot));
    let mut choices: Vec<(String, String)> = Vec::new();
    for gpu in ordered {
        if let Some((_, key, name, _)) = VENDORS.iter().find(|vendor| vendor.0 == gpu.vendor) {
            if !choices.iter().any(|(known, _)| known == key) {
                choices.push(((*key).into(), (*name).into()));
            }
        }
    }
    choices
}

/// The loader filter for this start: the person's choice `chosen` if this
/// machine has such a GPU, else RUNE's ([`integrated_only`]); none when a
/// variable of the environment already decides.
pub fn filter(
    gpus: &[Gpu],
    chosen: Option<&str>,
    set: impl Fn(&str) -> bool,
) -> Option<&'static str> {
    if CHOSEN_BY_PERSON.iter().any(|name| set(name)) {
        return None;
    }
    chosen_filter(gpus, chosen).or_else(|| integrated_only(gpus, set))
}

/// The loader filter of the person's choice `chosen`, if this machine has
/// such a GPU.
pub fn chosen_filter(gpus: &[Gpu], chosen: Option<&str>) -> Option<&'static str> {
    let (vendor, _, _, filter) = VENDORS.iter().find(|vendor| Some(vendor.1) == chosen)?;
    gpus.iter()
        .any(|gpu| gpu.vendor == *vendor)
        .then_some(*filter)
}

/// The variable of the environment that decides the GPU, if one does.
pub fn overridden(set: impl Fn(&str) -> bool) -> Option<&'static str> {
    CHOSEN_BY_PERSON.into_iter().find(|name| set(name))
}

/// The display controllers of this machine, from sysfs. Reading them wakes
/// nothing: no driver is asked.
pub fn display_controllers() -> Vec<Gpu> {
    let Ok(devices) = std::fs::read_dir("/sys/bus/pci/devices") else {
        return Vec::new();
    };
    let read = |path: std::path::PathBuf| {
        std::fs::read_to_string(path)
            .ok()
            .map(|text| text.trim().to_owned())
    };
    devices
        .flatten()
        .filter_map(|device| {
            let path = device.path();
            // Class 0x03xxxx: display controllers.
            if !read(path.join("class"))?.starts_with("0x03") {
                return None;
            }
            let vendor =
                u32::from_str_radix(read(path.join("vendor"))?.trim_start_matches("0x"), 16)
                    .ok()?;
            let boot = read(path.join("boot_vga")).as_deref() == Some("1");
            Some(Gpu { vendor, boot })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpus(list: &[(u32, bool)]) -> Vec<Gpu> {
        list.iter()
            .map(|(vendor, boot)| Gpu {
                vendor: *vendor,
                boot: *boot,
            })
            .collect()
    }

    #[test]
    fn a_hybrid_laptop_keeps_the_discrete_gpu_asleep() {
        let nobody = |_: &str| false;
        assert_eq!(
            integrated_only(&gpus(&[(NVIDIA, false), (AMD, true)]), nobody),
            Some("*radeon*")
        );
        assert_eq!(
            integrated_only(&gpus(&[(INTEL, true), (NVIDIA, false)]), nobody),
            Some("*intel*")
        );
    }

    #[test]
    fn one_gpu_an_unknown_boot_gpu_or_a_choice_of_the_person_changes_nothing() {
        let nobody = |_: &str| false;
        assert_eq!(
            integrated_only(&gpus(&[(AMD, true)]), nobody),
            None,
            "nothing to keep asleep"
        );
        assert_eq!(
            integrated_only(&gpus(&[(NVIDIA, true), (AMD, false)]), nobody),
            None,
            "the display boots on NVIDIA"
        );
        assert_eq!(
            integrated_only(&gpus(&[(NVIDIA, false), (AMD, false)]), nobody),
            None,
            "no boot GPU is known"
        );
        let chose = |name: &str| name == "VK_DRIVER_FILES";
        assert_eq!(
            integrated_only(&gpus(&[(NVIDIA, false), (AMD, true)]), chose),
            None
        );
    }

    // Linux desktop (escritorio-linux).
    #[test]
    fn the_person_s_gpu_is_drawn_with_unless_the_environment_decides() {
        let nobody = |_: &str| false;
        let laptop = gpus(&[(NVIDIA, false), (AMD, true)]);
        assert_eq!(
            choices(&laptop),
            [
                ("amd".to_owned(), "AMD".to_owned()),
                ("nvidia".to_owned(), "NVIDIA".to_owned())
            ],
            "the boot GPU first"
        );
        assert_eq!(filter(&laptop, None, nobody), Some("*radeon*"), "automatic");
        assert_eq!(
            filter(&laptop, Some("nvidia"), nobody),
            Some("*nvidia*,*nouveau*")
        );
        assert_eq!(filter(&laptop, Some("amd"), nobody), Some("*radeon*"));
        // A GPU no longer here, or unknown, is as automatic.
        assert_eq!(filter(&laptop, Some("intel"), nobody), Some("*radeon*"));
        assert_eq!(filter(&laptop, Some("other"), nobody), Some("*radeon*"));
        // Whose filter it is: the person's, or RUNE's own.
        assert_eq!(chosen_filter(&laptop, Some("amd")), Some("*radeon*"));
        assert_eq!(chosen_filter(&laptop, Some("intel")), None);
        assert_eq!(chosen_filter(&laptop, None), None);
        let chose = |name: &str| name == "RUNE_GPU";
        assert_eq!(filter(&laptop, Some("nvidia"), chose), None);
        assert_eq!(overridden(chose), Some("RUNE_GPU"));
        assert_eq!(overridden(nobody), None);
        // One per vendor; an unknown vendor cannot be chosen.
        let two = gpus(&[(AMD, true), (AMD, false), (0x1234, false)]);
        assert_eq!(choices(&two), [("amd".to_owned(), "AMD".to_owned())]);
    }
}
