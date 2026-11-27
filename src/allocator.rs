//! The global allocator for builds with the `mimalloc` feature.
//!
//! `mimalloc::MiMalloc` sends every allocation through `mi_malloc_aligned`,
//! even when the alignment asked for is one `mi_malloc` already guarantees.
//! In mimalloc's aligned path, a block size that is not a power of two (a
//! 96-byte `Rc` box, say) takes the fast path only while its page's free list
//! has blocks; once it runs dry, the generic aligned path over-allocates from
//! a larger size class instead of refilling it, so every later allocation of
//! that size takes the slow path too (about 110 more instructions each). Which
//! sizes fall into this depends on what else shares their size class, so a
//! struct growing by one field could slow evaluation by several percent.
//!
//! [`SmallAlignMiMalloc`] calls `mi_malloc` directly for the alignments it
//! guarantees, which is nearly every Rust allocation, and hands the rest to
//! `MiMalloc`.
//!
//! The crates are built with their `v2` feature (Cargo.toml), which bundles
//! mimalloc 2.x instead of 3.3.x. mimalloc v3 keeps its thread heap in fixed
//! macOS thread-specific-data slots shared by every v3 copy in the process,
//! so it corrupted pyarrow 24's bundled mimalloc (and was corrupted by it),
//! and it crashes once the first thread that allocated has exited
//! (microsoft/mimalloc#1287). See GitHub issue #298. v2 keeps the heap in a
//! plain thread-local. Moving back to v3 needs a release that uses a
//! thread-local on macOS (mimalloc 3.4.1 can be configured to) and a
//! passing `tests/python/test_allocator_coexistence.py`.

use std::alloc::{GlobalAlloc, Layout};

use mimalloc::MiMalloc;

/// The alignment `mi_malloc` guarantees for a block of at least this many
/// bytes: `MI_MAX_ALIGN_SIZE` (16) on 64-bit targets. Smaller blocks are
/// word-aligned, and their size is a multiple of the word size, so a request
/// whose size is a multiple of its alignment is aligned as well. On 32-bit
/// targets the claim is only relied on up to 8 bytes.
#[cfg(target_pointer_width = "64")]
const MI_MALLOC_ALIGN: usize = 16;
#[cfg(not(target_pointer_width = "64"))]
const MI_MALLOC_ALIGN: usize = 8;

/// mimalloc, calling `mi_malloc` for the alignments it guarantees and
/// `mi_malloc_aligned` (through `MiMalloc`) for the rest. See the module docs.
pub struct SmallAlignMiMalloc;

/// Whether `mi_malloc(size)` returns memory aligned for `layout`: the
/// alignment is at most what mimalloc guarantees, and the size is a non-zero
/// multiple of it (Rust type layouts always are).
#[inline]
fn mi_malloc_fits(align: usize, size: usize) -> bool {
    align <= MI_MALLOC_ALIGN && size >= align && size % align == 0
}

// SAFETY: every method returns memory from mimalloc that satisfies `layout`.
// Where `mi_malloc_fits` holds, `mi_malloc`/`mi_zalloc`/`mi_realloc` return
// blocks aligned to at least `MI_MALLOC_ALIGN` for blocks of that size or
// more (mimalloc's `MI_MAX_ALIGN_SIZE`, asserted in its `alloc.c`), and to
// the word size for smaller ones, whose sizes are multiples of the word
// size; a size that is a multiple of a power-of-two alignment no larger than
// these is therefore aligned for it. Otherwise the call goes to `MiMalloc`,
// which uses the aligned API. All blocks come from the same mimalloc heap,
// and `mi_free` releases any of them, whichever function allocated it.
unsafe impl GlobalAlloc for SmallAlignMiMalloc {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if mi_malloc_fits(layout.align(), layout.size()) {
            libmimalloc_sys::mi_malloc(layout.size()).cast()
        } else {
            MiMalloc.alloc(layout)
        }
    }

    #[inline]
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if mi_malloc_fits(layout.align(), layout.size()) {
            libmimalloc_sys::mi_zalloc(layout.size()).cast()
        } else {
            MiMalloc.alloc_zeroed(layout)
        }
    }

    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        MiMalloc.dealloc(ptr, layout)
    }

    #[inline]
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if mi_malloc_fits(layout.align(), new_size) {
            libmimalloc_sys::mi_realloc(ptr.cast(), new_size).cast()
        } else {
            MiMalloc.realloc(ptr, layout, new_size)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every size and power-of-two alignment up to a few pages, through each
    /// entry point: the memory is aligned, usable and zeroed when asked.
    #[test]
    fn allocations_are_aligned_for_their_layout() {
        let a = SmallAlignMiMalloc;
        let mut align = 1;
        while align <= 4096 {
            for size in (1..=520).chain([1000, 4096, 5000, 70_000]) {
                let Ok(layout) = Layout::from_size_align(size, align) else {
                    continue;
                };
                unsafe {
                    let p = a.alloc(layout);
                    assert!(!p.is_null());
                    assert_eq!(p as usize % align, 0, "alloc {size}/{align}");
                    p.write_bytes(0xAB, size);

                    let new_size = size * 3 + 1;
                    let q = a.realloc(p, layout, new_size);
                    assert!(!q.is_null());
                    assert_eq!(q as usize % align, 0, "realloc {size}/{align}");
                    assert_eq!(*q.add(size - 1), 0xAB, "realloc keeps the data");
                    a.dealloc(q, Layout::from_size_align(new_size, align).unwrap());

                    let z = a.alloc_zeroed(layout);
                    assert_eq!(z as usize % align, 0, "alloc_zeroed {size}/{align}");
                    assert!((0..size).all(|i| *z.add(i) == 0));
                    a.dealloc(z, layout);
                }
            }
            align *= 2;
        }
    }

    #[test]
    fn mi_malloc_is_used_only_for_guaranteed_alignments() {
        assert!(mi_malloc_fits(8, 96));
        assert!(mi_malloc_fits(8, 8));
        assert!(mi_malloc_fits(16, 48));
        assert!(!mi_malloc_fits(32, 64));
        assert!(!mi_malloc_fits(8, 4));
        assert!(!mi_malloc_fits(8, 12));
        assert!(!mi_malloc_fits(8, 0));
    }
}
