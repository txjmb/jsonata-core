// JValue: Rc-wrapped value type for O(1) cloning
// Replaces serde_json::Value for internal use

use std::fmt;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

use indexmap::IndexMap;
use serde::de::{self, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::ser::{Serialize, SerializeMap, SerializeSeq, Serializer};

/// A JSON-like value with O(1) clone semantics via Rc-wrapping.
///
/// Standard JSON types (Array, Object, String) are wrapped in Rc for cheap cloning.
/// Internal types (Undefined, Lambda, Builtin, Regex) are first-class variants
/// instead of tagged objects with hash-map lookups.
#[derive(Clone, Debug)]
pub enum JValue {
    // Standard JSON types
    Null,
    Bool(bool),
    Number(f64),
    String(Rc<str>),
    Array(JArray),
    Object(Rc<JMap>),

    // Internal types (previously tagged objects)
    Undefined,
    /// A function value carrying its closure directly (issue #157). The `Rc`
    /// provides the closure's lifetime: a lambda escaping its defining scope
    /// stays alive for as long as any value references it, with no side table
    /// or escape analysis involved.
    Lambda(Rc<crate::evaluator::StoredLambda>),
    Builtin {
        name: Rc<str>,
    },
    Regex {
        pattern: Rc<str>,
        flags: Rc<str>,
    },

    /// Lazily-converted Python dict (see src/lazy.rs). Python-builds only.
    #[cfg(feature = "python")]
    LazyPyDict(Rc<crate::lazy::LazyPyDict>),
}

// ── JArray ───────────────────────────────────────────────────────────────────

/// The payload of `JValue::Array`: a shared, reference-counted list of items
/// plus the flags jsonata-js keeps on array values to decide singleton
/// unwrapping and flattening (issue #184).
///
/// - `sequence`: built by the evaluator (jsonata-js `createSequence()`), as
///   opposed to an array read from the input or built by `[...]`.
/// - `cons`: built by the `[...]` array constructor; never flattened.
/// - `keep_singleton`: produced under the `[]` keep-array modifier; a one-item
///   sequence is not unwrapped.
/// - `outer_wrapper`: the evaluation's input array as a context value.
///   jsonata-js wraps that array in a one-item `outerWrapper` sequence, so a
///   path over it treats it as a single item; `$`, `*` and an object
///   constructor see the bare array. This flag stands for that wrapper, and
///   `JValue::bare` removes it.
/// - `sorted_tuples`: tuple-stream wrappers that a sort of a tuple stream
///   returned. jsonata-js's sort builds a plain array of the tuples, which
///   drops the stream's `tupleStream` mark, so a filter, sort or group-by
///   right after it sees each tuple as an object rather than its `@` value
///   (#214). The next path step makes a marked stream again.
///
/// One more flag describes the items rather than the value: `flat`, known
/// to hold no array that a path step flattens (none but `cons` arrays), so a
/// `$` step can reuse the items without scanning them (#196). It is private
/// and set only by constructors that already know the answer (input
/// conversion, numeric ranges, a step that flattened nothing); any mutation
/// through `make_mut` clears it, and arrays built with new items start
/// without it. `false` only means "not known".
///
/// The flags sit beside the `Rc`, not inside it, so flagging an array that is
/// shared (an input array, a bound variable) is O(1) and never copies the
/// items. They belong to this value, not to the allocation: two `JValue`s may
/// share items and differ in flags. jsonata-js gets the same effect by
/// mutating the array object in place.
///
/// The flags are metadata only: equality, serialization, and conversion to
/// host values ignore them. `JArray` dereferences to `Vec<JValue>`.
#[derive(Clone, Default)]
pub struct JArray {
    items: Rc<JItems>,
    pub sequence: bool,
    pub cons: bool,
    pub keep_singleton: bool,
    pub outer_wrapper: bool,
    pub sorted_tuples: bool,
    /// No item is an array a step flattens (one without `cons`). Private, so
    /// it can only be set where that is known; see the type's docs.
    flat: bool,
}

impl JArray {
    /// A plain array with no flags set.
    #[inline]
    pub fn new(items: Vec<JValue>) -> Self {
        JArray {
            items: Rc::new(JItems(items)),
            sequence: false,
            cons: false,
            keep_singleton: false,
            outer_wrapper: false,
            sorted_tuples: false,
            flat: false,
        }
    }

    /// A plain array that the caller knows holds no array a path step
    /// flattens (see [`JArray::is_flat`]). Pass `flat = false` when that is
    /// not known; the flag is only ever a shortcut, so `false` is always
    /// correct. Checked in debug builds.
    #[inline]
    pub fn new_flat(items: Vec<JValue>, flat: bool) -> Self {
        debug_assert!(
            !flat || !items.iter().any(flattens_in_step),
            "array marked flat holds an array"
        );
        JArray {
            flat,
            ..JArray::new(items)
        }
    }

    /// True when the items are known to hold no array that a path step
    /// flattens (none but `cons` arrays), so a `$` step can take them as
    /// they are. `false` means not known, not that there is one.
    #[inline]
    pub fn is_flat(&self) -> bool {
        self.flat
    }

    /// The shared items themselves, for code that follows reference counts.
    #[inline]
    pub(crate) fn items_rc(&self) -> &Rc<JItems> {
        &self.items
    }

    /// A mutable reference to the items, cloning them first if they are
    /// shared (`Rc::make_mut`). Clears `flat`, since the caller may store an
    /// array in them.
    #[inline]
    pub fn make_mut(&mut self) -> &mut Vec<JValue> {
        self.flat = false;
        &mut Rc::make_mut(&mut self.items).0
    }

    /// Consume the array and return its items, dropping the flags. Clones the
    /// items only if they are shared.
    #[inline]
    pub fn into_vec(self) -> Vec<JValue> {
        match Rc::try_unwrap(self.items) {
            // Leaves `JItems` empty, so its drop has nothing to walk.
            Ok(mut items) => std::mem::take(&mut items.0),
            Err(rc) => rc.0.clone(),
        }
    }

    /// A new array holding `items`, carrying this array's flags other than
    /// `outer_wrapper`, which only the input array has. Use it to
    /// rebuild an array from its items; `into_vec` and `JValue::array` drop
    /// the flags.
    #[inline]
    pub fn with_items(&self, items: Vec<JValue>) -> JArray {
        JArray {
            items: Rc::new(JItems(items)),
            outer_wrapper: false,
            // The new items are not known to be flat.
            flat: false,
            ..*self
        }
    }

    /// A new sequence of the same items, sharing them: what jsonata-js
    /// builds when a step collects an array's items with `createSequence()`.
    /// O(1); of this array's other flags only `sorted_tuples` carries over,
    /// with `flat`, which describes the shared items.
    #[inline]
    pub fn fresh_sequence(&self) -> JArray {
        JArray {
            items: Rc::clone(&self.items),
            sequence: true,
            cons: false,
            keep_singleton: false,
            outer_wrapper: false,
            // Tuples stay unmarked when a step collects them (#214).
            sorted_tuples: self.sorted_tuples,
            flat: self.flat,
        }
    }

    /// True when both arrays share the same items allocation.
    #[inline]
    pub fn ptr_eq(a: &JArray, b: &JArray) -> bool {
        Rc::ptr_eq(&a.items, &b.items)
    }
}

/// True for an item a path step flattens into its own items: an array not
/// built by the `[...]` constructor.
#[inline]
pub fn flattens_in_step(value: &JValue) -> bool {
    matches!(value, JValue::Array(arr) if !arr.cons)
}

impl Deref for JArray {
    type Target = Vec<JValue>;

    #[inline]
    fn deref(&self) -> &Vec<JValue> {
        &self.items.0
    }
}

/// Prints the items only, as `Rc<Vec<JValue>>` did, so that error messages
/// formatting a value with `{:?}` don't change with the flags.
impl fmt::Debug for JArray {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.items.0, f)
    }
}

