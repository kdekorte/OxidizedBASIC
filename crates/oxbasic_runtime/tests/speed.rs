//! How fast the interpreter runs a tight numeric loop: the Mandelbrot
//! sample's own loop, at a small size. Run it with
//! `cargo test --release -p oxbasic_runtime --test speed -- --ignored --nocapture`;
//! it prints the time with and without a debugger attached (as in the IDE).

use oxbasic_runtime::{DebugAction, Interpreter, Output};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Instant;

struct Quiet;
impl Output for Quiet {
    fn write(&mut self, _s: &str) {}
}

fn mandelbrot(w: u32, h: u32) -> String {
    format!(
        "Dim maxiter As Long, x As Long, y As Long, k As Long, n As Long, z As Long\n\
         Dim cr As Double, ci As Double, zr As Double, zi As Double, zr2 As Double, zi2 As Double\n\
         maxiter = 128\n\
         For y = 0 To {h1}\n\
           ci = -1.05 + y * (2.1 / {h})\n\
           For x = 0 To {w1}\n\
             cr = -2.0 + x * (2.8 / {w})\n\
             zr = 0.0: zi = 0.0: zr2 = 0.0: zi2 = 0.0\n\
             k = 0\n\
             While (zr2 + zi2 <= 4.0) And (k < maxiter)\n\
               zi = 2.0 * zr * zi + ci\n\
               zr = zr2 - zi2 + cr\n\
               zr2 = zr * zr\n\
               zi2 = zi * zi\n\
               k = k + 1\n\
             Wend\n\
             If k = maxiter Then n = 0 Else n = (k * 3 + 16) Mod 256\n\
             z = z + n\n\
           Next x\n\
         Next y\n",
        w = w,
        h = h,
        w1 = w - 1,
        h1 = h - 1
    )
}

fn time(src: &str, debugger: bool) -> f64 {
    let prog = oxbasic_parser::parse(src).unwrap();
    let mut out = Quiet;
    let mut rt = Interpreter::new(&prog, &mut out);
    if debugger {
        // what the IDE installs: breakpoints (none set) and a hook
        rt.set_breakpoints(Arc::new(Mutex::new(HashSet::new())));
        let mut first = true;
        rt.set_debug_hook(Box::new(move |_| {
            first = false;
            DebugAction::Continue
        }));
    }
    let t = Instant::now();
    rt.run().unwrap();
    t.elapsed().as_secs_f64()
}

#[test]
#[ignore = "a timing, not a check; run in release"]
fn mandelbrot_loop_speed() {
    let src = mandelbrot(320, 240);
    let plain = time(&src, false);
    let debug = time(&src, true);
    println!(
        "320x240 Mandelbrot: {:.2} s plain, {:.2} s with a debugger",
        plain, debug
    );
}
