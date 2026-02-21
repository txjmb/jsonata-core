//! Bytecode VM for JSONata compiled expressions.
//!
//! A `BytecodeProgram` is produced by `crate::compiler::BytecodeCompiler` from a
//! `CompiledExpr` tree.  Running a program involves the `Vm` struct which walks
//! the flat `instrs` vector sequentially, using a small operand stack.
//!
//! Why a flat bytecode vs the tree-based `CompiledExpr`?
//! - `Vec<Instr>` is one contiguous allocation; pointer-based trees cause cache misses
//!   every time we recurse into a `Box<CompiledExpr>`.
//! - An iterative `match instrs[ip]` loop is branch-predictor-friendly.
//! - Inline shape caches (per-`GetField` `Cell`) amortise IndexMap hash lookups to O(1)
//!   positional access on subsequent hits.

use std::cell::Cell;
use std::collections::HashMap;

use crate::evaluator::{eval_compiled, CompiledExpr, EvaluatorError};
use crate::value::JValue;

// ---------------------------------------------------------------------------
// Instruction set
// ---------------------------------------------------------------------------

/// A single bytecode instruction.
#[derive(Debug, Clone)]
pub(crate) enum Instr {
    // ── Constants ────────────────────────────────────────────────────────
    /// Push `const_pool[idx]` onto the stack.
    PushConst(u16),
    /// Push the explicit-null sentinel (triggers T2010/T2002 in comparisons).
    PushExplicitNull,
    /// Push `JValue::Undefined`.
    PushUndefined,

    // ── Data access ──────────────────────────────────────────────────────
    /// Push the current context value (`$`).
    PushData,
    /// Pop TOS, push the field named `string_pool[idx]` of that value.
    /// Uses the inline shape cache at `shape_cache[ip]`.
    GetField(u16),
    /// Like `GetField` but operates directly on the context (`$`), skipping a push.
    /// Peephole target for `PushData; GetField(x)`.
    GetDataField(u16),
    /// Push the variable `string_pool[idx]` from the `vars` map.
    GetVar(u16),
    /// Shorthand for `GetVar(v); GetField(f)` — common `$var.field` pattern.
    GetVarField { var_idx: u16, field_idx: u16 },

    // ── Arithmetic ───────────────────────────────────────────────────────
    /// `lhs_en` / `rhs_en` — whether the operand was a compile-time explicit `null`
    /// literal, used to generate T2002 errors matching the tree-walker.
    Add(bool, bool),
    Sub(bool, bool),
    Mul(bool, bool),
    Div(bool, bool),
    Mod(bool, bool),

    // ── Comparison ───────────────────────────────────────────────────────
    CmpEq,
    CmpNe,
    /// `lhs_en` / `rhs_en` — explicit-null flags for T2010 error generation.
    CmpLt(bool, bool),
    CmpLe(bool, bool),
    CmpGt(bool, bool),
    CmpGe(bool, bool),

    // ── Logical / string ─────────────────────────────────────────────────
    /// Binary And — kept for potential future use, but the compiler emits
    /// short-circuit jump sequences for And/Or rather than these instructions.
    #[allow(dead_code)]
    And,
    #[allow(dead_code)]
    Or,
    Not,
    Negate,
    Concat,

    // ── Control flow (relative signed offsets from instruction AFTER jump) ──
    /// Unconditional jump by `offset`.
    Jump(i16),
    /// Jump by `offset` if TOS is falsy (pops TOS).
    JumpIfFalsy(i16),
    /// Jump by `offset` if TOS is truthy (pops TOS).
    JumpIfTruthy(i16),
    /// Jump by `offset` if TOS is Undefined (does NOT pop TOS).
    JumpIfUndef(i16),
    /// Pop and discard TOS.
    Pop,