impl PartialEq for JArray {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.items == other.items
    }
}

impl From<Vec<JValue>> for JArray {
    #[inline]
    fn from(items: Vec<JValue>) -> Self {
        JArray::new(items)
    }
}

impl From<JArray> for Vec<JValue> {
    #[inline]
    fn from(arr: JArray) -> Self {
        arr.into_vec()
    }
}

impl<'a> IntoIterator for &'a JArray {
    type Item = &'a JValue;
    type IntoIter = std::slice::Iter<'a, JValue>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.items.0.iter()
    }
}

impl IntoIterator for JArray {
    type Item = JValue;
    type IntoIter = std::vec::IntoIter<JValue>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.into_vec().into_iter()
    }
}

// ── Container payloads ───────────────────────────────────────────────────────

/// The items of a [`JArray`], behind its `Rc`: a `Vec<JValue>` whose drop
/// does not recurse (#233). Dereferences to the `Vec`.
#[derive(Clone, Default, PartialEq)]
pub struct JItems(Vec<JValue>);

impl Deref for JItems {
    type Target = Vec<JValue>;

    #[inline]
    fn deref(&self) -> &Vec<JValue> {
        &self.0
    }
}

impl Drop for JItems {
    #[inline]
    fn drop(&mut self) {
        let items = &self.0;
        if !frees_only_scalars(items.len(), || items.iter().any(frees_container)) {
            self.drop_items();
        }
    }
}

impl JItems {
    /// The rest of `drop`, kept out of line so the common case above stays
    /// small enough to inline.
    #[inline(never)]
    fn drop_items(&mut self) {
        if must_drop_flat(|| self.0.iter().any(frees_nested)) {
            drop_deep(self.0.drain(..));
        }
    }
}

