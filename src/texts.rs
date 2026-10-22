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
pub const T_ENCRYPTION: &str = "4 of 8 · Disk encryption";
pub const T_ENCRYPTION_S: &str = "4 of 8 · Encryption";
pub const T_PASSWORD: &str = "4 of 8 · Disk password";
pub const T_PASSWORD_S: &str = "4 of 8 · Password";
pub const T_ADMIN: &str = "5 of 8 · Administrator";
pub const T_REVIEW: &str = "6 of 8 · Review";
pub const T_INSTALLING: &str = "7 of 8 · Installing";
pub const T_RECOVERY: &str = "7 of 8 · Recovery key";
pub const T_BOOT_KEY: &str = "8 of 8 · Secure Boot key";
pub const T_BOOT_KEY_S: &str = "8 of 8 · Boot key";
pub const T_DONE: &str = "8 of 8 · Done";
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
pub const F_ENCRYPTION: &str = "Up/Down choose · Enter continue · Esc back · ? help";
pub const F_FORM: &str = "Tab next field · Enter continue · Esc back · ? help";
pub const F_FORM_S: &str = "Tab next · Enter continue · Esc back";
pub const F_REVIEW: &str = "Tab next · Enter select · Esc back · ? help";
pub const F_INSTALLING_CANCEL: &str = "Esc cancel · ? help";
pub const F_INSTALLING_CANCEL_S: &str = "Esc: cancel";
pub const F_INSTALLING_WAIT: &str = "Please wait";
pub const F_HANDOFF: &str = "Enter select · ? help";
pub const F_ENTER_S: &str = "Enter select";
pub const F_HANDOFF_S: &str = "Enter select · Down: more";
pub const F_DONE: &str = "Up/Down choose · Enter select · ? help";
pub const F_DONE_S: &str = "Enter: restart · Down: steps";
pub const F_ERROR: &str = "Up/Down choose · Enter select · d details · ? help";
pub const F_ERROR_AGAIN_S: &str = "Enter: start again · d details";
pub const F_ERROR_TRY_S: &str = "Enter: try again · d details";
pub const F_OVERLAY: &str = "Up/Down scroll · Esc close";
pub const F_TOO_SMALL: &str = "Make this window larger (40x4)";

// ----- Buttons and labels --------------------------------------------------------------------

pub const B_CONTINUE: &str = "Continue";
pub const B_BACK: &str = "Back";
pub const B_GO_BACK: &str = "Go back";
pub const B_RESTART: &str = "Restart";
pub const B_BACK_TO_MENU: &str = "Back to the menu";
pub const B_MATCH: &str = "They match. Continue";
pub const B_NO_MATCH: &str = "They don’t match. Restart";
pub const B_SHOW_KEY: &str = "Show the recovery key";
pub const B_CHOOSE_OTP: &str = "Choose the one-time password";
pub const B_RESTART_NOW: &str = "Restart now";
pub const B_SHOW_STEPS: &str = "Show these steps again";
pub const B_START_AGAIN: &str = "Start again";
pub const B_TRY_AGAIN: &str = "Try again";
pub const B_SHELL: &str = "Open a shell (a command line, for recovery and experts)";
pub const M_INSTALL: &str = "Install Nyra OS";
pub const M_REGISTER: &str = "Register the Secure Boot key again";
pub const M_RESTART: &str = "Restart";
pub const M_POWER_OFF: &str = "Power off";

pub fn erase_button(disk: &str) -> String {
    format!("Erase {disk} and install")
}

pub const L_PASSWORD: &str = "Password";
pub const L_REPEAT: &str = "Repeat password";
pub const L_NAME: &str = "Name";
pub const L_DISK_NAME: &str = "Disk name";
pub const L_SHOW: &str = "Show the characters";

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

// ----- Step 4: the encryption ----------------------------------------------------------------

pub const E_TITLE: &str = "How should the disk unlock?";
pub const E_ALWAYS: &str = "Nyra OS always encrypts the disk, so nobody can read it without a key.";
pub const E_TPM: &str = "Unlock with this computer’s TPM (recommended)";
pub const E_TPM_PLAIN: &str = "Unlock with this computer’s TPM";
pub const E_TPM_WHAT: &str = "The TPM is a security chip. The disk opens by itself at startup, but only when this computer starts the genuine Nyra OS. You type nothing. Whoever turns the computer on reaches the sign-in screen.";
pub const E_TPM_NA: &str =
    "Not available: this computer has no TPM, or it is switched off in the firmware settings.";
