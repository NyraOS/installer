// SPDX-License-Identifier: GPL-3.0-or-later

//! Every text the installer shows. English is the source language. A message is one whole
//! sentence or paragraph with its values as parameters, never pieces glued together, so that a
//! translation can reorder it. `tests/texts.rs` checks this file against the writing rules.

// ----- Bars ----------------------------------------------------------------------------------

pub const TITLE: &str = "Nyra OS installer";

pub fn menu_title(version: &str, arch: &str) -> String {
    format!("Version {version} · {arch}")
}
pub fn menu_title_short(n: usize, of: usize) -> String {
    format!("Menu {n} of {of}")
}

// Step names, long and short. The short form is for 40 columns.
pub const T_COMPUTER: &str = "1 of 8 · This computer";
pub const T_COMPUTER_S: &str = "1 of 8 · Computer";
pub const T_RELEASE: &str = "2 of 8 · This release";
pub const T_RELEASE_S: &str = "2 of 8 · Release";
pub const T_DISK: &str = "3 of 8 · Disk";
pub const T_HELP: &str = "Help";
pub const T_DETAILS: &str = "Details";

// Footers: the full line, then the short one for small windows.
pub const F_MENU: &str = "Up/Down choose · Enter select · ? help";
pub const F_MENU_S: &str = "Up/Down choose · Enter select";
pub const F_COMPUTER: &str = "Up/Down choose · Enter select · d details · ? help";
pub const F_FAIL: &str = F_COMPUTER;
pub const F_FAIL_S: &str = "Enter: restart · d details";
pub const F_FAIL_S_MORE: &str = "Enter: restart · d details · Down: more";
pub const F_RELEASE: &str = "Up/Down choose · Enter select · Esc back · ? help";
pub const F_RELEASE_S: &str = "Down: read · Enter: continue";
pub const F_DISK: &str = "Up/Down choose · Enter continue · Esc back · d details · ? help";
pub const F_DISK_S: &str = "Up/Down choose · Enter next · Esc back";
pub const F_OVERLAY: &str = "Up/Down scroll · Esc close";
pub const F_TOO_SMALL: &str = "Make this window larger (40x4)";

// ----- Buttons and labels --------------------------------------------------------------------

pub const B_CONTINUE: &str = "Continue";
pub const B_RESTART: &str = "Restart";
pub const B_BACK_TO_MENU: &str = "Back to the menu";
pub const B_MATCH: &str = "They match. Continue";
pub const B_NO_MATCH: &str = "They don’t match. Restart";
pub const B_SHELL: &str = "Open a shell (a command line, for recovery and experts)";
pub const M_INSTALL: &str = "Install Nyra OS";
pub const M_REGISTER: &str = "Register the Secure Boot key again";
pub const M_RESTART: &str = "Restart";
pub const M_POWER_OFF: &str = "Power off";

// ----- Menu ----------------------------------------------------------------------------------

pub const WELCOME: &str = "Welcome to Nyra OS.";
pub const MENU_INTRO: &str =
    "This installs Nyra OS on this computer. It doesn’t need an internet connection.";
pub const MENU_NOTE: &str = "Installs Nyra OS on one disk. Nothing is erased until you confirm.";
pub const SHELL_WARNING: &str =
    "This is a temporary system in memory. Type exit to come back to the installer.";

// ----- Step 1: the computer ------------------------------------------------------------------

pub const C_TITLE: &str = "Checking this computer";
pub const C_UEFI: &str = "Started in UEFI mode";
pub const C_SECURE_BOOT: &str = "Secure Boot is on";
pub const C_TPM: &str = "TPM found (a security chip, used to unlock the disk)";
pub const C_NO_TPM: &str =
    "No TPM found. You can still install and unlock the disk with a password.";