impl<'a> IntoIterator for &'a JItems {
    type Item = &'a JValue;
    type IntoIter = std::slice::Iter<'a, JValue>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

/// The payload of `JValue::Object`, behind its `Rc`: an
/// `IndexMap<String, JValue>` whose drop does not recurse (#233).
/// Dereferences to the map, so it reads and updates like one; build it with
/// [`JValue::object`] or `JMap::from(map)`.
///
/// Its keys are in JavaScript's own-key order, as `Object.keys` lists them:
/// the array-index keys first in ascending number, then every other key in
/// insertion order. Building one orders it, and [`JMap::insert`] keeps it so.
#[derive(Clone, Default, PartialEq)]
pub struct JMap(IndexMap<String, JValue>);

/// The value of `key` when JavaScript treats it as an array index: `0`, or
/// digits with no leading zero up to 4294967294.
fn array_index(key: &str) -> Option<u32> {
    let bytes = key.as_bytes();
    match bytes.first()? {
        b'0' => (bytes.len() == 1).then_some(0),
        b'1'..=b'9' if bytes.len() <= 10 && bytes.iter().all(u8::is_ascii_digit) => key
            .parse::<u64>()
            .ok()
            .filter(|&n| n <= 4_294_967_294)
            .map(|n| n as u32),
        _ => None,
    }
}

/// Puts `map`'s keys in JavaScript's own-key order. The sort is stable, so
/// the other keys keep their insertion order, and it is skipped when no key
/// starts with a digit.
fn order_keys(map: &mut IndexMap<String, JValue>) {
    if !map
        .keys()
        .any(|key| key.as_bytes().first().is_some_and(u8::is_ascii_digit))
    {
        return;
    }
    map.sort_by(|a, _, b, _| match (array_index(a), array_index(b)) {
        (Some(a), Some(b)) => a.cmp(&b),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
}

impl JMap {
    /// Sets `key` to `value` and keeps JavaScript's key order: an existing
    /// key keeps its place, a new array-index key goes to its place among the
    /// index keys, and any other new key goes last. Returns the old value.
    pub fn insert(&mut self, key: String, value: JValue) -> Option<JValue> {
        if let Some(slot) = self.0.get_mut(&key) {
            return Some(std::mem::replace(slot, value));
        }
        match array_index(&key) {
            Some(index) => {
                let at = self
                    .0
                    .keys()
                    .position(|other| array_index(other).map_or(true, |other| other > index))
                    .unwrap_or(self.0.len());
                self.0.shift_insert(at, key, value);
            }
            None => {
                self.0.insert(key, value);
            }
        }
        None
    }

    /// The map itself. Never clones.
    #[inline]
    pub fn into_inner(mut self) -> IndexMap<String, JValue> {
        // Leaves an empty map behind, so this `JMap`'s drop has nothing to
        // walk; reusing the hasher avoids seeding a new one.
        let empty = IndexMap::with_hasher(self.0.hasher().clone());
        std::mem::replace(&mut self.0, empty)
    }
}

impl Deref for JMap {
    type Target = IndexMap<String, JValue>;

    #[inline]
    fn deref(&self) -> &IndexMap<String, JValue> {
        &self.0
    }
}

impl DerefMut for JMap {
    #[inline]
    fn deref_mut(&mut self) -> &mut IndexMap<String, JValue> {
        &mut self.0
    }
}

impl From<IndexMap<String, JValue>> for JMap {
    #[inline]
    fn from(mut map: IndexMap<String, JValue>) -> Self {
        order_keys(&mut map);
        JMap(map)
    }
}

impl From<JMap> for IndexMap<String, JValue> {
    #[inline]
    fn from(map: JMap) -> Self {
        map.into_inner()
    }
}

impl FromIterator<(String, JValue)> for JMap {
    #[inline]
    fn from_iter<I: IntoIterator<Item = (String, JValue)>>(iter: I) -> Self {
        JMap::from(IndexMap::from_iter(iter))
    }
}

impl<'a> IntoIterator for &'a JMap {
    type Item = (&'a String, &'a JValue);
    type IntoIter = indexmap::map::Iter<'a, String, JValue>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'a> IntoIterator for &'a mut JMap {
    type Item = (&'a String, &'a mut JValue);
    type IntoIter = indexmap::map::IterMut<'a, String, JValue>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter_mut()
    }
}

/// Prints the map only, as `Rc<IndexMap>` did.
impl fmt::Debug for JMap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl Drop for JMap {
    #[inline]
    fn drop(&mut self) {
        let map = &self.0;
        if !frees_only_scalars(map.len(), || map.values().any(frees_container)) {
            self.drop_items();
        }
    }
}

impl JMap {
    /// The rest of `drop`, kept out of line so the common case above stays
    /// small enough to inline.
    #[inline(never)]
    fn drop_items(&mut self) {
        if must_drop_flat(|| self.0.values().any(frees_nested)) {
            drop_deep(self.0.drain(..).map(|(_, value)| value));
        }
    }
}

// ── Dropping deep values (#233) ──────────────────────────────────────────────
//
// Left to the compiler, dropping a value drops its items, which drop theirs,
// one stack frame per level: a value nested ~10,000 deep (easy to build with
// `$reduce`) overflows a 1MB stack while being freed. Instead, the drop of a
// container's payload (`JItems`, `JMap`) first checks the stack. With room to
// spare it lets the ordinary recursive drop run, which costs one
// stack-pointer compare per payload and nothing per item. Once the stack runs
// low, it moves its items onto a heap work-list and takes each one apart in a
// loop, so the stack stays flat however deep the rest of the value goes.
// Reading the stack limit is a thread-local access, which in a shared library
// costs a call, so a small payload first looks at its items and skips the
// check when it frees no container (the common leaf object).
//
// Where the stack limit is unknown, the payload instead scans its items and
// uses the work-list when freeing them would recurse more than two levels.
//
// Only payloads this drop actually frees are taken apart: a container whose
// `Rc` is shared elsewhere is just released (its count drops by one).
//
// A lazy Python dict wrapper (`LazyPyDict`) counts as a container too: it
// holds the converted values of the fields read from it, so reading down a
// deep Python input builds a chain of wrappers with no payload between them.

/// Stack headroom below which a payload's drop switches to the work-list.
/// Covers the frames of a few levels of drop glue and the allocator's `free`.
const DROP_RED_ZONE: usize = 32 * 1024;

/// Payloads with at most this many items look at them before checking the
/// stack; see [`frees_only_scalars`].
const SMALL_PAYLOAD: usize = 8;

/// Whether a payload of `len` items can be freed without looking at the
/// stack: it is empty, or small and frees no container (`frees_container`
/// says whether any item would). Larger payloads skip the scan, as one stack
/// check costs less than a look at each item.
#[inline]
fn frees_only_scalars(len: usize, frees_container: impl FnOnce() -> bool) -> bool {
    len == 0 || (len <= SMALL_PAYLOAD && !frees_container())
}

/// Whether a payload must free its items with [`drop_deep`]: when the stack
/// is running low, or, where its limit is unknown, when `nested` (freeing the
/// items would recurse more than two levels) says so.
#[inline]
fn must_drop_flat(nested: impl FnOnce() -> bool) -> bool {
    match stacker::remaining_stack() {
        Some(left) => left < DROP_RED_ZONE,
        None => nested(),
    }
}

/// True when dropping `value` frees a non-empty array or object payload, or
/// a lazy Python dict wrapper that may hold one (or another wrapper).
#[inline]
fn frees_container(value: &JValue) -> bool {
    match value {
        JValue::Array(arr) => Rc::strong_count(&arr.items) == 1 && !arr.items.0.is_empty(),
        JValue::Object(map) => frees_map(map),
        #[cfg(feature = "python")]
        JValue::LazyPyDict(lazy) => Rc::strong_count(lazy) == 1 && lazy.holds_containers(),
        _ => false,
    }
}

/// `frees_container` for an object payload.
#[inline]
fn frees_map(map: &Rc<JMap>) -> bool {
    Rc::strong_count(map) == 1 && !map.0.is_empty()
}

/// True when dropping `value` frees a payload that itself holds a payload to
/// free: dropping it would recurse at least two levels.
#[inline]
fn frees_nested(value: &JValue) -> bool {
    match value {
        JValue::Array(arr) => {
            Rc::strong_count(&arr.items) == 1 && arr.items.0.iter().any(frees_container)
        }
        JValue::Object(map) => frees_nested_map(map),
        #[cfg(feature = "python")]
        JValue::LazyPyDict(lazy) => {
            Rc::strong_count(lazy) == 1 && lazy.any_held(frees_container, frees_map)
        }
        _ => false,
    }
}

/// `frees_nested` for an object payload.
#[inline]
fn frees_nested_map(map: &Rc<JMap>) -> bool {
    Rc::strong_count(map) == 1 && map.0.values().any(frees_container)
}

/// The drop of a lazy Python dict wrapper holding containers (see
/// `LazyPyDict`'s `Drop`): its converted values are freed as a payload's
/// items are.
#[cfg(feature = "python")]
#[cold]
#[inline(never)]
pub(crate) fn drop_lazy_dict(lazy: &mut crate::lazy::LazyPyDict) {
    if must_drop_flat(|| lazy.any_held(frees_nested, frees_nested_map)) {
        drop_deep(lazy.take_held());
    }
}

/// Drop `values` without recursing deeper than two levels at a time: each
/// value whose drop would go deeper is taken apart on a heap work-list. The
/// values left to drop inline free at most scalars and shared containers
/// below their own payload, so whatever their payloads' drops decide, the
/// recursion ends there.
#[cold]
#[inline(never)]
fn drop_deep(values: impl Iterator<Item = JValue>) {
    let mut pending: Vec<JValue> = values.filter(frees_nested).collect();
    while let Some(mut value) = pending.pop() {
        match &mut value {
            JValue::Array(arr) => {
                if let Some(items) = Rc::get_mut(&mut arr.items) {
                    pending.extend(items.0.drain(..).filter(frees_nested));
                }
            }
            JValue::Object(map) => {
                if let Some(map) = Rc::get_mut(map) {
                    pending.extend(map.0.drain(..).map(|(_, value)| value).filter(frees_nested));
                }
            }
            #[cfg(feature = "python")]
            JValue::LazyPyDict(lazy) => {
                if let Some(lazy) = Rc::get_mut(lazy) {
                    pending.extend(lazy.take_held().filter(frees_nested));
                }
            }
            _ => {}
        }
        // `value`'s payload is empty now (or, held by a `Weak`, not freed
        // here), so dropping it does not recurse.
    }
}

// ── Type checks ──────────────────────────────────────────────────────────────

impl JValue {
    #[inline]
    pub fn is_null(&self) -> bool {
        matches!(self, JValue::Null)
    }

    #[inline]
    pub fn is_undefined(&self) -> bool {
        matches!(self, JValue::Undefined)
    }

    #[inline]
    pub fn is_bool(&self) -> bool {
        matches!(self, JValue::Bool(_))
    }

    #[inline]
    pub fn is_number(&self) -> bool {
        matches!(self, JValue::Number(_))
    }

    #[inline]
    pub fn is_string(&self) -> bool {
        matches!(self, JValue::String(_))
    }

    #[inline]
    pub fn is_array(&self) -> bool {
        matches!(self, JValue::Array(_))
    }

    #[inline]
    pub fn is_object(&self) -> bool {
        match self {
            JValue::Object(_) => true,
            #[cfg(feature = "python")]
            JValue::LazyPyDict(_) => true,
            _ => false,
        }
    }

    /// True only for the lazy Python-dict wrapper variant (always false when
    /// the `python` feature is off). Used to guard `normalize_lazy` calls so
    /// hot paths pay nothing when no lazy value is involved.
    #[inline]
    pub fn is_lazy(&self) -> bool {
        #[cfg(feature = "python")]
        {
            matches!(self, JValue::LazyPyDict(_))
        }
        #[cfg(not(feature = "python"))]
        {
            false
        }
    }

    #[inline]
    pub fn is_lambda(&self) -> bool {
        matches!(self, JValue::Lambda { .. })
    }

    #[inline]
    pub fn is_builtin(&self) -> bool {
        matches!(self, JValue::Builtin { .. })
    }

    #[inline]
    pub fn is_function(&self) -> bool {
        // A regex is a function too: called with a string, it gives the
        // first match, as jsonata-js `evaluateRegex`'s closure does (#205).
        matches!(
            self,
            JValue::Lambda { .. } | JValue::Builtin { .. } | JValue::Regex { .. }
        )
    }

    #[inline]
    pub fn is_regex(&self) -> bool {
        matches!(self, JValue::Regex { .. })
    }
}

// ── Extraction ───────────────────────────────────────────────────────────────

impl JValue {
    #[inline]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            JValue::Number(n) => Some(*n),
            _ => None,
        }
    }

    #[inline]
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            JValue::Number(n) => {
                let f = *n;
                if f.fract() == 0.0 && f >= i64::MIN as f64 && f <= i64::MAX as f64 {
                    Some(f as i64)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    #[inline]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            JValue::String(s) => Some(s),
            _ => None,
        }
    }

