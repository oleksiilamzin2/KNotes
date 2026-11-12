use std::alloc::{GlobalAlloc, Layout, System};

/// Blocks of at least this size bypass malloc.
pub const LARGE: usize = 128 << 10;

/// Alignment every mapping has (the smallest page size of supported Macs).
const MAP_ALIGN: usize = 4096;

/// Small blocks come from the system malloc; blocks of `LARGE` bytes and more are mapped
/// straight from the kernel and unmapped when freed. macOS malloc keeps freed large blocks
/// in a cache, dirty and counted in the app's footprint; after a cold index of 50k notes
/// that cache held about 250 MB, released only under memory pressure (ADR 0014).
pub struct DirectLarge;

fn direct(size: usize, align: usize) -> bool {
    cfg!(target_os = "macos") && size >= LARGE && align <= MAP_ALIGN
}

#[cfg(target_os = "macos")]
mod vm {
    use std::ffi::c_void;

    const PROT_READ_WRITE: i32 = 0x1 | 0x2;
    const MAP_PRIVATE_ANON: i32 = 0x0002 | 0x1000;
    const MAP_FAILED: *mut c_void = !0 as *mut c_void;

    unsafe extern "C" {
        fn mmap(addr: *mut c_void, len: usize, prot: i32, flags: i32, fd: i32, offset: i64) -> *mut c_void;
        fn munmap(addr: *mut c_void, len: usize) -> i32;
        fn getpagesize() -> i32;
    }

    /// Zero-filled pages, page-aligned; null when the kernel refuses.
    pub fn map(size: usize) -> *mut u8 {
        // SAFETY: an anonymous private mapping at an address the kernel picks.
        let p = unsafe { mmap(std::ptr::null_mut(), size, PROT_READ_WRITE, MAP_PRIVATE_ANON, -1, 0) };
        if p == MAP_FAILED { std::ptr::null_mut() } else { p.cast() }
    }

    /// # Safety
    /// `ptr` and `size` must come from one `map` call (sizes rounding to the same pages).
    pub unsafe fn unmap(ptr: *mut u8, size: usize) {
        // SAFETY: the caller passes a mapping made by `map`.
        unsafe { munmap(ptr.cast(), size) };
    }

    pub fn pages(size: usize) -> usize {
        // SAFETY: a plain query.
        let page = unsafe { getpagesize() } as usize;
        size.div_ceil(page)
    }
}

#[cfg(not(target_os = "macos"))]
mod vm {
    pub fn map(_: usize) -> *mut u8 {
        unreachable!("direct mappings are macOS only")
    }
    pub unsafe fn unmap(_: *mut u8, _: usize) {}
    pub fn pages(size: usize) -> usize {
        size
    }
}

unsafe impl GlobalAlloc for DirectLarge {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if direct(layout.size(), layout.align()) {
            vm::map(layout.size())
        } else {
            // SAFETY: forwarding the caller's layout.
            unsafe { System.alloc(layout) }
        }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if direct(layout.size(), layout.align()) {
            // Fresh anonymous pages are zero already.
            vm::map(layout.size())
        } else {
            // SAFETY: forwarding the caller's layout.
            unsafe { System.alloc_zeroed(layout) }
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if direct(layout.size(), layout.align()) {
            // SAFETY: a block of this layout was mapped by `alloc`.
            unsafe { vm::unmap(ptr, layout.size()) }
        } else {
            // SAFETY: a block of this layout came from the system allocator.
            unsafe { System.dealloc(ptr, layout) }
        }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let (was, will) = (direct(layout.size(), layout.align()), direct(new_size, layout.align()));
        if !was && !will {
            // SAFETY: both sizes are small: the block stays with the system allocator.
            return unsafe { System.realloc(ptr, layout, new_size) };
        }
        if was && will && vm::pages(layout.size()) == vm::pages(new_size) {
            // The mapping already covers the new size, and unmapping either size frees it whole.
            return ptr;
        }
        // SAFETY: `new_size` is non-zero and does not overflow with this alignment (the
        // caller's contract for realloc).
        let new_layout = unsafe { Layout::from_size_align_unchecked(new_size, layout.align()) };
        // SAFETY: a valid layout.
        let new = unsafe { self.alloc(new_layout) };
        if !new.is_null() {
            // SAFETY: both blocks are valid for the copied length and do not overlap; the
            // old block is freed with its own layout.
            unsafe {
                std::ptr::copy_nonoverlapping(ptr, new, layout.size().min(new_size));
                self.dealloc(ptr, layout);
            }
        }
        new
    }
}