pub fn c_memory(tenths: u32, need: u32) -> String {
    format!(
        "Memory: {}.{} GiB (the installer needs at least {need} GiB)",
        tenths / 10,
        tenths % 10
    )
}
pub fn c_disks(n: usize) -> String {
    format!("Disks found: {n}")
}
pub const C_READY_TPM: &str = "This computer is ready. No internet connection is needed.";
pub const C_READY: &str = "This computer is ready.";
pub fn c_all_passed(n: usize) -> String {
    format!("All {n} checks passed.")
}
pub const C_NO_TPM_S: &str = "No TPM. A password will do.";

pub const NO_CHANGE: &str = "Nothing on your disks was changed.";

pub const FAIL_BIOS: &str = "This computer started in the old BIOS mode.";
pub const FAIL_BIOS_WHY: &str = "Nyra OS needs UEFI, the modern way a computer starts.";
pub const FAIL_BIOS_DO: &str = "What you can do: restart, open your computer’s firmware settings (often by pressing Del, F2 or Esc as it starts) and start in UEFI mode. Then start this installer again.";
pub const FAIL_BIOS_S: &str = "Start in UEFI mode in the firmware.";
pub const FAIL_BIOS_T: &str = "Started in BIOS mode";

pub const FAIL_SB: &str = "Secure Boot is off.";
pub const FAIL_SB_WHY: &str = "Nyra OS needs Secure Boot. It protects the disk and checks the files that Nyra OS starts from.";
pub const FAIL_SB_DO: &str = "What you can do: restart, open your computer’s firmware settings (often by pressing Del, F2 or Esc as it starts) and turn Secure Boot on. Then start this installer again.";
pub const FAIL_SB_UTM: &str = "In UTM, open the virtual machine’s settings and turn on the TPM 2.0 device and Secure Boot keys (“Preload Secure Boot Keys”). The names can differ between UTM versions.";
pub const FAIL_SB_S: &str = "Turn Secure Boot on in the firmware.";

pub const FAIL_SHIM: &str = "Secure Boot checks are switched off.";
pub const FAIL_SHIM_WHY: &str = "Nyra OS needs them to check the files it starts from. They were switched off on this computer.";
pub const FAIL_SHIM_DO: &str = "What you can do: restart. On the blue screen that appears as the computer starts, choose “Change Secure Boot state” and switch the checks on. Then start this installer again.";
pub const FAIL_SHIM_S: &str = "Switch the checks on at startup.";
pub const FAIL_SHIM_T: &str = "Secure Boot checks off";

pub fn fail_memory(have_tenths: u32, need: u32) -> String {
    format!(
        "This computer has {}.{} GiB of memory. The installer needs at least {need} GiB.",
        have_tenths / 10,
        have_tenths % 10
    )
}
pub const FAIL_MEMORY_DO: &str = "What you can do: add memory to this computer. In a virtual machine, give it more memory. Then start this installer again.";
pub const FAIL_MEMORY_S: &str = "Add memory to this computer.";
pub const FAIL_MEMORY_T: &str = "Not enough memory";

pub const FAIL_NO_DISK: &str = "No disk found to install on.";
pub const FAIL_NO_DISK_WHY: &str =
    "The installer sees no disk it can use. The disk you started from can’t be used.";
pub const FAIL_NO_DISK_DO: &str =
    "What you can do: check that the disk is connected, then restart.";
pub const FAIL_NO_DISK_S: &str = "Check that the disk is connected.";
pub const FAIL_NO_DISK_T: &str = "No disk found";

// ----- Step 2: the release -------------------------------------------------------------------

pub const R_TITLE: &str = "You are installing this release";
pub const R_VERSION: &str = "Version";
pub const R_COMMIT: &str = "Commit";
pub const R_DIGEST: &str = "Image digest";
pub const R_SIGNER: &str = "Signed by";
pub const R_FINGERPRINT: &str = "Certificate fingerprint (SHA-256)";
pub const R_FINGERPRINT_S: &str = "Fingerprint (SHA-256)";
pub const R_TESTING: &str = "This release is for testing (channel “dev”).";
pub const R_COMPARE: &str = "Compare these values with the release page on github.com, opened on another device. If anything is different, stop here.";
pub fn r_more(n: usize) -> String {
    format!("Compare the rest: Down ({n} more)")
}