pub const E_PASSWORD: &str = "Unlock with a password";
pub const E_PASSWORD_WHAT: &str = "You type a password every time the computer starts.";
pub const E_RECOVERY_TPM: &str =
    "You also get a recovery key. It opens the disk if the TPM or the password stops working.";
pub const E_RECOVERY: &str =
    "You also get a recovery key. It opens the disk if the password stops working.";
pub const E_TPM_S: &str = "Unlock with the TPM (recommended)";
pub const E_TPM_WHAT_S: &str = "Opens by itself at startup";
pub const E_PASSWORD_WHAT_S: &str = "You type a password at startup";
pub const E_NO_TPM_S: &str = "(the TPM is not available)";

pub const P_TITLE: &str = "Choose a disk password";
pub const P_WHAT: &str = "You type this password every time the computer starts, before anything else appears. If you forget it, the recovery key opens the disk.";
pub const P_RULES: &str = "Use at least 12 characters. Letters, digits and the symbols of a US keyboard are fine. The startup screen always uses a US keyboard layout, so check the characters with “Show the characters” if your keyboard is different.";

pub const PROBLEM_MISMATCH: &str = "The passwords don’t match.";
pub const PROBLEM_SHORT: &str = "This password is too short. Use at least 12 characters.";
pub const PROBLEM_LONG: &str = "This password is too long. Use at most 64 characters.";
pub const PROBLEM_COMMON: &str = "This password is too common. Choose another one.";
pub const PROBLEM_CHARS: &str =
    "Only letters, digits and the symbols of a US keyboard can be used here.";
pub const PROBLEM_NAME_FORM: &str =
    "The name uses lowercase letters, digits, - and _, and starts with a letter.";
pub const PROBLEM_NAME_RESERVED: &str = "This name is used by the system. Choose another one.";
pub const PROBLEM_SAME: &str = "The password can’t be the same as the name.";

// ----- Step 5: the administrator -------------------------------------------------------------

pub const A_TITLE: &str = "Create the administrator";
pub const A_WHAT: &str =
    "You use this account to sign in and to change the system. The password can’t be empty.";
pub const A_RULES: &str = "The name uses lowercase letters, digits, - and _, and starts with a letter. The password needs at least 12 characters.";

// ----- Step 6: review ------------------------------------------------------------------------

pub const V_TITLE: &str = "Ready to install";
pub const V_DISK: &str = "Disk";
pub const V_UNLOCK: &str = "Unlock";
pub const V_ADMIN: &str = "Administrator";
pub const V_RELEASE: &str = "Release";
pub const V_UNLOCK_TPM: &str = "This computer’s TPM";
pub const V_UNLOCK_PASSWORD: &str = "A password you choose";
pub fn v_disk(name: &str, size: &str, model: &str) -> String {
    format!("{name} · {size} · {model}")
}
pub fn v_erase_windows(disk: &str) -> String {
    format!("Everything on {disk} will be erased, including Windows and the files on it. This can’t be undone.")
}
pub fn v_erase_data(disk: &str) -> String {
    format!("Everything on {disk} will be erased, including the files on it. This can’t be undone.")
}
pub fn v_erase_empty(disk: &str) -> String {
    format!("Everything on {disk} will be erased. This can’t be undone.")
}
pub fn v_type(disk: &str) -> String {
    format!("To go on, type the name of the disk: {disk}")
}
pub fn v_erase_s(disk: &str, typed_name: &str) -> String {
    format!("Erase {disk}? Type its name: {typed_name}")
}

// ----- Step 7: installing --------------------------------------------------------------------

