//! Bytecode compiler: lowers a `CompiledExpr` tree into a flat `BytecodeProgram`.
//!
//! The compiler does a single recursive traversal of the `CompiledExpr` tree,
//! emitting `Instr` values into a flat `Vec`.  After emission, a lightweight
//! peephole pass simplifies common instruction sequences.

use crate::evaluator::{CompiledArithOp, CompiledCmp, CompiledExpr};
use crate::value::JValue;
use crate::vm::{BytecodeProgram, Instr};

// ---------------------------------------------------------------------------
// Compiler state
// ---------------------------------------------------------------------------

pub(crate) struct BytecodeCompiler {
    instrs: Vec<Instr>,
    const_pool: Vec<JValue>,
    string_pool: Vec<String>,
    /// Reverse index for interning: string → pool index.
    string_intern: std::collections::HashMap<String, u16>,
    /// Fallback expressions for `EvalFallback` instructions.
    /// Contains `CompiledExpr` variants too complex to lower to flat bytecode.
    fallback_exprs: Vec<CompiledExpr>,
    /// Sub-programs for `FilterByBytecode` instructions.
    /// Each is a fully compiled predicate `BytecodeProgram` run per array element.
    sub_programs: Vec<BytecodeProgram>,
}

impl BytecodeCompiler {
    pub(crate) fn new() -> Self {
        BytecodeCompiler {
            instrs: Vec::with_capacity(32),
            const_pool: Vec::new(),
            string_pool: Vec::new(),
            string_intern: std::collections::HashMap::new(),
            fallback_exprs: Vec::new(),
            sub_programs: Vec::new(),
        }
    }

    // ── Internal helpers ────────────────────────────────────────────────

    fn emit(&mut self, instr: Instr) {
        self.instrs.push(instr);
    }

    /// Return the current instruction pointer (index of next instruction to be emitted).
    fn here(&self) -> usize {
        self.instrs.len()
    }

    /// Intern a `JValue` constant and return its pool index.
    fn intern_const(&mut self, v: JValue) -> u16 {
        // Linear scan is fine for small const pools (< 256 entries typical).
        for (i, c) in self.const_pool.iter().enumerate() {
            if values_eq(c, &v) {
                return i as u16;
            }
        }
        let idx = self.const_pool.len() as u16;
        self.const_pool.push(v);
        idx
    }

    /// Intern a string and return its pool index.
    fn intern_str(&mut self, s: &str) -> u16 {
        if let Some(&idx) = self.string_intern.get(s) {
            return idx;
        }
        let idx = self.string_pool.len() as u16;
        self.string_pool.push(s.to_string());
        self.string_intern.insert(s.to_string(), idx);
        idx
    }

    /// Store a `CompiledExpr` in the fallback pool and return its index.
    /// Used for `EvalFallback` instructions when an expression is too complex
    /// to lower to flat bytecode directly.
    fn add_fallback(&mut self, expr: CompiledExpr) -> u16 {
        let idx = self.fallback_exprs.len() as u16;
        self.fallback_exprs.push(expr);
        idx
    }

    /// Emit a placeholder jump and return its index for later patching.
    fn emit_jump_placeholder(&mut self, placeholder: Instr) -> usize {
        let pos = self.here();
        self.emit(placeholder);
        pos
    }

    /// Patch the jump offset at `pos` so it targets `target`.
    /// Offset = target - (pos + 1)  (relative from instruction AFTER the jump).
    fn patch_jump(&mut self, pos: usize, target: usize) {
        let offset = target as isize - (pos as isize + 1);
        let instr = match &self.instrs[pos] {
            Instr::Jump(_) => Instr::Jump(offset as i16),
            Instr::JumpIfFalsy(_) => Instr::JumpIfFalsy(offset as i16),
            Instr::JumpIfTruthy(_) => Instr::JumpIfTruthy(offset as i16),
            Instr::JumpIfUndef(_) => Instr::JumpIfUndef(offset as i16),
            _ => panic!("patch_jump: not a jump instruction at {}", pos),
        };
        self.instrs[pos] = instr;
    }

    // ── Core compilation ─────────────────────────────────────────────────