    // ── Construction ────────────────────────────────────────────────────
    /// Pop `n` values from the stack (last pushed = last element); build an array.
    /// Undefined elements are skipped; Array elements are flattened one level
    /// (implements is_nested=false semantics from JSONata array constructors).
    MakeArray(u16),
    /// Pop `n` (string key, value) pairs and push an object.
    MakeObject(u16),
    /// Pop `n` values and push the last one (sequential block evaluation).
    BlockEnd(u16),

    // ── Builtins ─────────────────────────────────────────────────────────
    /// Call the builtin named `string_pool[name_idx]` with `arg_count` stack args.
    /// Pops args (first pushed = first arg), pushes result.
    CallBuiltin { name_idx: u16, arg_count: u8 },

    // ── Array filtering ──────────────────────────────────────────────────
    /// Pop TOS (must be Array or Undefined), run `sub_programs[idx]` for each element
    /// with the element as `data`, keep elements where result is truthy.
    /// Pushes the filtered array (or Undefined if empty).
    /// The sub-program runs with a reusable stack (no per-element allocation).
    FilterByBytecode(u16),

    // ── Fallback ─────────────────────────────────────────────────────────
    /// Evaluate `fallback_exprs[idx]` via `eval_compiled` (tree-IR fallback).
    /// Used for `CompiledExpr` variants too complex to lower to flat bytecode.
    EvalFallback(u16),

    // ── Misc ─────────────────────────────────────────────────────────────
    /// Sentinel — terminates execution and returns TOS.
    Return,
}

// ---------------------------------------------------------------------------
// Program
// ---------------------------------------------------------------------------

/// A compiled, self-contained bytecode program.
///
/// Produced by `BytecodeCompiler::compile` and evaluated by `Vm::run`.
#[derive(Debug, Clone)]
pub(crate) struct BytecodeProgram {
    pub instrs: Vec<Instr>,
    /// Constant value pool (string literals, numbers, booleans, null).
    pub const_pool: Vec<JValue>,
    /// String interning pool (field names, variable names, builtin names).
    pub string_pool: Vec<String>,
    /// Per-`GetField`/`GetDataField`/`GetVarField` inline shape cache.
    /// One slot per instruction (indexed by `ip`); `None` = cold/polymorphic.
    /// `Cell` provides interior mutability without `Mutex` (GIL guarantees single-thread access).
    pub shape_cache: Vec<Cell<Option<usize>>>,
    /// Fallback `CompiledExpr` pool for `EvalFallback` instructions.
    /// Contains expressions too complex to lower to flat bytecode.
    pub fallback_exprs: Vec<CompiledExpr>,
    /// Sub-programs for `FilterByBytecode` instructions.
    /// Each sub-program is a compiled predicate to be run per array element.
    pub sub_programs: Vec<BytecodeProgram>,
}

impl BytecodeProgram {
    /// Allocate an uninitialized shape cache matching `instrs.len()`.
    pub(crate) fn alloc_shape_cache(n: usize) -> Vec<Cell<Option<usize>>> {
        (0..n).map(|_| Cell::new(None)).collect()
    }
}

// ---------------------------------------------------------------------------
// Virtual machine
// ---------------------------------------------------------------------------

/// Stack-based evaluator for a `BytecodeProgram`.
pub(crate) struct Vm<'prog> {
    prog: &'prog BytecodeProgram,
    /// Operand stack.
    stack: Vec<JValue>,
}

impl<'prog> Vm<'prog> {
    pub(crate) fn new(prog: &'prog BytecodeProgram) -> Self {
        Vm {
            prog,
            stack: Vec::with_capacity(16),
        }
    }

    /// Run the program against `data` with optional variable bindings.
    #[inline]
    pub(crate) fn run(
        &mut self,
        data: &JValue,
        vars: Option<&HashMap<&str, &JValue>>,
    ) -> Result<JValue, EvaluatorError> {
        run_inner(self.prog, data, vars, &mut self.stack)
    }
}

