// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Rob Ranslam

//! `unswiper-blur`: find likely blurry photos (never moves anything), photo info, and the Mac
//! app's Google Takeout import (`takeout …`, see [`unswiper_takeout::cli`]), place names
//! (`geo`, JSON on stdin, see [`unswiper_blur::geo`]). See
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
    // Place names for the Mac app: one JSON request on stdin, one JSON answer on stdout.
    if args.first().map(String::as_str) == Some("geo") {
        let mut input = String::new();
        let _ = std::io::Read::read_to_string(&mut std::io::stdin(), &mut input);
        return match unswiper_blur::geo::run(&input) {
            Ok(out) => {
                println!("{out}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        };
    }
    // Grouping by place for the Mac app (plan §20.6): reads library.json, prints groups.
    if args.first().map(String::as_str) == Some("group") {
        return match unswiper_blur::geo::parse_group(&args)
            .and_then(|(lib, mode)| unswiper_blur::geo::group(std::path::Path::new(&lib), mode))
        {
            Ok(out) => {
                println!("{out}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
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
