
#[cfg(target_os = "linux")]
extern crate libc;

use std::alloc::{handle_alloc_error, GlobalAlloc, Layout};
use super::Locked;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use std::ptr::null_mut;

pub struct BumpAllocator {

    size: usize,
    offset: AtomicUsize,
    initializing: AtomicBool,
    mmap: *mut u8,
}

#[cfg(all(unix, not(target_os = "android")))]
unsafe fn mmap_wrapper(size: usize) -> *mut u8 {
    libc::mmap(
        null_mut(),
        size,
        libc::PROT_READ | libc::PROT_WRITE,
        libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
        -1,
        0,
    ) as *mut u8
}

fn align_to(size: usize, align: usize) -> usize {
    (size + align - 1) & !(align - 1)
}

impl BumpAllocator {
    /// Creates a new empty bump allocator.
    pub const fn new() -> Self {
        BumpAllocator::with_size(1024 * 1024 * 1024) 
    }

    pub const fn with_size(size: usize) -> BumpAllocator {
        BumpAllocator {
            initializing: AtomicBool::new(true),
            mmap: null_mut(),
            offset: AtomicUsize::new(0),
            size: size,
        }
    }
}

unsafe impl Sync for Locked<BumpAllocator> {}

unsafe impl GlobalAlloc for Locked<BumpAllocator> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut bump = self.lock(); // get a mutable reference

        // If initializing is true it means we need to do the original mmap.
        if bump.initializing.swap(false, Ordering::Relaxed) {
            bump.mmap = mmap_wrapper(bump.size);
            if (*bump.mmap as isize) == -1isize {
                handle_alloc_error(layout);
            }
        } else {
            // Spin loop waiting on the mmap to be ready.
            while 0 == bump.offset.load(Ordering::Relaxed) {}
        }

        let bytes_required = align_to(layout.size() + layout.align(), layout.align());

        let my_offset = bump.offset.fetch_add(bytes_required, Ordering::Relaxed);

        let aligned_offset = align_to(my_offset, layout.align());

        if (aligned_offset + layout.size()) > bump.size {
            handle_alloc_error(layout);
        }

        bump.mmap.offset(aligned_offset as isize)
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
    }
}