    fn compile_expr(&mut self, expr: &CompiledExpr) {
        match expr {
            // ── Leaves ────────────────────────────────────────────────
            CompiledExpr::Literal(v) => {
                let idx = self.intern_const(v.clone());
                self.emit(Instr::PushConst(idx));
            }
            CompiledExpr::ExplicitNull => {
                self.emit(Instr::PushExplicitNull);
            }
            CompiledExpr::VariableLookup(name) => {
                if name.is_empty() {
                    // `$` = current context
                    self.emit(Instr::PushData);
                } else {
                    let idx = self.intern_str(name);
                    self.emit(Instr::GetVar(idx));
                }
            }
            CompiledExpr::ContextVar(name) => {
                let idx = self.intern_str(name);
                self.emit(Instr::GetVar(idx));
            }
            CompiledExpr::FieldLookup(field) => {
                // Peephole opportunity: `PushData; GetField` → `GetDataField`
                // We emit `PushData` then `GetField`; the peephole pass folds them.
                self.emit(Instr::PushData);
                let fidx = self.intern_str(field);
                self.emit(Instr::GetField(fidx));
            }
            CompiledExpr::NestedFieldLookup(outer, inner) => {
                self.emit(Instr::PushData);
                let oidx = self.intern_str(outer);
                self.emit(Instr::GetField(oidx));
                let iidx = self.intern_str(inner);
                self.emit(Instr::GetField(iidx));
            }
            CompiledExpr::FieldPath(steps) => {
                if steps.is_empty() {
                    self.emit(Instr::PushData);
                    return;
                }
                let all_simple = steps.iter().all(|s| s.filter.is_none());
                if all_simple {
                    // Inline the chain: PushData then GetField for each step.
                    // Peephole will fold the first PushData+GetField into GetDataField.
                    self.emit(Instr::PushData);
                    for step in steps {
                        let fidx = self.intern_str(&step.field);
                        self.emit(Instr::GetField(fidx));
                    }
                } else if steps.len() == 1 {
                    // Single step with filter: `field[predicate]`
                    // Emit: GetDataField("field") + FilterByBytecode(predicate_prog)
                    // The predicate is compiled to its own BytecodeProgram and run per
                    // element with a reusable stack — no per-element Vec allocation.
                    let step = &steps[0];
                    let fidx = self.intern_str(&step.field);
                    self.emit(Instr::GetDataField(fidx));
                    if let Some(filter) = &step.filter {
                        let sub_bc = BytecodeCompiler::compile(filter);
                        let sidx = self.sub_programs.len() as u16;
                        self.sub_programs.push(sub_bc);
                        self.emit(Instr::FilterByBytecode(sidx));
                    }
                } else {
                    // Multi-step filtered path: delegate to eval_compiled (handles
                    // intermediate array-mapping and nested filter application).
                    let idx = self.add_fallback(expr.clone());
                    self.emit(Instr::EvalFallback(idx));
                }
            }

            // ── Arithmetic ───────────────────────────────────────────
            CompiledExpr::Arithmetic { op, lhs, rhs } => {
                let lhs_en = matches!(**lhs, CompiledExpr::ExplicitNull);
                let rhs_en = matches!(**rhs, CompiledExpr::ExplicitNull);
                self.compile_expr(lhs);
                self.compile_expr(rhs);
                let instr = match op {
                    CompiledArithOp::Add => Instr::Add(lhs_en, rhs_en),
                    CompiledArithOp::Sub => Instr::Sub(lhs_en, rhs_en),
                    CompiledArithOp::Mul => Instr::Mul(lhs_en, rhs_en),
                    CompiledArithOp::Div => Instr::Div(lhs_en, rhs_en),
                    CompiledArithOp::Mod => Instr::Mod(lhs_en, rhs_en),
                };
                self.emit(instr);
            }

            // ── Comparison ───────────────────────────────────────────
            CompiledExpr::Compare { op, lhs, rhs } => {
                let lhs_en = matches!(**lhs, CompiledExpr::ExplicitNull);
                let rhs_en = matches!(**rhs, CompiledExpr::ExplicitNull);
                self.compile_expr(lhs);
                self.compile_expr(rhs);
                let instr = match op {
                    CompiledCmp::Eq => Instr::CmpEq,
                    CompiledCmp::Ne => Instr::CmpNe,
                    // Ordered comparisons need explicit-null flags for T2010 errors
                    CompiledCmp::Lt => Instr::CmpLt(lhs_en, rhs_en),
                    CompiledCmp::Le => Instr::CmpLe(lhs_en, rhs_en),
                    CompiledCmp::Gt => Instr::CmpGt(lhs_en, rhs_en),
                    CompiledCmp::Ge => Instr::CmpGe(lhs_en, rhs_en),
                };
                self.emit(instr);
            }

            // ── Logical (short-circuit) ──────────────────────────────
            // Both And/Or must short-circuit: rhs must not be evaluated when lhs
            // already determines the result. This is critical when rhs contains
            // builtins that error on undefined (e.g. $number(undefined)).
            // Result is always Bool, matching eval_compiled_inner semantics.
            CompiledExpr::And(lhs, rhs) => {
                // compile lhs; if falsy → false; else compile rhs; result = Bool(truthy(rhs))
                self.compile_expr(lhs);
                let jump_to_false = self.emit_jump_placeholder(Instr::JumpIfFalsy(0));
                self.compile_expr(rhs);
                let jump_to_false2 = self.emit_jump_placeholder(Instr::JumpIfFalsy(0));
                // both truthy → true
                let true_idx = self.intern_const(JValue::Bool(true));
                self.emit(Instr::PushConst(true_idx));
                let jump_to_end = self.emit_jump_placeholder(Instr::Jump(0));
                // false label
                let false_label = self.here();
                self.patch_jump(jump_to_false, false_label);
                self.patch_jump(jump_to_false2, false_label);
                let false_idx = self.intern_const(JValue::Bool(false));
                self.emit(Instr::PushConst(false_idx));
                let end_label = self.here();
                self.patch_jump(jump_to_end, end_label);
            }
            CompiledExpr::Or(lhs, rhs) => {
                // compile lhs; if truthy → true; else compile rhs; result = Bool(truthy(rhs))
                self.compile_expr(lhs);
                let jump_to_true = self.emit_jump_placeholder(Instr::JumpIfTruthy(0));
                self.compile_expr(rhs);
                let jump_to_true2 = self.emit_jump_placeholder(Instr::JumpIfTruthy(0));
                // both falsy → false
                let false_idx = self.intern_const(JValue::Bool(false));
                self.emit(Instr::PushConst(false_idx));
                let jump_to_end = self.emit_jump_placeholder(Instr::Jump(0));
                // true label
                let true_label = self.here();
                self.patch_jump(jump_to_true, true_label);
                self.patch_jump(jump_to_true2, true_label);
                let true_idx = self.intern_const(JValue::Bool(true));
                self.emit(Instr::PushConst(true_idx));
                let end_label = self.here();
                self.patch_jump(jump_to_end, end_label);
            }
            CompiledExpr::Not(inner) => {
                self.compile_expr(inner);
                self.emit(Instr::Not);
            }
            CompiledExpr::Negate(inner) => {
                self.compile_expr(inner);
                self.emit(Instr::Negate);
            }
            CompiledExpr::Concat(lhs, rhs) => {
                self.compile_expr(lhs);
                self.compile_expr(rhs);
                self.emit(Instr::Concat);
            }

            // ── Conditional ──────────────────────────────────────────
            CompiledExpr::Conditional {
                condition,
                then_expr,
                else_expr,
            } => {
                // emit condition
                self.compile_expr(condition);
                // JumpIfFalsy → else branch
                let jump_to_else = self.emit_jump_placeholder(Instr::JumpIfFalsy(0));
                // then branch
                self.compile_expr(then_expr);
                // Jump past else
                let jump_past_else = self.emit_jump_placeholder(Instr::Jump(0));
                // else label
                let else_label = self.here();
                self.patch_jump(jump_to_else, else_label);
                // else branch (or Undefined if None)
                if let Some(e) = else_expr {
                    self.compile_expr(e);
                } else {
                    self.emit(Instr::PushUndefined);
                }
                let end_label = self.here();
                self.patch_jump(jump_past_else, end_label);
            }

            // ── Coalesce ─────────────────────────────────────────────
            CompiledExpr::Coalesce(lhs, rhs) => {
                // Emit lhs
                self.compile_expr(lhs);
                // JumpIfUndef peeks: if lhs is Undefined, jump to rhs block
                let jump_to_rhs = self.emit_jump_placeholder(Instr::JumpIfUndef(0));
                // lhs is defined: jump past rhs
                let jump_to_end = self.emit_jump_placeholder(Instr::Jump(0));
                // rhs label: pop the Undefined (JumpIfUndef peeked, didn't pop)
                let rhs_label = self.here();
                self.patch_jump(jump_to_rhs, rhs_label);
                self.emit(Instr::Pop);
                self.compile_expr(rhs);
                let end_label = self.here();
                self.patch_jump(jump_to_end, end_label);
            }

            // ── Collections ──────────────────────────────────────────
            CompiledExpr::ArrayConstruct(elems) => {
                // Check whether any element requires nested-array semantics.
                // is_nested=true means the element must be kept as a nested array
                // (not flattened). The MakeArray VM instruction implements
                // is_nested=false semantics (flatten arrays one level).
                // For is_nested=true elements, fall back to eval_compiled.
                let all_flat = elems.iter().all(|(_, is_nested)| !is_nested);
                if all_flat {
                    for (elem, _) in elems {
                        self.compile_expr(elem);
                    }
                    self.emit(Instr::MakeArray(elems.len() as u16));
                } else {
                    // Mixed/nested: use eval_compiled fallback for correctness
                    let idx = self.add_fallback(expr.clone());
                    self.emit(Instr::EvalFallback(idx));
                }
            }
            CompiledExpr::ObjectConstruct(pairs) => {
                for (key, val_expr) in pairs {
                    let kidx = self.intern_const(JValue::string(key.clone()));
                    self.emit(Instr::PushConst(kidx));
                    self.compile_expr(val_expr);
                }
                self.emit(Instr::MakeObject(pairs.len() as u16));
            }

            // ── Block ────────────────────────────────────────────────
            CompiledExpr::Block(exprs) => {
                let n = exprs.len();
                if n == 0 {
                    self.emit(Instr::PushUndefined);
                    return;
                }
                for e in exprs {
                    self.compile_expr(e);
                }
                if n > 1 {
                    self.emit(Instr::BlockEnd(n as u16));
                }
            }

            // ── Builtin call ─────────────────────────────────────────
            CompiledExpr::BuiltinCall { name, args } => {
                for arg in args {
                    self.compile_expr(arg);
                }
                let nidx = self.intern_str(name);
                self.emit(Instr::CallBuiltin {
                    name_idx: nidx,
                    arg_count: args.len() as u8,
                });
            }

            // ── Higher-order functions ────────────────────────────────
            // MapCall / FilterCall / ReduceCall contain compiled lambda bodies that
            // are evaluated by eval_compiled_inner — the bytecode VM delegates to it
            // via EvalFallback. This keeps the VM instruction set simple while still
            // benefiting from compile-once lambda body compilation.
            CompiledExpr::MapCall { .. }
            | CompiledExpr::FilterCall { .. }
            | CompiledExpr::ReduceCall { .. } => {
                let idx = self.add_fallback(expr.clone());
                self.emit(Instr::EvalFallback(idx));
            }
        }
    }

