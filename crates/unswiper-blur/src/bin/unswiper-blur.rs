// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! `unswiper-blur`: find likely blurry photos (never moves anything), photo info, and the Mac
//! app's Google Takeout import (`takeout …`, see [`unswiper_takeout::cli`]). See
//! [`unswiper_blur::cli`].

use std::io::Write;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // The Mac app's Takeout import streams progress, so it writes as it goes.
    if args.first().map(String::as_str) == Some("takeout") {
        let (mut out, mut err) = (std::io::stdout(), std::io::stderr());
        return match unswiper_takeout::cli::run(&args[1..], &mut out, &mut err) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                let _ = writeln!(err, "{e}");
                ExitCode::FAILURE
            }
        };
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (code, out, err) = unswiper_blur::cli::main_with(&args, now);
    let _ = std::io::stdout().write_all(out.as_bytes());
    let _ = std::io::stderr().write_all(err.as_bytes());
    ExitCode::from(code)
}
