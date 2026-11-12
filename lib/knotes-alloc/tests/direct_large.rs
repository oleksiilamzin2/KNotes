use std::alloc::{GlobalAlloc, Layout};

use knotes_alloc::{DirectLarge, LARGE};

#[global_allocator]
static ALLOCATOR: DirectLarge = DirectLarge;

/// Physical footprint of this process in MB (what Activity Monitor shows).
fn footprint_mb() -> f64 {
    #[repr(C)]
    #[derive(Default)]
    struct RusageInfoV0 {
        uuid: [u8; 16],
        times_and_counts: [u64; 6],
        resident_size: u64,
        phys_footprint: u64,
        abstimes: [u64; 2],
    }
    unsafe extern "C" {
        fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut RusageInfoV0) -> i32;
    }
    let mut info = RusageInfoV0::default();
    assert_eq!(unsafe { proc_pid_rusage(std::process::id() as i32, 0, &mut info) }, 0);
    info.phys_footprint as f64 / (1 << 20) as f64
}

#[test]
fn contents_survive_growing_past_and_shrinking_below_the_threshold() {
    let mut v: Vec<u32> = Vec::new();
    for i in 0..3_000_000u32 {
        v.push(i.wrapping_mul(2_654_435_761));
    }
    assert!(v.capacity() * 4 > LARGE);
    assert!(v.iter().enumerate().all(|(i, &x)| x == (i as u32).wrapping_mul(2_654_435_761)));
    v.truncate(100);
    v.shrink_to_fit();
    assert!(v.iter().enumerate().all(|(i, &x)| x == (i as u32).wrapping_mul(2_654_435_761)));
}

#[test]
fn large_zeroed_blocks_are_zero_and_aligned() {
    let zeros = vec![0u8; 8 << 20];
    assert!(zeros.iter().all(|&b| b == 0));
    let layout = Layout::from_size_align(LARGE * 3, 4096).unwrap();
    let p = unsafe { ALLOCATOR.alloc(layout) };
    assert!(!p.is_null() && (p as usize).is_multiple_of(4096));
    unsafe { ALLOCATOR.dealloc(p, layout) };
}

#[test]
fn freed_large_blocks_leave_the_footprint() {
    let before = footprint_mb();
    for round in 0..4 {
        // Many sizes, as growing vectors and file buffers make: a cache keeps them all.
        let blocks: Vec<Vec<u8>> = (1..=40).map(|i| vec![round as u8 + 1; i * 200_000]).collect();
        assert!(blocks.iter().all(|b| b[b.len() - 1] == round as u8 + 1));
    }
    let after = footprint_mb();
    assert!(after - before < 16.0, "{before:.1} MB -> {after:.1} MB");
}