    #[inline]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            JValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    #[inline]
    pub fn as_array(&self) -> Option<&Vec<JValue>> {
        match self {
            JValue::Array(arr) => Some(arr),
            _ => None,
        }
    }

    /// The array payload, flags included, if this is an array.
    #[inline]
    pub fn as_jarray(&self) -> Option<&JArray> {
        match self {
            JValue::Array(arr) => Some(arr),
            _ => None,
        }
    }

    #[inline]
    pub fn as_object(&self) -> Option<&IndexMap<String, JValue>> {
        match self {
            JValue::Object(map) => Some(map),
            #[cfg(feature = "python")]
            JValue::LazyPyDict(lazy) => lazy.to_object_ref(),
            _ => None,
        }
    }

    /// Get a mutable reference to the inner Vec, cloning if shared (Rc::make_mut).
    #[inline]
    pub fn as_array_mut(&mut self) -> Option<&mut Vec<JValue>> {
        match self {
            JValue::Array(arr) => Some(arr.make_mut()),
            _ => None,
        }
    }

    /// jsonata-js's generic unwrap, applied to the result of every
    /// expression: an empty sequence is undefined, and a one-item sequence is
    /// its item unless `keep_singleton` is set. Arrays that are not sequences
    /// are left alone.
    #[inline]
    pub fn unwrap_sequence(self) -> JValue {
        match self {
            JValue::Array(arr) if arr.sequence => match arr.len() {
                0 => JValue::Undefined,
                1 if !arr.keep_singleton => arr[0].clone(),
                _ => JValue::Array(arr),
            },
            other => other,
        }
    }

    /// This value as the context for a top-level evaluation: an input array
    /// that is not a sequence is marked `outer_wrapper`, as jsonata-js wraps
    /// it. O(1).
    #[inline]
    pub fn into_outer_wrapper(self) -> JValue {
        match self {
            JValue::Array(mut arr) if !arr.sequence => {
                arr.outer_wrapper = true;
                JValue::Array(arr)
            }
            other => other,
        }
    }

    /// This value with any `outer_wrapper` removed: what jsonata-js's
    /// wrapper holds. O(1).
    #[inline]
    pub fn bare(&self) -> JValue {
        match self {
            JValue::Array(arr) if arr.outer_wrapper => JValue::Array(JArray {
                outer_wrapper: false,
                ..arr.clone()
            }),
            other => other.clone(),
        }
    }

    /// Set `sequence` on an array. A no-op on non-arrays. O(1): the items are
    /// never copied, even when shared.
    #[inline]
    pub fn mark_sequence(&mut self) {
        if let JValue::Array(arr) = self {
            arr.sequence = true;
        }
    }

    /// Set `keep_singleton` on an array (the `[]` keep-array modifier). A
    /// no-op on non-arrays. O(1), like `mark_sequence`.
    #[inline]
    pub fn mark_keep_singleton(&mut self) {
        if let JValue::Array(arr) = self {
            arr.keep_singleton = true;
        }
    }

    /// Get a mutable reference to the inner IndexMap, cloning if shared (Rc::make_mut).
    /// An insert through it appends a new key in insertion order; use
    /// [`JMap::insert`] to keep JavaScript's key order.
    #[inline]
    pub fn as_object_mut(&mut self) -> Option<&mut IndexMap<String, JValue>> {
        match self {
            JValue::Object(map) => Some(&mut Rc::make_mut(map).0),
            _ => None,
        }
    }

    /// Get a reference to the `Rc<str>` for the string variant.
    #[inline]
    pub fn as_rc_str(&self) -> Option<&Rc<str>> {
        match self {
            JValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// Index into an object by key.
    #[inline]
    pub fn get(&self, key: &str) -> Option<&JValue> {
        match self {
            JValue::Object(map) => map.get(key),
            _ => None,
        }
    }

    /// Index into an array by position.
    #[inline]
    pub fn get_index(&self, index: usize) -> Option<&JValue> {
        match self {
            JValue::Array(arr) => arr.get(index),
            _ => None,
        }
    }
}

// ── Constructors ─────────────────────────────────────────────────────────────

impl JValue {
    #[inline]
    pub fn from_i64(n: i64) -> Self {
        JValue::Number(n as f64)
    }

    #[inline]
    pub fn from_f64(n: f64) -> Self {
        JValue::Number(n)
    }

    #[inline]
    pub fn string(s: impl Into<Rc<str>>) -> Self {
        JValue::String(s.into())
    }

    #[inline]
    pub fn array(v: Vec<JValue>) -> Self {
        JValue::Array(JArray::new(v))
    }

    /// An array built by the evaluator (jsonata-js `createSequence()`).
    #[inline]
    pub fn sequence(v: Vec<JValue>) -> Self {
        JValue::Array(JArray {
            sequence: true,
            ..JArray::new(v)
        })
    }

    /// `JValue::sequence` for items the caller knows hold no array a path
    /// step flattens; see [`JArray::new_flat`].
    #[inline]
    pub fn sequence_flat(v: Vec<JValue>, flat: bool) -> Self {
        JValue::Array(JArray {
            sequence: true,
            ..JArray::new_flat(v, flat)
        })
    }

    /// An array built by the `[...]` array constructor.
    #[inline]
    pub fn cons(v: Vec<JValue>) -> Self {
        JValue::Array(JArray {
            cons: true,
            ..JArray::new(v)
        })
    }

    /// An object from `m`, its keys put in JavaScript's own-key order.
    #[inline]
    pub fn object(m: IndexMap<String, JValue>) -> Self {
        JValue::Object(Rc::new(JMap::from(m)))
    }

    #[inline]
    pub fn builtin(name: impl Into<Rc<str>>) -> Self {
        JValue::Builtin { name: name.into() }
    }

    #[inline]
    pub fn regex(pattern: impl Into<Rc<str>>, flags: impl Into<Rc<str>>) -> Self {
        JValue::Regex {
            pattern: pattern.into(),
            flags: flags.into(),
        }
    }
}

// ── From impls ───────────────────────────────────────────────────────────────

impl From<bool> for JValue {
    #[inline]
    fn from(b: bool) -> Self {
        JValue::Bool(b)
    }
}

impl From<i64> for JValue {
    #[inline]
    fn from(n: i64) -> Self {
        JValue::Number(n as f64)
    }
}

impl From<i32> for JValue {
    #[inline]
    fn from(n: i32) -> Self {
        JValue::Number(n as f64)
    }
}

impl From<u64> for JValue {
    #[inline]
    fn from(n: u64) -> Self {
        JValue::Number(n as f64)
    }
}

impl From<usize> for JValue {
    #[inline]
    fn from(n: usize) -> Self {
        JValue::Number(n as f64)
    }
}

impl From<f64> for JValue {
    #[inline]
    fn from(n: f64) -> Self {
        JValue::Number(n)
    }
}

impl From<&str> for JValue {
    #[inline]
    fn from(s: &str) -> Self {
        JValue::String(s.into())
    }
}

impl From<String> for JValue {
    #[inline]
    fn from(s: String) -> Self {
        JValue::String(s.into())
    }
}

impl From<Rc<str>> for JValue {
    #[inline]
    fn from(s: Rc<str>) -> Self {
        JValue::String(s)
    }
}

impl From<Vec<JValue>> for JValue {
    #[inline]
    fn from(v: Vec<JValue>) -> Self {
        JValue::array(v)
    }
}

impl From<IndexMap<String, JValue>> for JValue {
    #[inline]
    fn from(m: IndexMap<String, JValue>) -> Self {
        JValue::object(m)
    }
}

// ── Deep-value recursion ─────────────────────────────────────────────────────

/// Stack headroom below which [`with_stack`] moves to a new segment. Covers
/// the frames of one level of a recursive value walker plus the leaf work
/// below it (a serde writer, a Python object allocation).
const VALUE_RED_ZONE: usize = 64 * 1024;

/// Size of each heap-allocated stack segment [`with_stack`] switches to.
const VALUE_STACK_SEGMENT: usize = 1024 * 1024;

/// Run one level of a recursive walk over a value tree (conversion,
/// serialization, equality, ...), moving to a heap-allocated stack segment
/// when the current stack is nearly used up. A value's nesting depth has no
/// fixed bound (`$reduce` or a recursive function can nest objects thousands
/// deep), so without this a deep value would overflow a small thread stack
/// (Windows' 1MB main thread) and crash the process. Call it at each
/// container level only; on the shallow path it is one stack-pointer compare.
#[inline]
pub fn with_stack<R>(f: impl FnOnce() -> R) -> R {
    stacker::maybe_grow(VALUE_RED_ZONE, VALUE_STACK_SEGMENT, f)
}

// ── PartialEq ────────────────────────────────────────────────────────────────

impl PartialEq for JValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (JValue::Null, JValue::Null) => true,
            (JValue::Undefined, JValue::Undefined) => true,
            (JValue::Bool(a), JValue::Bool(b)) => a == b,
            (JValue::Number(a), JValue::Number(b)) => {
                // Handle NaN: NaN != NaN
                if a.is_nan() && b.is_nan() {
                    return false;
                }
                a == b
            }
            (JValue::String(a), JValue::String(b)) => a == b,
            (JValue::Array(a), JValue::Array(b)) => with_stack(|| a == b),
            (JValue::Object(a), JValue::Object(b)) => with_stack(|| a == b),
            // Function identity, like JS: two lambda values are equal only if
            // they are the same closure instance.
            (JValue::Lambda(a), JValue::Lambda(b)) => Rc::ptr_eq(a, b),
            (JValue::Builtin { name: a }, JValue::Builtin { name: b }) => a == b,
            (
                JValue::Regex {
                    pattern: ap,
                    flags: af,
                },
                JValue::Regex {
                    pattern: bp,
                    flags: bf,
                },
            ) => ap == bp && af == bf,
            // `PartialEq` cannot return `Result`, so a conversion failure on either side
            // here is swallowed and yields `false` (not-equal) rather than raising. This
            // is by design for this trait impl -- it must stay infallible for internal
            // consumers (e.g. `targets.iter().any(|t| t == value)` in transform). Sites
            // that need conversion errors to actually surface as a Python `TypeError`
            // (the `=`/`!=` operators, the `in` operator) are responsible for calling
            // `normalize_lazy` themselves *before* reaching here -- see the equality
            // operator handling and `in_operator` in `evaluator.rs`.
            #[cfg(feature = "python")]
            (JValue::LazyPyDict(a), JValue::LazyPyDict(b)) => {
                Rc::ptr_eq(a, b)
                    || a.same_object(b)
                    || matches!((a.to_object_ref(), b.to_object_ref()), (Some(x), Some(y)) if x == y)
            }
            #[cfg(feature = "python")]
            (JValue::LazyPyDict(a), JValue::Object(b)) => {
                a.to_object_ref().is_some_and(|x| x == &***b)
            }
            #[cfg(feature = "python")]
            (JValue::Object(a), JValue::LazyPyDict(b)) => {
                b.to_object_ref().is_some_and(|x| &***a == x)
            }
            _ => false,
        }
    }
}