    // ── Public interface ─────────────────────────────────────────────────

    /// Compile a `CompiledExpr` tree into a `BytecodeProgram`.
    /// Applies peephole optimization before returning.
    pub(crate) fn compile(expr: &CompiledExpr) -> BytecodeProgram {
        let mut c = BytecodeCompiler::new();
        c.compile_expr(expr);
        c.emit(Instr::Return);
        let prog = BytecodeProgram {
            shape_cache: BytecodeProgram::alloc_shape_cache(c.instrs.len()),
            instrs: c.instrs,
            const_pool: c.const_pool,
            string_pool: c.string_pool,
            fallback_exprs: c.fallback_exprs,
            sub_programs: c.sub_programs,
        };
        peephole(prog)
    }
}

// ---------------------------------------------------------------------------
// Peephole optimizations
// ---------------------------------------------------------------------------

/// Single-pass peephole optimizer on a `BytecodeProgram`.
///
/// Rules applied:
/// - `PushData; GetField(x)` → `GetDataField(x)`
/// - `GetVar(v); GetField(f)` → `GetVarField { v, f }`
/// - `Not; Not` → remove both (double negation)
/// - `JumpIfFalsy(0)` or `Jump(0)` → remove (no-op jumps)
pub(crate) fn peephole(prog: BytecodeProgram) -> BytecodeProgram {
    let BytecodeProgram {
        instrs,
        const_pool,
        string_pool,
        fallback_exprs,
        sub_programs,
        ..
    } = prog;

    let n = instrs.len();
    let mut out: Vec<Instr> = Vec::with_capacity(n);

    // First pass: build remap[old_ip] → new_ip for jump-target patching.
    // For each peephole rule that folds/removes instructions, both consumed
    // instructions must map to the same new_ip in the output.
    let mut remap = vec![0usize; n + 1];
    {
        let mut new_ip = 0usize;
        let mut i = 0usize;
        while i < n {
            match (&instrs[i], instrs.get(i + 1)) {
                // PushData + GetField → GetDataField (2 old instrs → 1 new)
                (Instr::PushData, Some(Instr::GetField(_))) => {
                    remap[i] = new_ip;
                    remap[i + 1] = new_ip; // GetField also maps to GetDataField's slot
                    new_ip += 1;
                    i += 2;
                }
                // GetVar + GetField → GetVarField (2 old instrs → 1 new)
                (Instr::GetVar(_), Some(Instr::GetField(_))) => {
                    remap[i] = new_ip;
                    remap[i + 1] = new_ip;
                    new_ip += 1;
                    i += 2;
                }
                // Not + Not → removed (both old instrs map to the next instruction)
                (Instr::Not, Some(Instr::Not)) => {
                    remap[i] = new_ip; // points to whatever comes after
                    remap[i + 1] = new_ip;
                    // new_ip NOT incremented
                    i += 2;
                }
                // No-op jumps (single instr removed)
                (Instr::JumpIfFalsy(0) | Instr::JumpIfTruthy(0) | Instr::Jump(0), _) => {
                    remap[i] = new_ip;
                    // new_ip NOT incremented
                    i += 1;
                }
                _ => {
                    remap[i] = new_ip;
                    new_ip += 1;
                    i += 1;
                }
            }
        }
        remap[n] = new_ip; // sentinel: one past the end
    }

    // Second pass: emit transformed instructions.
    let mut i = 0usize;
    while i < n {
        match (&instrs[i], instrs.get(i + 1)) {
            // PushData + GetField → GetDataField
            (Instr::PushData, Some(Instr::GetField(fidx))) => {
                out.push(Instr::GetDataField(*fidx));
                i += 2;
                continue;
            }
            // GetVar + GetField → GetVarField
            (Instr::GetVar(vidx), Some(Instr::GetField(fidx))) => {
                out.push(Instr::GetVarField {
                    var_idx: *vidx,
                    field_idx: *fidx,
                });
                i += 2;
                continue;
            }
            // Not + Not → skip both
            (Instr::Not, Some(Instr::Not)) => {
                i += 2;
                continue;
            }
            // No-op jumps
            (Instr::JumpIfFalsy(0) | Instr::JumpIfTruthy(0) | Instr::Jump(0), _) => {
                i += 1;
                continue;
            }
            _ => {}
        }

        // For jump instructions: remap the target using the remap table.
        let patched = match &instrs[i] {
            Instr::Jump(off) => {
                let old_target = (i as isize + 1 + *off as isize) as usize;
                let new_target = remap[old_target.min(n)];
                let new_src = out.len();
                Instr::Jump((new_target as isize - new_src as isize - 1) as i16)
            }
            Instr::JumpIfFalsy(off) => {
                let old_target = (i as isize + 1 + *off as isize) as usize;
                let new_target = remap[old_target.min(n)];
                let new_src = out.len();
                Instr::JumpIfFalsy((new_target as isize - new_src as isize - 1) as i16)
            }
            Instr::JumpIfTruthy(off) => {
                let old_target = (i as isize + 1 + *off as isize) as usize;
                let new_target = remap[old_target.min(n)];
                let new_src = out.len();
                Instr::JumpIfTruthy((new_target as isize - new_src as isize - 1) as i16)
            }
            Instr::JumpIfUndef(off) => {
                let old_target = (i as isize + 1 + *off as isize) as usize;
                let new_target = remap[old_target.min(n)];
                let new_src = out.len();
                Instr::JumpIfUndef((new_target as isize - new_src as isize - 1) as i16)
            }
            other => other.clone(),
        };
        out.push(patched);
        i += 1;
    }

    let shape_cache = BytecodeProgram::alloc_shape_cache(out.len());
    BytecodeProgram {
        instrs: out,
        const_pool,
        string_pool,
        shape_cache,
        fallback_exprs,
        sub_programs,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Structural equality for constant pool deduplication.
/// Covers only the literal types that appear in `CompiledExpr::Literal`.
fn values_eq(a: &JValue, b: &JValue) -> bool {
    match (a, b) {
        (JValue::Null, JValue::Null) => true,
        (JValue::Bool(x), JValue::Bool(y)) => x == y,
        (JValue::Number(x), JValue::Number(y)) => x == y,
        (JValue::String(x), JValue::String(y)) => x == y,
        _ => false,
    }
}
