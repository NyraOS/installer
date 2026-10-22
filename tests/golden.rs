// SPDX-License-Identifier: GPL-3.0-or-later

//! Every screen at four sizes, compared with the files in `tests/golden/`. The 80x24 and 40x4
//! files are the mock-ups of the design; the other sizes are checked once by eye and kept.
//! `UPDATE_GOLDEN=new cargo test` writes the files that are missing.

use nyra_installer::app::App;
use nyra_installer::backend::Fake;
use nyra_installer::screens::frame;
use nyra_installer::state::{Failure, Screen};

const SIZES: [(usize, usize); 4] = [(80, 24), (40, 4), (60, 15), (120, 40)];

struct Case {
    name: &'static str,
    scenario: &'static str,
    screen: Screen,
    focus: usize,
}

fn cases() -> Vec<Case> {
    let c = |name, scenario, screen, focus| Case {
        name,
        scenario,
        screen,
        focus,
    };
    vec![
        c("menu", "ok", Screen::Menu, 0),
        c("computer-tpm", "ok", Screen::Computer, 0),
        c("computer-no-tpm", "no-tpm", Screen::Computer, 0),
        c(
            "fail-secure-boot",
            "secure-boot-off",
            Screen::Fail(Failure::SecureBoot),
            0,
        ),
        c("release", "ok", Screen::Release, 0),
        c("disk", "ok", Screen::Disk, 0),
    ]
}

fn render(case: &Case, w: usize, h: usize) -> String {
    let mut app = App::new(Fake::new(case.scenario).unwrap(), w, h);
    app.state.machine = Some(app.backend.machine.clone());
    app.state.screen = case.screen;
    app.state.focus = case.focus;
    frame(&app.state).grid.to_text(true)
}

#[test]
fn golden_screens() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let update = std::env::var("UPDATE_GOLDEN").is_ok_and(|v| v == "new");
    let mut bad = Vec::new();
    for case in cases() {
        for (w, h) in SIZES {
            let file = dir.join(format!("{}.{w}x{h}.txt", case.name));
            let got = render(&case, w, h);
            match std::fs::read_to_string(&file) {
                Ok(want) if want == got => {}
                Ok(want) => bad.push(format!(
                    "{}:\n--- wanted\n{want}--- got\n{got}",
                    file.display()
                )),
                Err(_) if update => std::fs::write(&file, &got).unwrap(),
                Err(e) => bad.push(format!("{}: {e}", file.display())),
            }
        }
    }
    assert!(
        bad.is_empty(),
        "{} screens differ:\n{}",
        bad.len(),
        bad.join("\n")
    );
}