// ── Display ──────────────────────────────────────────────────────────────────

impl fmt::Display for JValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JValue::Null => write!(f, "null"),
            JValue::Undefined => write!(f, "undefined"),
            JValue::Bool(b) => write!(f, "{}", b),
            JValue::Number(n) => format_number(*n, f),
            JValue::String(s) => write!(f, "\"{}\"", escape_json_string(s)),
            JValue::Array(arr) => {
                write!(f, "[")?;
                for (i, v) in arr.iter().enumerate() {
                    if i > 0 {
                        write!(f, ",")?;
                    }
                    write!(f, "{}", v)?;
                }
                write!(f, "]")
            }
            JValue::Object(map) => {
                write!(f, "{{")?;
                for (i, (k, v)) in map.iter().enumerate() {
                    if i > 0 {
                        write!(f, ",")?;
                    }
                    write!(f, "\"{}\":{}", escape_json_string(k), v)?;
                }
                write!(f, "}}")
            }
            JValue::Lambda(_) => write!(f, "\"<lambda>\""),
            JValue::Builtin { name } => write!(f, "\"<builtin:{}>\"", name),
            JValue::Regex { pattern, flags } => write!(f, "\"<regex:/{}/{}>\"", pattern, flags),
            #[cfg(feature = "python")]
            JValue::LazyPyDict(lazy) => match lazy.to_object() {
                Ok(map) => write!(f, "{}", JValue::Object(map)),
                Err(_) => write!(f, "{{}}"),
            },
        }
    }
}

fn escape_json_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c < '\x20' => {
                result.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => result.push(c),
        }
    }
    result
}

/// Print a finite f64 the way JavaScript's `String(number)` /
/// `JSON.stringify(number)` does: shortest round-trip decimal, plain digits
/// for magnitudes in [1e-6, 1e21), exponential (with an explicit `+` on
/// positive exponents) outside that range, and `-0` printed as `0` (matching
/// `JSON.stringify(-0)`).
///
/// This is the one definition of JS number printing; `$string` layers its
/// non-integer `toPrecision(15)` rounding on top of it (see
/// `functions::string`). Callers must handle non-finite values first.
pub(crate) fn js_number_to_string(f: f64) -> String {
    debug_assert!(f.is_finite());
    if f == 0.0 {
        return "0".to_string();
    }
    let abs = f.abs();
    if (1e-6..1e21).contains(&abs) {
        // Rust's Display is shortest-round-trip and never exponential,
        // which matches JS exactly in this range.
        format!("{}", f)
    } else {
        let exp_str = format!("{:e}", f);
        if exp_str.contains('e') && !exp_str.contains("e-") && !exp_str.contains("e+") {
            exp_str.replace('e', "e+")
        } else {
            exp_str
        }
    }
}

fn format_number(n: f64, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    if !n.is_finite() {
        // NaN and +/-Infinity serialize as null (matching JSON spec)
        write!(f, "null")
    } else {
        write!(f, "{}", js_number_to_string(n))
    }
}

// ── Serialization (for JSON output via evaluate_json) ────────────────────────

impl Serialize for JValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            JValue::Null => serializer.serialize_none(),
            JValue::Undefined => serializer.serialize_none(),
            JValue::Bool(b) => serializer.serialize_bool(*b),
            JValue::Number(n) => {
                if n.is_nan() || n.is_infinite() {
                    serializer.serialize_none()
                } else if n.fract() == 0.0 && *n >= i64::MIN as f64 && *n <= i64::MAX as f64 {
                    serializer.serialize_i64(*n as i64)
                } else {
                    serializer.serialize_f64(*n)
                }
            }
            JValue::String(s) => serializer.serialize_str(s),
            JValue::Array(arr) => with_stack(|| {
                let mut seq = serializer.serialize_seq(Some(arr.len()))?;
                for v in arr.iter() {
                    seq.serialize_element(v)?;
                }
                seq.end()
            }),
            JValue::Object(map) => with_stack(|| {
                let mut m = serializer.serialize_map(Some(map.len()))?;
                for (k, v) in map.iter() {
                    m.serialize_entry(k, v)?;
                }
                m.end()
            }),
            // Functions, regexes included, serialize as "" (the JSON.stringify
            // replacer jsonata-js's $string uses).
            JValue::Lambda { .. } | JValue::Builtin { .. } | JValue::Regex { .. } => {
                serializer.serialize_str("")
            }
            #[cfg(feature = "python")]
            JValue::LazyPyDict(lazy) => match lazy.to_object() {
                Ok(map) => {
                    let mut m = serializer.serialize_map(Some(map.len()))?;
                    for (k, v) in map.iter() {
                        m.serialize_entry(k, v)?;
                    }
                    m.end()
                }
                Err(e) => Err(serde::ser::Error::custom(e.0)),
            },
        }
    }
}