pub const I_TITLE: &str = "Installing Nyra OS";
// Each step has a form for waiting and running, and a form for done.
pub const I_VERIFY: &str = "Checking the signature and the image";
pub const I_PREPARE: &str = "Preparing the disk";
pub const I_INSTALL: &str = "Installing Nyra OS";
pub const I_PROTECT: &str = "Protecting the disk";
pub const I_FINAL: &str = "Final checks";
pub const I_VERIFY_DONE: &str = "Checked the signature and the image";
pub const I_PREPARE_DONE: &str = "Prepared the disk";
pub const I_INSTALL_DONE: &str = "Installed Nyra OS";
pub const I_PROTECT_DONE: &str = "Protected the disk";
pub const I_FINAL_DONE: &str = "Final checks done";
pub const I_WAIT: &str = "Don’t turn off or restart the computer. This can take several minutes.";
pub const I_WAIT_S: &str = "Don’t turn off the computer.";
pub fn i_time(mm: u64, ss: u64) -> String {
    format!("Time so far: {mm:02}:{ss:02}")
}
pub fn i_log(path: &str) -> String {
    format!("Log: {path}")
}
pub fn i_step_s(text: &str, n: usize, of: usize) -> String {
    format!("{text} ({n} of {of})")
}
pub const LOG_PATH: &str = "/run/nyra-installer/install.log";
pub const I_CANCELLING: &str = "Stopping. Nothing on your disks has been changed.";

pub const K_TITLE: &str = "Your recovery key";
pub const K_WHAT: &str = "Next, the screen shows your recovery key: 64 letters in 8 groups. It opens the disk if the TPM or your password stops working.";
pub const K_WHAT_PASSWORD: &str = "Next, the screen shows your recovery key: 64 letters in 8 groups. It opens the disk if your password stops working.";
pub const K_POINTS: [&str; 4] = [
    "Write it down on paper, or take a photo of the screen with your phone.",
    "It is shown only once and saved nowhere on this computer.",
    "Anyone who has it can open the disk. Keep it somewhere safe.",
    "Then you type it back, to check that you copied it right.",
];
pub const K_WRITE_S: &str = "Write the next screen down.";

// ----- Step 8: the Secure Boot key and the end -----------------------------------------------

pub const O_TITLE: &str = "Register the Secure Boot key";
pub const O_WHAT: &str = "Nyra OS starts from files signed with a Nyra key. Your computer has to trust that key once. For this you choose a one-time password now, and type it on a blue screen right after the restart.";
pub const O_POINTS: [&str; 2] = [
    "Use letters and digits only, about 8 of them.",
    "You need it only once. Nothing else uses it.",
];
pub const O_NEXT: &str = "Next, you type the password twice. Nothing shows while you type.";
pub const O_ONE_TIME_S: &str = "Choose a one-time password.";

pub const Z_TITLE: &str = "Nyra OS is installed";
pub const Z_TITLE_AGAIN: &str = "The Secure Boot key is registered";
pub const Z_TITLE_S: &str = "Nyra OS is installed.";
pub const Z_TITLE_AGAIN_S: &str = "The key is registered.";
pub const Z_NEXT_S: &str = "Next: the blue screen. Down: steps";
pub const Z_WHAT: &str = "One more step before it starts. A blue screen asks you to trust the Nyra key. It appears right after the restart and waits only 10 seconds.";
pub const Z_STEP1: &str = "Remove the installation USB drive or disc.";
pub const Z_STEP1_UTM: &str = "In UTM, remove the installer disc: open the CD/DVD drive menu and choose “Eject”. Click the UTM window so it gets your keyboard.";
pub const Z_STEPS: [&str; 5] = [
    "Restart. When the blue screen says “Press any key”, press a key.",
    "Choose “Enroll MOK”.",
    "Choose “View key 0” and compare the name and fingerprint with the ones below. Then choose “Continue”.",
    "Choose “Continue”, then “Yes”.",
    "Type the one-time password. Then choose “Reboot”.",
];
pub const Z_NEVER: &str = "Never choose “Change Secure Boot state” or “Delete MOK”.";
pub const Z_NEVER_UTM: &str = "Never choose “Change Secure Boot state” or “Delete MOK”. In UTM, don’t use “Reset UEFI variables”: it erases the key.";
pub const Z_MISSED: &str = "If you miss the blue screen, start the installer again and choose “Register the Secure Boot key again”.";

// ----- Errors --------------------------------------------------------------------------------

pub const X_CHECK: &str = "The Nyra OS image didn’t pass the check.";
pub const X_CHECK_WHY: &str = "Its signature or its digest doesn’t match. This installer’s USB drive or disc may be damaged, or it may have been changed.";
pub const X_CHECK_DO: &str = "What you can do: download the installer again and verify it on your own computer before you write it to the USB drive.";
pub const X_CHECK_S: &str = "The image didn’t pass the check.";
pub const X_CHECK_T: &str = "Check failed";

