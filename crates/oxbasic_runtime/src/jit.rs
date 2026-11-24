//! The Cranelift JIT for numeric `For` loops (docs/jit.md). A loop is compiled
//! only when its body is in the first step's subset: integer scalar variables,
//! integer literals, `+ - * \ Mod`, comparisons, `And`/`Or`/`Not`/`-`, and `If`
//! with nested `For` loops over the same. Anything else is left to the
//! interpreter, as is an integer result that overflows.
//!
//! The compiled code works on `i64` and checks every operation for overflow; an
//! overflow makes the call report `None`, and the caller runs the loop in the
//! interpreter, which gives VB's answer.

use oxbasic_ast::{BinaryOp, Expr, Lit, Stmt, UnaryOp};
use std::collections::HashMap;

/// The integer variables a compiled loop reads and writes, by lower-case name,
/// with their values before the loop.
pub type Vars = HashMap<String, i64>;

/// Whether `body` is entirely within the first step's subset.
pub fn supported(body: &[Stmt]) -> bool {
    body.iter().all(stmt_supported)
}

fn stmt_supported(s: &Stmt) -> bool {
    match s {
        // the debugger's line markers: compiled code does not step
        Stmt::LineMarker(_) => true,
        Stmt::Assign { target, value } => {
            matches!(target, Expr::Var(_)) && expr_supported(value)
        }
        Stmt::If { arms, else_body } => {
            arms.iter()
                .all(|a| expr_supported(&a.cond) && supported(&a.body))
                && else_body.as_ref().is_none_or(|b| supported(b))
        }
        Stmt::For {
            start,
            end,
            step,
            body,
            ..
        } => {
            expr_supported(start)
                && expr_supported(end)
                && step.as_ref().is_none_or(expr_supported)
                && supported(body)
        }
        _ => false,
    }
}

fn expr_supported(e: &Expr) -> bool {
    match e {
        Expr::Lit(Lit::Int(_)) | Expr::Var(_) => true,
        Expr::Unary { op, expr } => {
            matches!(op, UnaryOp::Neg | UnaryOp::Not) && expr_supported(expr)
        }
        Expr::Binary { op, lhs, rhs } => {
            matches!(
                op,
                BinaryOp::Add
                    | BinaryOp::Sub
                    | BinaryOp::Mul
                    | BinaryOp::IntDiv
                    | BinaryOp::Mod
                    | BinaryOp::Eq
                    | BinaryOp::Ne
                    | BinaryOp::Lt
                    | BinaryOp::Gt
                    | BinaryOp::Le
                    | BinaryOp::Ge
                    | BinaryOp::And
                    | BinaryOp::Or
            ) && expr_supported(lhs)
                && expr_supported(rhs)
        }
        _ => false,
    }
}

#[cfg(feature = "jit")]
pub use native::{compile_for, Compiled};

#[cfg(feature = "jit")]
mod native {
    use super::*;
    use cranelift_codegen::ir::condcodes::IntCC;
    use cranelift_codegen::ir::{types, AbiParam, InstBuilder, MemFlagsData, Signature, Value};
    use cranelift_codegen::isa::CallConv;
    use cranelift_codegen::settings::{self, Configurable};
    use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
    use cranelift_jit::{JITBuilder, JITModule};
    use cranelift_module::{default_libcall_names, FuncId, Linkage, Module};

    /// A compiled `For` loop. `run` takes the variables' values before the loop
    /// and gives their values after it, or `None` when the compiled code could
    /// not give the interpreter's answer (a result outside the 32-bit `Long`
    /// range, which VB promotes; a zero divisor or step, which is an error). The
    /// caller then runs the loop in the interpreter.
    pub struct Compiled {
        module: JITModule,
        func: FuncId,
        /// The variables (lower case), in the order `run` takes and returns them.
        pub names: Vec<String>,
    }

    /// Inputs, outputs, and the result: 1 when the compiled answer is valid.
    type LoopFn = unsafe extern "C" fn(*const i64, *mut i64) -> i64;