// ----- Step 3: the disk ----------------------------------------------------------------------

pub const D_TITLE: &str = "Choose the disk for Nyra OS";
pub const D_NOTHING_YET: &str = "Nothing is erased yet. You confirm at the end.";
pub const D_COL_DISK: &str = "Disk";
pub const D_COL_SIZE: &str = "Size";
pub const D_COL_MODEL: &str = "Model";
pub const D_COL_CONTENT: &str = "On the disk";
pub const D_UNAVAILABLE: &str = "Not available";
pub const D_EMPTY: &str = "Empty";
pub const D_DATA: &str = "Contains data";
pub fn d_windows(parts: usize) -> String {
    if parts == 1 {
        "Windows, 1 partition".to_string()
    } else {
        format!("Windows, {parts} partitions")
    }
}
pub const D_BOOTED: &str = "You started from this disk";
pub const D_IN_USE: &str = "In use";
pub const D_READ_ONLY: &str = "Read-only";
pub fn d_too_small(min_gb: u32) -> String {
    format!("Too small (needs at least {min_gb} GB)")
}
pub fn d_on(disk: &str) -> String {
    format!("On {disk}")
}
pub fn d_more_parts(n: usize) -> String {
    format!("and {n} more")
}
pub const D_WARN_WINDOWS: &str = "Windows is on this disk. Installing Nyra OS here removes it.";
pub const D_WARN_DATA: &str = "This disk contains data. Installing Nyra OS here erases it.";
pub const D_OTHERS: &str = "Other disks are not touched.";
pub fn d_position(n: usize, of: usize) -> String {
    format!("({n} of {of})")
}

pub fn v_disk(name: &str, size: &str, model: &str) -> String {
    format!("{name} · {size} · {model}")
}
// ----- Details and help ----------------------------------------------------------------------

pub const DT_NONE: &str = "There are no details for this screen.";
pub fn dt_check(uefi: bool, sb: bool, shim: bool, tpm: bool, virt: bool) -> String {
    let yes = |b: bool| if b { "yes" } else { "no" };
    format!(
        "UEFI: {}. Secure Boot: {}. Secure Boot checks: {}. TPM: {}. Virtual machine: {}.",
        yes(uefi),
        yes(sb),
        yes(shim),
        yes(tpm),
        yes(virt)
    )
}
pub fn dt_disk(path: &str, removable: bool) -> String {
    format!(
        "Path: {path}. Removable: {}.",
        if removable { "yes" } else { "no" }
    )
}
pub fn dt_part(n: usize, label: &str, fs: &str, size: &str) -> String {
    format!("Partition {n}: {label} {fs} {size}")
}
pub const H_KEYS: [&str; 7] = [
    "Up/Down or Tab: choose an item or move between fields.",
    "Enter: select the item, or continue.",
    "Esc: go back, where going back is possible.",
    "Space: tick a box. In a field, it types a space.",
    "PgUp/PgDn, Home/End: scroll text that is longer than the screen.",
    "d: show details, where the bottom line mentions it.",
    "Ctrl-L: draw the screen again and measure its size again. Use it on a serial console when the screen doesn’t fit.",
];
pub const H_MENU: &str = "“Register the Secure Boot key again” is for when the blue screen after an installation was missed. “Open a shell” gives a command line for recovery and experts. Type exit to come back.";
pub const H_RELEASE: &str = "These values come from this installer’s own disk. A changed installer would show its own values, so this screen is a second check, not a guarantee. The guarantee is the signature of the installer file, which you verify on your own computer before you write it to the USB drive.";
pub const H_DISK: &str = "Nyra OS can’t be installed next to Windows in this version. To keep Windows, choose another disk. The disk you started from can’t be chosen.";
