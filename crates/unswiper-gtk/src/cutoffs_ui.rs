// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! "Blur Cutoffs…": the blurry-below sharpness score for Low, Normal and High.

use relm4::adw::prelude::*;
use relm4::{adw, gtk};
use unswiper_core::quality::{Cutoffs, Sensitivity};

/// What the dialog explains above the numbers.
pub const EXPLANATION: &str = "Each photo gets a sharpness score: how strongly its finest detail \
changes from pixel to pixel (Laplacian variance, measured on a copy at most 1024 pixels wide). \
Higher means sharper. Clearly blurry photos usually score in single digits; sharp photos \
usually score in the tens or more, detailed ones in the hundreds or thousands.\n\nA photo is suggested as blurry \
when its score is below the number for the chosen sensitivity. Lower numbers flag fewer photos. \
Low must be below Normal, and Normal below High.\n\nPhotos also count as blurry when they look \
shaken (score below 4 × the number) or are much softer than the best shot of a burst. Saving \
re-applies the marks from the saved scores; nothing is scanned again.";

/// The text under the numbers: the problem, or which number is in use.
pub fn status(c: Cutoffs, current: Sensitivity) -> (bool, String) {
    match c.validate() {
        Ok(()) => (
            true,
            format!(
                "In use now ({}): blurry below {:.1}.",
                current.as_str(),
                c.get(current)
            ),
        ),
        Err(e) => (false, e),
    }
}

/// Shows the dialog; `on_save` gets valid new cutoffs.
pub fn present(
    parent: &impl IsA<gtk::Widget>,
    current: Cutoffs,
    sensitivity: Sensitivity,
    on_save: impl Fn(Cutoffs) + 'static,
) {
    let d = adw::AlertDialog::new(Some("Blur Cutoffs"), Some(EXPLANATION));
    d.add_response("cancel", "Cancel");
    d.add_response("save", "Save");
    d.set_response_appearance("save", adw::ResponseAppearance::Suggested);
    d.set_default_response(Some("save"));
    d.set_close_response("cancel");

    let grid = gtk::Grid::builder()
        .row_spacing(8)
        .column_spacing(12)
        .halign(gtk::Align::Center)
        .build();
    let spin = |v: f32| {
        let s = gtk::SpinButton::with_range(0.1, f64::from(Cutoffs::MAX), 0.5);
        s.set_digits(1);
        s.set_value(f64::from(v));
        s.set_numeric(true);
        s
    };
    let spins = [spin(current.low), spin(current.normal), spin(current.high)];
    for (row, (s, sens)) in spins.iter().zip(Sensitivity::ALL).enumerate() {
        let label = gtk::Label::builder()
            .label(format!("{}: blurry below", sens.label()))
            .xalign(0.0)
            .build();
        grid.attach(&label, 0, row as i32, 1, 1);
        grid.attach(s, 1, row as i32, 1, 1);
    }
    let note = gtk::Label::builder()
        .wrap(true)
        .css_classes(["dim-label"])
        .build();
    let reset = gtk::Button::builder()
        .label("Reset to Defaults")
        .tooltip_text(format!(
            "Low {}, Normal {}, High {}",
            Cutoffs::DEFAULT.low,
            Cutoffs::DEFAULT.normal,
            Cutoffs::DEFAULT.high
        ))
        .halign(gtk::Align::Center)
        .build();
    let body = gtk::Box::new(gtk::Orientation::Vertical, 10);
    body.append(&grid);
    body.append(&note);
    body.append(&reset);
    d.set_extra_child(Some(&body));

    let read = {
        let spins = spins.clone();
        move || Cutoffs {
            low: spins[0].value() as f32,
            normal: spins[1].value() as f32,
            high: spins[2].value() as f32,
        }
    };
    let update = {
        let (d, note, read) = (d.clone(), note.clone(), read.clone());
        move || {
            let (ok, text) = status(read(), sensitivity);
            note.set_label(&text);
            d.set_response_enabled("save", ok);
        }
    };
    update();
    for s in &spins {
        let update = update.clone();
        s.connect_value_changed(move |_| update());
    }
    {
        let spins = spins.clone();
        reset.connect_clicked(move |_| {
            let c = Cutoffs::DEFAULT;
            for (s, v) in spins.iter().zip([c.low, c.normal, c.high]) {
                s.set_value(f64::from(v));
            }
        });
    }
    d.connect_response(None, move |_, r| {
        if r == "save" {
            let c = read();
            if c.validate().is_ok() {
                on_save(c);
            }
        }
    });
    d.present(Some(parent));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_names_the_number_in_use_or_the_problem() {
        let (ok, t) = status(Cutoffs::DEFAULT, Sensitivity::Normal);
        assert!(ok);
        assert_eq!(t, "In use now (normal): blurry below 13.0.");
        let bad = Cutoffs {
            low: 14.0,
            ..Cutoffs::DEFAULT
        };
        let (ok, t) = status(bad, Sensitivity::Normal);
        assert!(!ok);
        assert!(t.contains("Low below Normal"), "{t}");
    }
}