pub const X_MEMORY: &str = "There isn’t enough memory to prepare Nyra OS.";
pub const X_MEMORY_WHY: &str =
    "The installer copies the Nyra OS image into memory before it writes anything to a disk.";
pub const X_MEMORY_DO: &str = "What you can do: add memory to this computer, or give the virtual machine more, then start the installer again.";
pub const X_MEMORY_S: &str = "Not enough memory.";

pub const X_WRITE: &str = "Could not write to the disk.";
pub const X_WRITE_WHY: &str =
    "The disk may be failing, or it was disconnected during the installation.";
pub const X_WRITE_DO: &str =
    "What you can do: check the cable or try another disk, then choose “Start again”.";

pub const X_PROTECT: &str = "The disk protection didn’t finish.";
pub const X_PROTECT_WHY: &str = "Setting up the encryption of the disk stopped before it was done.";
pub const X_PROTECT_DO: &str =
    "What you can do: try another disk, or check the cable, then choose “Start again”.";

pub const X_MOK: &str = "The Secure Boot key couldn’t be registered.";
pub const X_MOK_WHY: &str = "Nyra OS is installed, but it won’t start until the key is registered.";
pub const X_MOK_DO: &str = "What you can do: choose “Try again”.";
pub const X_MOK_S: &str = "The key couldn’t be registered.";
pub const X_MOK_DISK_S: &str = "Nyra OS is installed. Try again.";
pub const X_MOK_T: &str = "Key not registered";

pub const X_STOPPED: &str = "The installation stopped unexpectedly.";
pub const X_STOPPED_WHY: &str = "The installation program ended before it finished.";
pub const X_STOPPED_DO: &str =
    "What you can do: choose “Start again”. If it happens again, press d to see the details.";
pub const X_STOPPED_S: &str = "The installation stopped.";

pub const X_FAILED_T: &str = "Installation failed";

pub fn x_changed(disk: &str) -> String {
    format!("The disk {disk} was changed and can’t be used as it is. Install again from the start. Nothing on it can be kept.")
}
pub fn x_maybe_changed(disk: &str) -> String {
    format!("The disk {disk} may have been changed and can’t be used as it is. Install again from the start. Nothing on it can be kept.")
}
pub fn x_changed_s(disk: &str) -> String {
    format!("{disk} was changed. Install again.")
}
pub fn x_maybe_changed_s(disk: &str) -> String {
    format!("{disk} may be changed. Install again.")
}
pub fn x_details_hint(path: &str) -> String {
    format!("Details: d · Log: {path} (gone at restart)")
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
pub const DT_LOG: &str = "The last lines of the log:";

pub const H_KEYS: [&str; 7] = [
    "Up/Down or Tab: choose an item or move between fields.",
    "Enter: select the item, or continue.",
    "Esc: go back, where going back is possible.",
    "Space: tick a box. In a field, it types a space.",
    "PgUp/PgDn, Home/End: scroll text that is longer than the screen.",
    "d: show details, where the bottom line mentions it.",
    "Ctrl-L: draw the screen again and measure its size again. Use it on a serial console when the screen doesn’t fit.",
];
pub const H_FIELD: &str = "In a field, ? types a question mark. Press F1 for this help instead.";
pub const H_MENU: &str = "“Register the Secure Boot key again” is for when the blue screen after an installation was missed. “Open a shell” gives a command line for recovery and experts. Type exit to come back.";
pub const H_RELEASE: &str = "These values come from this installer’s own disk. A changed installer would show its own values, so this screen is a second check, not a guarantee. The guarantee is the signature of the installer file, which you verify on your own computer before you write it to the USB drive.";
pub const H_DISK: &str = "Nyra OS can’t be installed next to Windows in this version. To keep Windows, choose another disk. The disk you started from can’t be chosen.";
pub const H_INSTALLING: &str = "Until the first change to the disk, Esc cancels the installation. After that, it can’t be stopped.";
pub const H_RECOVERY_SERIAL: &str = "On a serial console the key crosses the serial line. Use the computer’s own screen if others can see the serial log.";
pub const H_BLUE: &str = "The blue screen belongs to the computer’s firmware, not to Nyra OS. Its menus can be named a little differently.";