    impl Compiled {
        /// Run the loop over `values` (one per name, each within `Long`'s range).
        pub fn run(&self, values: &[i64]) -> Option<Vec<i64>> {
            if values.len() != self.names.len() {
                return None;
            }
            let code = self.module.get_finalized_function(self.func);
            // SAFETY: `code` was compiled with the signature of `LoopFn`; it reads
            // `names.len()` values from the first pointer and writes as many to
            // the second.
            let f: LoopFn = unsafe { std::mem::transmute(code) };
            let mut out = vec![0i64; values.len()];
            let valid = unsafe { f(values.as_ptr(), out.as_mut_ptr()) };
            (valid == 1).then_some(out)
        }
    }

    /// Compile `For var = start To end [Step step]` with `body` over the integer
    /// variables `names` (which include the counter). `None` when the loop is
    /// outside the subset or names a variable not in `names`.
    pub fn compile_for(
        var: &str,
        start: &Expr,
        end: &Expr,
        step: Option<&Expr>,
        body: &[Stmt],
        names: &[String],
    ) -> Option<Compiled> {
        let stmt = Stmt::For {
            var: var.to_string(),
            start: start.clone(),
            end: end.clone(),
            step: step.cloned(),
            body: body.to_vec(),
        };
        if !stmt_supported(&stmt) {
            return None;
        }
        let names: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
        let mut flags = settings::builder();
        flags.set("opt_level", "speed").ok()?;
        let isa = cranelift_native::builder()
            .ok()?
            .finish(settings::Flags::new(flags))
            .ok()?;
        let frontend = isa.frontend_config();
        let ptr = frontend.pointer_type();
        let mut module = JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()));
        let mut sig = Signature::new(CallConv::SystemV);
        sig.params.push(AbiParam::new(ptr));
        sig.params.push(AbiParam::new(ptr));
        sig.returns.push(AbiParam::new(types::I64));
        let id = module
            .declare_function("for_loop", Linkage::Export, &sig)
            .ok()?;
        let mut ctx = module.make_context();
        ctx.func.signature = sig;
        let mut fbc = FunctionBuilderContext::new();
        let mut generated = true;
        {
            let mut b = FunctionBuilder::new(&mut ctx.func, &mut fbc);
            let entry = b.create_block();
            b.append_block_params_for_function_params(entry);
            b.switch_to_block(entry);
            let inp = b.block_params(entry)[0];
            let outp = b.block_params(entry)[1];
            let valid = b.declare_var(types::I64);
            let one = b.ins().iconst(types::I64, 1);
            b.def_var(valid, one);
            let mut vars = HashMap::new();
            for (i, n) in names.iter().enumerate() {
                let v = b.declare_var(types::I64);
                let x = b.ins().load(types::I64, MemFlagsData::new(), inp, (i * 8) as i32);
                b.def_var(v, x);
                vars.insert(n.clone(), v);
            }
            let mut g = Gen { b: &mut b, vars, valid };
            generated = g.stmt(&stmt).is_some();
            for (i, n) in names.iter().enumerate() {
                let v = g.b.use_var(g.vars[n]);
                g.b.ins().store(MemFlagsData::new(), v, outp, (i * 8) as i32);
            }
            let result = g.b.use_var(g.valid);
            g.b.ins().return_(&[result]);
            b.seal_all_blocks();
            b.finalize(frontend);
        }
        if !generated {
            return None;
        }
        module.define_function(id, &mut ctx).ok()?;
        module.clear_context(&mut ctx);
        module.finalize_definitions().ok()?;
        Some(Compiled {
            module,
            func: id,
            names,
        })
    }

    struct Gen<'a, 'b> {
        b: &'a mut FunctionBuilder<'b>,
        vars: HashMap<String, Variable>,
        /// 1 while the compiled answer is still the interpreter's, else 0.
        valid: Variable,
    }

    impl Gen<'_, '_> {
        /// The answer stops being valid when `ok` (an i8 from a comparison) is 0.
        fn require(&mut self, ok: Value) {
            let ok64 = self.b.ins().uextend(types::I64, ok);
            let v = self.b.use_var(self.valid);
            let v = self.b.ins().band(v, ok64);
            self.b.def_var(self.valid, v);
        }

        /// `v` must fit a 32-bit `Long`; VB promotes anything else.
        fn in_long_range(&mut self, v: Value) -> Value {
            let narrow = self.b.ins().ireduce(types::I32, v);
            let wide = self.b.ins().sextend(types::I64, narrow);
            let same = self.b.ins().icmp(IntCC::Equal, wide, v);
            self.require(same);
            v
        }

        /// A comparison as the interpreter's Boolean, which counts as 1 or 0 in
        /// arithmetic.
        fn truth(&mut self, c: Value) -> Value {
            self.b.ins().uextend(types::I64, c)
        }

        fn cmp(&mut self, cc: IntCC, l: Value, r: Value) -> Value {
            let c = self.b.ins().icmp(cc, l, r);
            self.truth(c)
        }

        /// An integer-valued expression (not a Boolean).
        fn int(&mut self, e: &Expr) -> Option<Value> {
            match self.typed(e)? {
                (v, false) => Some(v),
                _ => None,
            }
        }

        fn expr(&mut self, e: &Expr) -> Option<Value> {
            self.typed(e).map(|t| t.0)
        }

        /// The value of `e` and whether it is a Boolean.
        fn typed(&mut self, e: &Expr) -> Option<(Value, bool)> {
            let is_cmp = |op: &BinaryOp| {
                matches!(
                    op,
                    BinaryOp::Eq
                        | BinaryOp::Ne
                        | BinaryOp::Lt
                        | BinaryOp::Gt
                        | BinaryOp::Le
                        | BinaryOp::Ge
                )
            };
            Some(match e {
                Expr::Lit(Lit::Int(v)) => (self.b.ins().iconst(types::I64, i64::from(*v)), false),
                Expr::Var(n) => {
                    let var = *self.vars.get(&n.to_lowercase())?;
                    (self.b.use_var(var), false)
                }
                Expr::Unary { op, expr } => {
                    let (v, isb) = self.typed(expr)?;
                    match op {
                        UnaryOp::Neg => {
                            let r = self.b.ins().ineg(v);
                            (self.in_long_range(r), false)
                        }
                        // logical on a Boolean, bitwise on an integer
                        UnaryOp::Not if isb => (self.b.ins().bxor_imm(v, 1), true),
                        UnaryOp::Not => (self.b.ins().bnot(v), false),
                    }
                }
                Expr::Binary { op, lhs, rhs } => {
                    let (l, lb) = self.typed(lhs)?;
                    let (r, rb) = self.typed(rhs)?;
                    let isb = is_cmp(op) || (matches!(op, BinaryOp::And | BinaryOp::Or) && lb && rb);
                    let v = match op {
                        BinaryOp::Add => {
                            let x = self.b.ins().iadd(l, r);
                            self.in_long_range(x)
                        }
                        BinaryOp::Sub => {
                            let x = self.b.ins().isub(l, r);
                            self.in_long_range(x)
                        }
                        BinaryOp::Mul => {
                            let x = self.b.ins().imul(l, r);
                            self.in_long_range(x)
                        }
                        // `\` truncates toward zero and `Mod` takes the dividend's
                        // sign, as `sdiv` and `srem` do. A zero divisor is an
                        // error in the interpreter: the answer is not valid, and
                        // the divide runs by one so it cannot trap.
                        BinaryOp::IntDiv | BinaryOp::Mod => {
                            let is_zero = self.b.ins().icmp_imm(IntCC::Equal, r, 0);
                            let nonzero = self.b.ins().icmp_imm(IntCC::NotEqual, r, 0);
                            self.require(nonzero);
                            let one = self.b.ins().iconst(types::I64, 1);
                            let safe = self.b.ins().select(is_zero, one, r);
                            let x = if *op == BinaryOp::IntDiv {
                                self.b.ins().sdiv(l, safe)
                            } else {
                                self.b.ins().srem(l, safe)
                            };
                            self.in_long_range(x)
                        }
                        BinaryOp::Eq => self.cmp(IntCC::Equal, l, r),
                        BinaryOp::Ne => self.cmp(IntCC::NotEqual, l, r),
                        BinaryOp::Lt => self.cmp(IntCC::SignedLessThan, l, r),
                        BinaryOp::Gt => self.cmp(IntCC::SignedGreaterThan, l, r),
                        BinaryOp::Le => self.cmp(IntCC::SignedLessThanOrEqual, l, r),
                        BinaryOp::Ge => self.cmp(IntCC::SignedGreaterThanOrEqual, l, r),
                        BinaryOp::And => self.b.ins().band(l, r),
                        BinaryOp::Or => self.b.ins().bor(l, r),
                        _ => return None,
                    };
                    (v, isb)
                }
                _ => return None,
            })
        }

        fn block(&mut self, body: &[Stmt]) -> Option<()> {
            body.iter().try_for_each(|s| self.stmt(s))
        }

        fn stmt(&mut self, s: &Stmt) -> Option<()> {
            match s {
                Stmt::LineMarker(_) => Some(()),
                Stmt::Assign { target: Expr::Var(n), value } => {
                    let var = *self.vars.get(&n.to_lowercase())?;
                    let v = self.int(value)?;
                    self.b.def_var(var, v);
                    Some(())
                }
                Stmt::If { arms, else_body } => {
                    let join = self.b.create_block();
                    for arm in arms {
                        let c = self.expr(&arm.cond)?;
                        let truthy = self.b.ins().icmp_imm(IntCC::NotEqual, c, 0);
                        let then_b = self.b.create_block();
                        let next = self.b.create_block();
                        self.b.ins().brif(truthy, then_b, &[], next, &[]);
                        self.b.switch_to_block(then_b);
                        self.block(&arm.body)?;
                        self.b.ins().jump(join, &[]);
                        self.b.switch_to_block(next);
                    }
                    if let Some(eb) = else_body {
                        self.block(eb)?;
                    }
                    self.b.ins().jump(join, &[]);
                    self.b.switch_to_block(join);
                    Some(())
                }
                Stmt::For { var, start, end, step, body } => {
                    let counter = *self.vars.get(&var.to_lowercase())?;
                    // start, end and step are evaluated once, as the interpreter does
                    let s0 = self.expr(start)?;
                    let e0 = self.expr(end)?;
                    let st = match step {
                        Some(e) => self.expr(e)?,
                        None => self.b.ins().iconst(types::I64, 1),
                    };
                    // a zero step is an error in the interpreter
                    let nonzero = self.b.ins().icmp_imm(IntCC::NotEqual, st, 0);
                    self.require(nonzero);
                    let i = self.b.declare_var(types::I64);
                    self.b.def_var(i, s0);
                    let head = self.b.create_block();
                    let body_b = self.b.create_block();
                    let exit = self.b.create_block();
                    // with a zero step the answer is already invalid: skip the loop
                    self.b.ins().brif(nonzero, head, &[], exit, &[]);
                    self.b.switch_to_block(head);
                    let iv = self.b.use_var(i);
                    let up = self.b.ins().icmp_imm(IntCC::SignedGreaterThanOrEqual, st, 0);
                    let le = self.b.ins().icmp(IntCC::SignedLessThanOrEqual, iv, e0);
                    let ge = self.b.ins().icmp(IntCC::SignedGreaterThanOrEqual, iv, e0);
                    let go = self.b.ins().select(up, le, ge);
                    self.b.ins().brif(go, body_b, &[], exit, &[]);
                    self.b.switch_to_block(body_b);
                    let iv = self.b.use_var(i);
                    self.b.def_var(counter, iv);
                    self.block(body)?;
                    let iv = self.b.use_var(i);
                    let next = self.b.ins().iadd(iv, st);
                    self.b.def_var(i, next);
                    self.b.ins().jump(head, &[]);
                    self.b.switch_to_block(exit);
                    Some(())
                }
                _ => None,
            }
        }
    }
}

#[cfg(all(test, feature = "jit"))]
#[path = "jit_tests.rs"]
mod tests;