// ── Deserialization (single-pass JSON→JValue) ────────────────────────────────

impl<'de> serde::Deserialize<'de> for JValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(JValueVisitor)
    }
}

struct JValueVisitor;

impl<'de> Visitor<'de> for JValueVisitor {
    type Value = JValue;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "any valid JSON value")
    }

    fn visit_bool<E: de::Error>(self, v: bool) -> Result<JValue, E> {
        Ok(JValue::Bool(v))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<JValue, E> {
        Ok(JValue::Number(v as f64))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<JValue, E> {
        Ok(JValue::Number(v as f64))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<JValue, E> {
        Ok(JValue::Number(v))
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<JValue, E> {
        Ok(JValue::string(v))
    }

    fn visit_string<E: de::Error>(self, v: String) -> Result<JValue, E> {
        Ok(JValue::String(v.into()))
    }

    fn visit_none<E: de::Error>(self) -> Result<JValue, E> {
        Ok(JValue::Null)
    }

    fn visit_unit<E: de::Error>(self) -> Result<JValue, E> {
        Ok(JValue::Null)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<JValue, A::Error> {
        let mut vec = Vec::with_capacity(seq.size_hint().unwrap_or(0));
        // Input arrays are never `cons`, so an input array is flat when it
        // holds no array at all (#196).
        let mut flat = true;
        while let Some(elem) = seq.next_element()? {
            flat &= !matches!(elem, JValue::Array(_));
            vec.push(elem);
        }
        Ok(JValue::Array(JArray::new_flat(vec, flat)))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<JValue, A::Error> {
        let mut m = IndexMap::with_capacity(map.size_hint().unwrap_or(0));
        while let Some((k, v)) = map.next_entry()? {
            m.insert(k, v);
        }
        Ok(JValue::object(m))
    }
}

// ── JSON string I/O ──────────────────────────────────────────────────────────

impl JValue {
    /// Serialize to a JSON string.
    pub fn to_json_string(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Serialize to a pretty-printed JSON string.
    pub fn to_json_string_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Parse a JSON string into a JValue (single-pass, no intermediate serde_json::Value).
    ///
    /// When the `simd` feature is enabled, uses simd-json for faster parsing on CPUs
    /// with SIMD support (SSE4.2/AVX2/NEON). Falls back to serde_json otherwise.
    ///
    /// `simd_json::serde::from_slice` allocates fresh internal scratch buffers
    /// (`Buffers::new`) on every call, sized proportionally to the input - measured
    /// as the dominant cost, large enough to make the "simd" path consistently
    /// SLOWER than plain serde_json (worse as input grows: -26% at 189 bytes,
    /// -29% at 180KB). Reusing those buffers across calls via
    /// `from_slice_with_buffers` (thread-local, since JValue's Rc-based design is
    /// already single-threaded per instance) eliminates that reallocation and
    /// brings simd-json back to consistently faster than serde_json (+5% to +22%
    /// across the same size range, measured with benches/examples/simd_json_bench.rs).
    pub fn from_json_str(s: &str) -> Result<JValue, serde_json::Error> {
        #[cfg(feature = "simd")]
        {
            thread_local! {
                static SCRATCH: std::cell::RefCell<(Vec<u8>, simd_json::Buffers)> =
                    std::cell::RefCell::new((Vec::new(), simd_json::Buffers::new(0)));
            }

            let result = SCRATCH.with(|scratch| {
                let (bytes, buffers) = &mut *scratch.borrow_mut();
                bytes.clear();
                bytes.extend_from_slice(s.as_bytes());
                simd_json::serde::from_slice_with_buffers::<JValue>(bytes, buffers)
            });
            if let Ok(value) = result {
                return Ok(value);
            }
            // Fall back to serde_json if simd-json fails (e.g., unsupported CPU)
        }
        serde_json::from_str(s)
    }
}

// ── Conversion from serde_json::Value ────────────────────────────────────────

impl From<serde_json::Value> for JValue {
    fn from(v: serde_json::Value) -> Self {
        match v {
            serde_json::Value::Null => JValue::Null,
            serde_json::Value::Bool(b) => JValue::Bool(b),
            serde_json::Value::Number(n) => JValue::Number(n.as_f64().unwrap_or(0.0)),
            serde_json::Value::String(s) => JValue::String(s.into()),
            serde_json::Value::Array(arr) => {
                let mut flat = true;
                let items = arr
                    .into_iter()
                    .map(|v| {
                        flat &= !v.is_array();
                        JValue::from(v)
                    })
                    .collect();
                JValue::Array(JArray::new_flat(items, flat))
            }
            serde_json::Value::Object(map) => {
                let m: IndexMap<String, JValue> =
                    map.into_iter().map(|(k, v)| (k, JValue::from(v))).collect();
                JValue::object(m)
            }
        }
    }
}

// ── Conversion to serde_json::Value (for Python boundary) ────────────────────

impl From<&JValue> for serde_json::Value {
    fn from(v: &JValue) -> Self {
        match v {
            JValue::Null | JValue::Undefined => serde_json::Value::Null,
            JValue::Bool(b) => serde_json::Value::Bool(*b),
            JValue::Number(n) => {
                if n.is_nan() || n.is_infinite() {
                    serde_json::Value::Null
                } else {
                    serde_json::json!(*n)
                }
            }
            JValue::String(s) => serde_json::Value::String(s.to_string()),
            JValue::Array(arr) => {
                serde_json::Value::Array(arr.iter().map(serde_json::Value::from).collect())
            }
            JValue::Object(map) => {
                let m: serde_json::Map<String, serde_json::Value> = map
                    .iter()
                    .map(|(k, v)| (k.clone(), serde_json::Value::from(v)))
                    .collect();
                serde_json::Value::Object(m)
            }
            JValue::Lambda { .. } | JValue::Builtin { .. } | JValue::Regex { .. } => {
                serde_json::Value::Null
            }
            #[cfg(feature = "python")]
            JValue::LazyPyDict(lazy) => match lazy.to_object() {
                Ok(map) => {
                    let m: serde_json::Map<String, serde_json::Value> = map
                        .iter()
                        .map(|(k, v)| (k.clone(), serde_json::Value::from(v)))
                        .collect();
                    serde_json::Value::Object(m)
                }
                Err(_) => serde_json::Value::Object(serde_json::Map::new()),
            },
        }
    }
}

// ── jvalue! macro ────────────────────────────────────────────────────────────

/// Macro for constructing JValue literals, similar to serde_json::json!
///
/// Usage:
///   jvalue!(null)           → JValue::Null
///   jvalue!(true)           → JValue::Bool(true)
///   jvalue!(false)          → JValue::Bool(false)
///   jvalue!(42)             → JValue::Number(42.0)
///   jvalue!(3.14)           → JValue::Number(3.14)
///   jvalue!("hello")        → JValue::String(Rc::from("hello"))
///   jvalue!([1, 2, 3])      → JValue::array(vec![...])
///   jvalue!({"k": v, ...})  → JValue::Object(Rc::new(IndexMap from pairs))
///   jvalue!(expr)           → JValue::from(expr)
#[macro_export]
macro_rules! jvalue {
    // null
    (null) => {
        $crate::value::JValue::Null
    };

    // true
    (true) => {
        $crate::value::JValue::Bool(true)
    };

    // false
    (false) => {
        $crate::value::JValue::Bool(false)
    };

    // Array
    ([ $($elem:tt),* $(,)? ]) => {
        $crate::value::JValue::array(vec![ $( jvalue!($elem) ),* ])
    };

    // Object
    ({ $($key:tt : $val:tt),* $(,)? }) => {
        {
            let mut map = indexmap::IndexMap::new();
            $(
                map.insert(($key).to_string(), jvalue!($val));
            )*
            $crate::value::JValue::object(map)
        }
    };

    // Expression (fallback — numbers, variables, function calls, etc.)
    ($other:expr) => {
        $crate::value::JValue::from($other)
    };
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clone_is_cheap() {
        // Array clone should be O(1) — same Rc pointer
        let arr = JValue::array(vec![
            JValue::from(1i64),
            JValue::from(2i64),
            JValue::from(3i64),
        ]);
        let arr2 = arr.clone();
        if let (JValue::Array(a), JValue::Array(b)) = (&arr, &arr2) {
            assert!(JArray::ptr_eq(a, b));
        } else {
            panic!("expected arrays");
        }

        // Object clone should be O(1)
        let mut map = IndexMap::new();
        map.insert("x".to_string(), JValue::from(1i64));
        let obj = JValue::object(map);
        let obj2 = obj.clone();
        if let (JValue::Object(a), JValue::Object(b)) = (&obj, &obj2) {
            assert!(Rc::ptr_eq(a, b));
        } else {
            panic!("expected objects");
        }

        // String clone should be O(1)
        let s = JValue::string("hello");
        let s2 = s.clone();
        if let (JValue::String(a), JValue::String(b)) = (&s, &s2) {
            assert!(Rc::ptr_eq(a, b));
        } else {
            panic!("expected strings");
        }
    }

    #[test]
    fn test_type_checks() {
        assert!(JValue::Null.is_null());
        assert!(JValue::Undefined.is_undefined());
        assert!(JValue::Bool(true).is_bool());
        assert!(JValue::Number(42.0).is_number());
        assert!(JValue::string("hello").is_string());
        assert!(JValue::array(vec![]).is_array());
        assert!(JValue::object(IndexMap::new()).is_object());
        let lambda = JValue::Lambda(Rc::new(crate::evaluator::StoredLambda::test_stub()));
        assert!(lambda.is_lambda());
        assert!(lambda.is_function());
        assert!(JValue::builtin("sum").is_builtin());
        assert!(JValue::builtin("sum").is_function());
        assert!(JValue::regex(".*", "i").is_regex());
    }

    #[test]
    fn test_extraction() {
        assert_eq!(JValue::Number(42.0).as_f64(), Some(42.0));
        assert_eq!(JValue::Number(42.0).as_i64(), Some(42));
        assert_eq!(JValue::Number(42.5).as_i64(), None);
        assert_eq!(JValue::string("hello").as_str(), Some("hello"));
        assert_eq!(JValue::Bool(true).as_bool(), Some(true));
        assert_eq!(
            JValue::array(vec![JValue::from(1i64)])
                .as_array()
                .map(|a| a.len()),
            Some(1)
        );
    }

    #[test]
    fn test_jvalue_macro() {
        let n = jvalue!(null);
        assert!(n.is_null());

        let b = jvalue!(true);
        assert_eq!(b.as_bool(), Some(true));

        let arr = jvalue!([1i64, 2i64, 3i64]);
        assert_eq!(arr.as_array().map(|a| a.len()), Some(3));

        let obj = jvalue!({"name": "Alice", "age": 30i64});
        assert_eq!(obj.get("name").and_then(|v| v.as_str()), Some("Alice"));
    }

    #[test]
    fn test_equality() {
        assert_eq!(JValue::Null, JValue::Null);
        assert_eq!(JValue::Bool(true), JValue::Bool(true));
        assert_ne!(JValue::Bool(true), JValue::Bool(false));
        assert_eq!(JValue::Number(42.0), JValue::Number(42.0));
        assert_ne!(JValue::Number(f64::NAN), JValue::Number(f64::NAN));
        assert_eq!(JValue::string("hello"), JValue::string("hello"));
        assert_ne!(JValue::Null, JValue::Undefined);
    }

    #[test]
    fn test_serde_roundtrip() {
        let v = jvalue!({"name": "Alice", "scores": [1i64, 2i64, 3i64], "active": true});
        let json_str = v.to_json_string().unwrap();
        let parsed = JValue::from_json_str(&json_str).unwrap();
        assert_eq!(v, parsed);
    }

    #[test]
    fn test_from_serde_json() {
        let sv = serde_json::json!({"name": "Alice", "age": 30, "scores": [1, 2, 3]});
        let jv = JValue::from(sv);
        assert_eq!(jv.get("name").and_then(|v| v.as_str()), Some("Alice"));
        assert_eq!(jv.get("age").and_then(|v| v.as_f64()), Some(30.0));
    }

    #[test]
    fn test_make_mut() {
        let mut arr = JValue::array(vec![JValue::from(1i64), JValue::from(2i64)]);
        let arr2 = arr.clone();

        // Mutate arr — should CoW (clone-on-write)
        arr.as_array_mut().unwrap().push(JValue::from(3i64));

        // arr2 should be unchanged
        assert_eq!(arr.as_array().unwrap().len(), 3);
        assert_eq!(arr2.as_array().unwrap().len(), 2);
    }

    fn flags(v: &JValue) -> (bool, bool, bool) {
        match v {
            JValue::Array(a) => (a.sequence, a.cons, a.keep_singleton),
            _ => panic!("not an array"),
        }
    }

    #[test]
    fn test_array_constructors_set_flags() {
        let items = || vec![JValue::from(1i64)];
        assert_eq!(flags(&JValue::array(items())), (false, false, false));
        assert_eq!(flags(&JValue::sequence(items())), (true, false, false));
        assert_eq!(flags(&JValue::cons(items())), (false, true, false));
        assert_eq!(flags(&JValue::from(items())), (false, false, false));
        assert_eq!(flags(&jvalue!([1, 2])), (false, false, false));
    }

    #[test]
    fn test_array_flags_ignored_by_eq_and_serialization() {
        let items = || vec![JValue::from(1i64), JValue::from("a")];
        let plain = JValue::array(items());
        let seq = JValue::sequence(items());
        let cons = JValue::cons(items());
        assert_eq!(plain, seq);
        assert_eq!(plain, cons);
        let json = serde_json::to_string(&plain).unwrap();
        assert_eq!(serde_json::to_string(&seq).unwrap(), json);
        assert_eq!(serde_json::to_string(&cons).unwrap(), json);
        assert_eq!(
            serde_json::Value::from(&seq),
            serde_json::Value::from(&plain)
        );
    }

    #[test]
    fn test_array_make_mut_keeps_flags() {
        let mut seq = JValue::sequence(vec![JValue::from(1i64)]);
        let shared = seq.clone();
        seq.as_array_mut().unwrap().push(JValue::from(2i64));
        assert_eq!(flags(&seq), (true, false, false));
        assert_eq!(shared.as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_flagging_a_shared_array_does_not_copy() {
        let input = JValue::array(vec![JValue::from(1i64), JValue::from(2i64)]);
        let mut flagged = input.clone();
        if let JValue::Array(a) = &mut flagged {
            a.sequence = true;
        }
        match (&input, &flagged) {
            (JValue::Array(a), JValue::Array(b)) => {
                assert!(JArray::ptr_eq(a, b), "items should still be shared");
                assert!(!a.sequence && b.sequence, "flags belong to each value");
            }
            _ => panic!("expected arrays"),
        }
    }

    #[test]
    fn test_array_debug_hides_flags() {
        let seq = JValue::sequence(vec![JValue::from(1i64)]);
        assert_eq!(format!("{seq:?}"), "Array([Number(1.0)])");
    }

    #[test]
    fn test_with_items_keeps_flags() {
        let seq = JValue::sequence(vec![JValue::from(1i64)]);
        let JValue::Array(arr) = &seq else {
            unreachable!()
        };
        let rebuilt = arr.with_items(vec![JValue::from(2i64)]);
        assert!(rebuilt.sequence && !rebuilt.cons && !rebuilt.keep_singleton);
        assert_eq!(*rebuilt, vec![JValue::from(2i64)]);
    }

    // ── `flat` (#196) ────────────────────────────────────────────────────

    fn is_flat(v: &JValue) -> bool {
        v.as_jarray().expect("an array").is_flat()
    }

    #[test]
    fn test_flat_set_by_json_input_without_nested_arrays() {
        let v = JValue::from_json_str(r#"{"a": [1, "x", null, {"b": [2]}], "e": []}"#).unwrap();
        assert!(is_flat(v.get("a").unwrap()));
        assert!(is_flat(v.get("e").unwrap()));
        // An array under an object item doesn't count; it is its own array.
        assert!(is_flat(
            v.get("a").unwrap().get_index(3).unwrap().get("b").unwrap()
        ));
    }

    #[test]
    fn test_flat_not_set_by_json_input_with_nested_arrays() {
        let v = JValue::from_json_str("[[1, 2], 3]").unwrap();
        assert!(!is_flat(&v));
        // The inner array holds numbers only.
        assert!(is_flat(v.get_index(0).unwrap()));
        let deep = JValue::from_json_str("[1, [[2]]]").unwrap();
        assert!(!is_flat(&deep));
        assert!(!is_flat(deep.get_index(1).unwrap()));
        assert!(is_flat(deep.get_index(1).unwrap().get_index(0).unwrap()));
    }

    #[test]
    fn test_flat_from_serde_json_value() {
        assert!(is_flat(&JValue::from(serde_json::json!([1, {"a": [1]}]))));
        assert!(!is_flat(&JValue::from(serde_json::json!([1, [2]]))));
    }

    #[test]
    fn test_flat_not_set_by_plain_constructors() {
        let items = || vec![JValue::from(1i64)];
        assert!(!is_flat(&JValue::array(items())));
        assert!(!is_flat(&JValue::sequence(items())));
        assert!(!is_flat(&JValue::cons(items())));
        assert!(!is_flat(&jvalue!([1, 2])));
        assert!(!JArray::default().is_flat());
    }

    #[test]
    fn test_flat_cleared_by_make_mut() {
        let mut v = JValue::from_json_str("[1, 2]").unwrap();
        let shared = v.clone();
        v.as_array_mut()
            .unwrap()
            .push(JValue::array(vec![JValue::from(3i64)]));
        assert!(!is_flat(&v), "a mutated array must lose the flag");
        assert!(is_flat(&shared), "the shared original is unchanged");

        // Unshared, so `make_mut` doesn't copy: the flag still goes.
        let JValue::Array(mut arr) = JValue::from_json_str("[1]").unwrap() else {
            unreachable!()
        };
        arr.make_mut();
        assert!(!arr.is_flat());
    }

    #[test]
    fn test_flat_cleared_by_with_items_and_kept_by_sharing() {
        let v = JValue::from_json_str("[1, 2]").unwrap();
        let arr = v.as_jarray().unwrap();
        assert!(!arr.with_items(vec![JValue::array(vec![])]).is_flat());
        assert!(!arr.with_items(vec![JValue::from(1i64)]).is_flat());
        // Same items: the flag describes them, so it carries over.
        assert!(arr.fresh_sequence().is_flat());
        assert!(arr.clone().is_flat());
        assert!(is_flat(&v.clone().into_outer_wrapper().bare()));
        let mut marked = v.clone();
        marked.mark_sequence();
        marked.mark_keep_singleton();
        assert!(is_flat(&marked));
    }

    #[test]
    fn test_flat_allows_cons_items() {
        // A step keeps a constructed array whole, so it doesn't stop `flat`.
        let arr = JArray::new_flat(vec![JValue::cons(vec![JValue::from(1i64)])], true);
        assert!(arr.is_flat());
        assert!(is_flat(&JValue::sequence_flat(
            vec![JValue::from(1i64)],
            true
        )));
        assert!(!is_flat(&JValue::sequence_flat(
            vec![JValue::from(1i64)],
            false
        )));
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "array marked flat holds an array")]
    fn test_flat_checked_in_debug_builds() {
        JArray::new_flat(vec![JValue::array(vec![])], true);
    }
    // --- #233: dropping deep values -----------------------------------------

    fn nest(depth: usize) -> JValue {
        let mut value = JValue::Null;
        for i in 0..depth {
            value = if i % 2 == 0 {
                JValue::array(vec![JValue::from(1i64), value])
            } else {
                let mut map = IndexMap::new();
                map.insert("a".to_string(), value);
                JValue::object(map)
            };
        }
        value
    }

    #[test]
    fn test_jmap_collects_and_iterates_like_a_map() {
        let pairs = || {
            [("a", 1i64), ("b", 2)]
                .into_iter()
                .map(|(k, v)| (k.to_string(), JValue::from(v)))
        };
        // `JValue::object` still takes an `IndexMap`, so `collect` infers it.
        let value = JValue::object(pairs().collect());
        assert_eq!(value, JValue::Object(Rc::new(pairs().collect::<JMap>())));
        let JValue::Object(map) = &value else {
            unreachable!()
        };
        let keys: Vec<&String> = (&**map).into_iter().map(|(k, _)| k).collect();
        assert_eq!(keys, ["a", "b"]);
        let mut owned = (**map).clone();
        for (_, v) in &mut owned {
            *v = JValue::Null;
        }
        assert_eq!(owned.into_inner()["a"], JValue::Null);
    }

    #[test]
    fn test_frees_nested_counts_only_payloads_this_drop_frees() {
        assert!(!frees_nested(&nest(1)));
        assert!(frees_nested(&nest(2)));
        assert!(frees_nested(&nest(3)));
        // The inner container is shared, so dropping the outer one only
        // releases it.
        let inner = nest(1);
        let outer = JValue::array(vec![inner.clone()]);
        assert!(!frees_nested(&outer));
        drop(inner);
        assert!(frees_nested(&outer));
        assert!(!frees_nested(&JValue::array(vec![JValue::array(vec![])])));
    }

    #[test]
    fn test_drop_deep_frees_any_depth_on_a_small_stack() {
        // The work-list alone, as used where the stack limit is unknown.
        std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(|| {
                drop_deep(std::iter::once(nest(100_000)));
                let value = nest(100_000);
                let keep = value.clone();
                drop_deep(std::iter::once(value));
                // Shared, so only released: still whole.
                let mut depth = 0;
                let mut node = &keep;
                loop {
                    node = match node {
                        JValue::Array(arr) => &arr[1],
                        JValue::Object(map) => &map["a"],
                        _ => break,
                    };
                    depth += 1;
                }
                assert_eq!(depth, 100_000);
                drop(keep);
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
