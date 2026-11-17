//! `oxbasic` — OxidizedBASIC command-line front end.
//!
//! Subcommands:
//!   oxbasic run <file.bas>   run a BASIC program
//!   oxbasic repl             interactive immediate window
//!   oxbasic ide              launch the IDE (Phase 3 placeholder)
//!   oxbasic version          print version

use oxbasic_runtime::{ConsoleOutput, Interpreter};
use std::io::{BufRead, Write};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("run") => cmd_run(&args[2..]),
        Some("repl") => cmd_repl(),
        Some("ide") => cmd_ide(),
        Some("version") | Some("--version") => {
            println!("oxbasic {} (Phase 1: parser + interpreter)", env!("CARGO_PKG_VERSION"))
        }
        _ => {
            eprintln!("OxidizedBASIC — a Rust-native classic BASIC suite");
            eprintln!();
            eprintln!("USAGE:");
            eprintln!("  oxbasic run <file.bas>   run a BASIC program");
            eprintln!("  oxbasic repl             interactive immediate window");
            eprintln!("  oxbasic ide              launch the IDE (Phase 3 placeholder)");
            eprintln!("  oxbasic version          print version");
            std::process::exit(2);
        }
    }
}

fn cmd_run(args: &[String]) {
    let path = match args.first() {
        Some(p) => p,
        None => {
            eprintln!("usage: oxbasic run <file.bas>");
            std::process::exit(2);
        }
    };
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot read {}: {}", path, e);
            std::process::exit(1);
        }
    };
    let prog = match oxbasic_parser::parse(&src) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };
    let mut out = ConsoleOutput;
    if let Err(e) = oxbasic_runtime::run_program(&prog, &mut out) {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}

fn cmd_repl() {
    let mut out = ConsoleOutput;
    let mut it = Interpreter::new(&oxbasic_ast::Program { items: vec![] }, &mut out);
    println!("OxidizedBASIC immediate window — enter statements, ? <expr> to print, exit to quit");
    let stdin = std::io::stdin();
    loop {
        print!("ox> ");
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.eq_ignore_ascii_case("exit") || line.eq_ignore_ascii_case("quit") {
            break;
        }
        let src = if let Some(rest) = line.strip_prefix('?') {
            format!("Print {}", rest)
        } else {
            line.to_string()
        };
        if let Err(e) = it.repl_line(&format!("{}\n", src)) {
            println!("{}", e);
        }
    }
}

fn cmd_ide() {
    // Phase 3 per the roadmap: GPUI form designer, tree-sitter editor,
    // property inspector, and debugger. The CLI ships first.
    println!("OxidizedBASIC IDE (GPUI) is under development — Phase 3 of the roadmap.");
    println!("For now, use: oxbasic run <file.bas>  /  oxbasic repl");
}
