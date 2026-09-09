//! Standalone measurement fixture built by measure.py against both revisions.
//! Generated nesting calls the preceding level once: depth means invoke edges.

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use corophage::prelude::*;

struct CountingAllocator;
static COUNTING: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

// Delegate every operation unchanged. Enable counting only for separate samples,
// so the atomic increment does not distort the timed dispatch measurements.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.realloc(ptr, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[effect(u64)]
struct Tick(u64);

#[effectful(Tick)]
fn flat(count: usize) -> u64 {
    let mut sum = 0;
    for _ in 0..count {
        sum += yield_!(Tick(black_box(1)));
    }
    sum
}

// NESTING_FUNCTIONS

fn tick(Tick(n): Tick) -> Control<u64> {
    Control::resume(black_box(n))
}

async fn async_tick(effect: Tick) -> Control<u64> {
    tick(effect)
}

fn state_tick(state: &mut u64, Tick(n): Tick) -> Control<u64> {
    *state += n;
    Control::resume(black_box(n))
}

async fn async_state_tick(state: &mut u64, effect: Tick) -> Control<u64> {
    state_tick(state, effect)
}

fn sample(name: &str, mut operation: impl FnMut()) {
    operation(); // Warm lazy runtime/thread-local storage before counting.
    ALLOCATIONS.store(0, Ordering::Relaxed);
    COUNTING.store(true, Ordering::Relaxed);
    operation();
    COUNTING.store(false, Ordering::Relaxed);
    let allocations = ALLOCATIONS.load(Ordering::Relaxed);

    let mut iterations = 1;
    loop {
        let start = Instant::now();
        for _ in 0..iterations {
            operation();
        }
        if start.elapsed() >= Duration::from_millis(10) {
            break;
        }
        iterations *= 2;
    }
    let mut times = [0.0_f64; 7];
    for time in &mut times {
        let start = Instant::now();
        for _ in 0..iterations {
            operation();
        }
        *time = start.elapsed().as_nanos() as f64 / iterations as f64;
    }
    times.sort_by(f64::total_cmp);
    println!(
        "{{\"name\":\"{name}\",\"allocations\":{allocations},\"median_ns\":{},\"min_ns\":{},\"max_ns\":{}}}",
        times[3], times[0], times[6]
    );
}

fn size(name: &str, value: &impl Sized) {
    println!(
        "{{\"name\":\"{name}\",\"bytes\":{}}}",
        std::mem::size_of_val(value)
    );
}

fn main() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    sample("creation", || drop(black_box(flat(black_box(1)))));
    size("program_bytes", &flat(1));
    size("run_future_bytes", &flat(1).handle(async_tick).run());
    for count in [1, 1000] {
        sample(&format!("sync_{count}"), || {
            assert_eq!(
                black_box(flat(black_box(count)).handle(tick).run_sync()),
                Ok(count as u64)
            );
        });
        sample(&format!("async_{count}"), || {
            assert_eq!(
                runtime.block_on(flat(black_box(count)).handle(async_tick).run()),
                Ok(count as u64)
            );
        });
        sample(&format!("state_sync_{count}"), || {
            let mut state = 0;
            #[cfg(not(feature = "candidate"))]
            let result = flat(black_box(count))
                .handle(state_tick)
                .run_sync_stateful(&mut state);
            #[cfg(feature = "candidate")]
            let result = flat(black_box(count))
                .with_state(&mut state)
                .handle(state_tick)
                .run_sync();
            assert_eq!(black_box(result), Ok(count as u64));
            assert_eq!(state, count as u64);
        });
        sample(&format!("state_async_{count}"), || {
            let mut state = 0;
            #[cfg(not(feature = "candidate"))]
            let result = runtime.block_on(
                flat(black_box(count))
                    .handle(async_state_tick)
                    .run_stateful(&mut state),
            );
            #[cfg(feature = "candidate")]
            let result = runtime.block_on(
                flat(black_box(count))
                    .with_state(&mut state)
                    .handle(async_state_tick)
                    .run(),
            );
            assert_eq!(black_box(result), Ok(count as u64));
            assert_eq!(state, count as u64);
        });
    }
    // NESTING_SAMPLES

    // Additional candidate-only cases are built separately so their code does
    // not affect the common-workload compile-time and binary-size comparison.
    #[cfg(feature = "extras")]
    for count in [1, 1000] {
        sample(&format!("boxed_sync_{count}"), || {
            assert_eq!(
                black_box(flat(black_box(count)).boxed().handle(tick).run_sync()),
                Ok(count as u64)
            );
        });
        sample(&format!("boxed_send_async_{count}"), || {
            assert_eq!(
                runtime.block_on(flat(black_box(count)).boxed_send().handle(async_tick).run()),
                Ok(count as u64)
            );
        });
        sample(&format!("local_lending_{count}"), || {
            let mut captured = 0;
            let result = runtime.block_on(
                flat(black_box(count))
                    .handle(corophage::local(async |Tick(n)| {
                        captured += n;
                        std::future::ready(()).await;
                        Control::resume(n)
                    }))
                    .run(),
            );
            assert_eq!(black_box(result), Ok(count as u64));
            assert_eq!(captured, count as u64);
        });
    }
}