// ---------------------------------------------------------------------------
// Core interpreter loop (free function so FilterByBytecode can recurse into it)
// ---------------------------------------------------------------------------

fn run_inner(
    prog: &BytecodeProgram,
    data: &JValue,
    vars: Option<&HashMap<&str, &JValue>>,
    stack: &mut Vec<JValue>,
) -> Result<JValue, EvaluatorError> {
    use crate::evaluator::{
        call_pure_builtin_by_name, compiled_arithmetic, compiled_concat, compiled_equal,
        compiled_is_truthy, compiled_ordered_cmp, CompiledArithOp,
    };

    let instrs = &prog.instrs;
    let const_pool = &prog.const_pool;
    let string_pool = &prog.string_pool;
    let shape_cache = &prog.shape_cache;
    let fallback_exprs = &prog.fallback_exprs;
    let sub_programs = &prog.sub_programs;

    let mut ip: usize = 0;

    loop {
        // SAFETY: BytecodeCompiler guarantees well-formed programs
        // (each jump lands on a valid instruction, last instr is Return).
        match &instrs[ip] {
            Instr::PushConst(idx) => {
                stack.push(const_pool[*idx as usize].clone());
                ip += 1;
            }
            Instr::PushExplicitNull => {
                // Use Null — explicit-null semantics handled at compare/arithmetic sites
                stack.push(JValue::Null);
                ip += 1;
            }
            Instr::PushUndefined => {
                stack.push(JValue::Undefined);
                ip += 1;
            }
            Instr::PushData => {
                stack.push(data.clone());
                ip += 1;
            }
            Instr::GetField(idx) => {
                let val = stack.pop().unwrap_or(JValue::Undefined);
                let field = &string_pool[*idx as usize];
                stack.push(get_field_cached(&val, field, &shape_cache[ip]));
                ip += 1;
            }
            Instr::GetDataField(idx) => {
                let field = &string_pool[*idx as usize];
                stack.push(get_field_cached(data, field, &shape_cache[ip]));
                ip += 1;
            }
            Instr::GetVar(idx) => {
                let name = &string_pool[*idx as usize];
                let v = match vars {
                    Some(m) => m.get(name.as_str()).copied().cloned(),
                    None => None,
                }
                .unwrap_or(JValue::Undefined);
                stack.push(v);
                ip += 1;
            }
            Instr::GetVarField { var_idx, field_idx } => {
                let var_name = &string_pool[*var_idx as usize];
                let obj = match vars {
                    Some(m) => m.get(var_name.as_str()).copied().cloned(),
                    None => None,
                }
                .unwrap_or(JValue::Undefined);
                let field = &string_pool[*field_idx as usize];
                stack.push(get_field_cached(&obj, field, &shape_cache[ip]));
                ip += 1;
            }

            // ── Arithmetic ───────────────────────────────────────────
            Instr::Add(lhs_en, rhs_en) => {
                let rhs = stack.pop().unwrap_or(JValue::Undefined);
                let lhs = stack.pop().unwrap_or(JValue::Undefined);
                stack.push(compiled_arithmetic(CompiledArithOp::Add, &lhs, &rhs, *lhs_en, *rhs_en)?);
                ip += 1;
            }
            Instr::Sub(lhs_en, rhs_en) => {
                let rhs = stack.pop().unwrap_or(JValue::Undefined);
                let lhs = stack.pop().unwrap_or(JValue::Undefined);
                stack.push(compiled_arithmetic(CompiledArithOp::Sub, &lhs, &rhs, *lhs_en, *rhs_en)?);
                ip += 1;
            }
            Instr::Mul(lhs_en, rhs_en) => {
                let rhs = stack.pop().unwrap_or(JValue::Undefined);
                let lhs = stack.pop().unwrap_or(JValue::Undefined);
                stack.push(compiled_arithmetic(CompiledArithOp::Mul, &lhs, &rhs, *lhs_en, *rhs_en)?);
                ip += 1;
            }
            Instr::Div(lhs_en, rhs_en) => {
                let rhs = stack.pop().unwrap_or(JValue::Undefined);
                let lhs = stack.pop().unwrap_or(JValue::Undefined);
                stack.push(compiled_arithmetic(CompiledArithOp::Div, &lhs, &rhs, *lhs_en, *rhs_en)?);
                ip += 1;
            }
            Instr::Mod(lhs_en, rhs_en) => {
                let rhs = stack.pop().unwrap_or(JValue::Undefined);
                let lhs = stack.pop().unwrap_or(JValue::Undefined);
                stack.push(compiled_arithmetic(CompiledArithOp::Mod, &lhs, &rhs, *lhs_en, *rhs_en)?);
                ip += 1;
            }

            // ── Comparison ───────────────────────────────────────────
            Instr::CmpEq => {
                let rhs = stack.pop().unwrap_or(JValue::Undefined);
                let lhs = stack.pop().unwrap_or(JValue::Undefined);
                stack.push(compiled_equal(&lhs, &rhs));
                ip += 1;
            }
            Instr::CmpNe => {
                let rhs = stack.pop().unwrap_or(JValue::Undefined);
                let lhs = stack.pop().unwrap_or(JValue::Undefined);
                let eq = compiled_equal(&lhs, &rhs);
                let ne = match eq {
                    JValue::Bool(b) => JValue::Bool(!b),
                    other => other,
                };
                stack.push(ne);
                ip += 1;
            }
            Instr::CmpLt(lhs_en, rhs_en) => {
                let rhs = stack.pop().unwrap_or(JValue::Undefined);
                let lhs = stack.pop().unwrap_or(JValue::Undefined);
                stack.push(compiled_ordered_cmp(
                    &lhs, &rhs, *lhs_en, *rhs_en,
                    |a, b| a < b,
                    |a, b| a < b,
                )?);
                ip += 1;
            }
            Instr::CmpLe(lhs_en, rhs_en) => {
                let rhs = stack.pop().unwrap_or(JValue::Undefined);
                let lhs = stack.pop().unwrap_or(JValue::Undefined);
                stack.push(compiled_ordered_cmp(
                    &lhs, &rhs, *lhs_en, *rhs_en,
                    |a, b| a <= b,
                    |a, b| a <= b,
                )?);
                ip += 1;
            }
            Instr::CmpGt(lhs_en, rhs_en) => {
                let rhs = stack.pop().unwrap_or(JValue::Undefined);
                let lhs = stack.pop().unwrap_or(JValue::Undefined);
                stack.push(compiled_ordered_cmp(
                    &lhs, &rhs, *lhs_en, *rhs_en,
                    |a, b| a > b,
                    |a, b| a > b,
                )?);
                ip += 1;
            }
            Instr::CmpGe(lhs_en, rhs_en) => {
                let rhs = stack.pop().unwrap_or(JValue::Undefined);
                let lhs = stack.pop().unwrap_or(JValue::Undefined);
                stack.push(compiled_ordered_cmp(
                    &lhs, &rhs, *lhs_en, *rhs_en,
                    |a, b| a >= b,
                    |a, b| a >= b,
                )?);
                ip += 1;
            }

            // ── Logical / string ─────────────────────────────────────
            Instr::And => {
                // Mirrors eval_compiled_inner: if lhs is falsy → Bool(false),
                // else Bool(is_truthy(rhs)). Always returns Bool, never Undefined.
                let rhs = stack.pop().unwrap_or(JValue::Undefined);
                let lhs = stack.pop().unwrap_or(JValue::Undefined);
                if !compiled_is_truthy(&lhs) {
                    stack.push(JValue::Bool(false));
                } else {
                    stack.push(JValue::Bool(compiled_is_truthy(&rhs)));
                }
                ip += 1;
            }
            Instr::Or => {
                // Mirrors eval_compiled_inner: if lhs is truthy → Bool(true),
                // else Bool(is_truthy(rhs)). Always returns Bool, never Undefined.
                let rhs = stack.pop().unwrap_or(JValue::Undefined);
                let lhs = stack.pop().unwrap_or(JValue::Undefined);
                if compiled_is_truthy(&lhs) {
                    stack.push(JValue::Bool(true));
                } else {
                    stack.push(JValue::Bool(compiled_is_truthy(&rhs)));
                }
                ip += 1;
            }
            Instr::Not => {
                let val = stack.pop().unwrap_or(JValue::Undefined);
                stack.push(JValue::Bool(!compiled_is_truthy(&val)));
                ip += 1;
            }
            Instr::Negate => {
                let val = stack.pop().unwrap_or(JValue::Undefined);
                match val {
                    JValue::Number(n) => stack.push(JValue::Number(-n)),
                    JValue::Undefined => stack.push(JValue::Undefined),
                    _ => {
                        return Err(EvaluatorError::TypeError(
                            "D1002: Cannot negate non-number value".to_string(),
                        ))
                    }
                }
                ip += 1;
            }
            Instr::Concat => {
                let rhs = stack.pop().unwrap_or(JValue::Undefined);
                let lhs = stack.pop().unwrap_or(JValue::Undefined);
                stack.push(compiled_concat(lhs, rhs)?);
                ip += 1;
            }

            // ── Control flow ─────────────────────────────────────────
            Instr::Jump(offset) => {
                ip = (ip as isize + 1 + *offset as isize) as usize;
            }
            Instr::JumpIfFalsy(offset) => {
                let val = stack.pop().unwrap_or(JValue::Undefined);
                if !compiled_is_truthy(&val) {
                    ip = (ip as isize + 1 + *offset as isize) as usize;
                } else {
                    ip += 1;
                }
            }
            Instr::JumpIfTruthy(offset) => {
                let val = stack.pop().unwrap_or(JValue::Undefined);
                if compiled_is_truthy(&val) {
                    ip = (ip as isize + 1 + *offset as isize) as usize;
                } else {
                    ip += 1;
                }
            }
            Instr::JumpIfUndef(offset) => {
                // Peek (do not pop): jump if TOS is Undefined, keep TOS otherwise
                let is_undef = stack.last().map_or(true, |v| v.is_undefined());
                if is_undef {
                    ip = (ip as isize + 1 + *offset as isize) as usize;
                } else {
                    ip += 1;
                }
            }
            Instr::Pop => {
                stack.pop();
                ip += 1;
            }

            // ── Construction ─────────────────────────────────────────
            Instr::MakeArray(n) => {
                let n = *n as usize;
                let start = stack.len().saturating_sub(n);
                let raw: Vec<JValue> = stack.drain(start..).collect();
                // Implements JSONata array constructor semantics (is_nested=false):
                // skip undefined elements; flatten nested arrays one level.
                let mut elems: Vec<JValue> = Vec::with_capacity(raw.len());
                for v in raw {
                    match v {
                        JValue::Undefined => {}
                        JValue::Array(arr) => elems.extend(arr.iter().cloned()),
                        other => elems.push(other),
                    }
                }
                stack.push(JValue::array(elems));
                ip += 1;
            }
            Instr::MakeObject(n) => {
                let n = *n as usize;
                let start = stack.len().saturating_sub(n * 2);
                let pairs: Vec<JValue> = stack.drain(start..).collect();
                let mut obj = indexmap::IndexMap::new();
                for chunk in pairs.chunks(2) {
                    if let [JValue::String(k), val] = chunk {
                        if !val.is_undefined() {
                            obj.insert(k.to_string(), val.clone());
                        }
                    }
                }
                stack.push(JValue::Object(std::rc::Rc::new(obj)));
                ip += 1;
            }
            Instr::BlockEnd(n) => {
                // Keep only the last element of a block (sequential evaluation)
                let n = *n as usize;
                if n > 1 && stack.len() >= n {
                    let last = stack.pop().unwrap();
                    let start = stack.len().saturating_sub(n - 1);
                    stack.drain(start..);
                    stack.push(last);
                }
                ip += 1;
            }

            // ── Builtins ─────────────────────────────────────────────
            Instr::CallBuiltin { name_idx, arg_count } => {
                let name = &string_pool[*name_idx as usize];
                let n = *arg_count as usize;
                let start = stack.len().saturating_sub(n);
                let args: Vec<JValue> = stack.drain(start..).collect();
                stack.push(call_pure_builtin_by_name(name, &args, data)?);
                ip += 1;
            }

            // ── Array filtering ─────────────────────────────────────
            Instr::FilterByBytecode(idx) => {
                let sub_prog = &sub_programs[*idx as usize];
                let src = stack.pop().unwrap_or(JValue::Undefined);
                // Allocate one reusable sub-stack for all element evaluations.
                let mut sub_stack: Vec<JValue> = Vec::with_capacity(8);
                let result = match src {
                    JValue::Array(arr) => {
                        let mut kept: Vec<JValue> = Vec::with_capacity(arr.len());
                        for item in arr.iter() {
                            let test = run_inner(sub_prog, item, vars, &mut sub_stack)?;
                            if compiled_is_truthy(&test) {
                                kept.push(item.clone());
                            }
                        }
                        match kept.len() {
                            0 => JValue::Undefined,
                            1 => kept.pop().unwrap(),
                            _ => JValue::array(kept),
                        }
                    }
                    JValue::Undefined => JValue::Undefined,
                    other => {
                        // Single value: apply predicate directly
                        let test = run_inner(sub_prog, &other, vars, &mut sub_stack)?;
                        if compiled_is_truthy(&test) { other } else { JValue::Undefined }
                    }
                };
                stack.push(result);
                ip += 1;
            }

            // ── Fallback ─────────────────────────────────────────────
            Instr::EvalFallback(idx) => {
                let expr = &fallback_exprs[*idx as usize];
                stack.push(eval_compiled(expr, data, vars)?);
                ip += 1;
            }

            Instr::Return => {
                break;
            }
        }
    }

    Ok(stack.pop().unwrap_or(JValue::Undefined))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Field access with inline shape cache.
///
/// On a cold access, looks up `field` in `obj` normally.
/// On objects with IndexMap storage, caches the positional index so subsequent
/// accesses (for the same object schema) are O(1) positional lookups.
#[inline]
fn get_field_cached(val: &JValue, field: &str, cache: &Cell<Option<usize>>) -> JValue {
    match val {
        JValue::Object(obj) => {
            // Try cached positional index first
            if let Some(idx) = cache.get() {
                if let Some(v) = obj.get_index(idx) {
                    if v.0 == field {
                        return v.1.clone();
                    }
                    // Cache miss (schema changed) — fall through
                }
            }
            // Cold path: hash lookup
            if let Some(v) = obj.get(field) {
                // Prime cache with positional index
                if let Some(idx) = obj.get_index_of(field) {
                    cache.set(Some(idx));
                }
                v.clone()
            } else {
                JValue::Undefined
            }
        }
        JValue::Array(arr) => {
            // Array: map field access over elements (implicit array mapping).
            // Mirrors `compiled_field_step`: flatten nested arrays one level.
            let mut results: Vec<JValue> = Vec::with_capacity(arr.len());
            for item in arr.iter() {
                let v = get_field_cached(item, field, cache);
                match v {
                    JValue::Undefined => {}
                    JValue::Array(inner) => results.extend(inner.iter().cloned()),
                    other => results.push(other),
                }
            }
            match results.len() {
                0 => JValue::Undefined,
                1 => results.pop().unwrap(),
                _ => JValue::array(results),
            }
        }
        _ => JValue::Undefined,
    }
}
