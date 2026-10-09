//! The small rayon pool the def-replaying passes run on: `VerifyOrder`'s
//! def keys and `Session::prefetch_clean_merge_previews`' merge previews.
//!
//! Capped rather than the global pool (one thread per logical CPU), for two
//! measured reasons on a real install of about 1,000 active mods (8 cores,
//! 16 threads, release build): these passes stop getting faster past about 4
//! threads and get *slower* at 16 (the verify phase alone, run on a pool of
//! that size in a fresh process, took about 21 s on 1 thread, 6.6 s on 4 and
//! 10 to 12 s on 16, counterfactual included), and their extra working set
//! grows with every thread replaying a def at once (about 50 MB on 4
//! threads, 150 MB on 16). A fixed cap keeps both bounded on a machine with
//! more cores, and leaves the rest of the CPU to the game when it is running
//! alongside. The scan and the analyzer, which do keep scaling, stay on the
//! global pool.
//!
//! The 6.6 s is the best case. In the app the install is loaded first on the
//! 16-thread global pool, and a verify on this 4-thread pool afterwards
//! measured about 12 s per order; why a prior 16-thread load slows it is not
//! understood (a suspected cause is the heap state it leaves plus re-parsing
//! in `FileDefSourceReader::read_element`).

use std::sync::OnceLock;

use rayon::{ThreadPool, ThreadPoolBuildError, ThreadPoolBuilder};

/// How many threads replay defs at once — see this module's doc comment.
const REPLAY_THREADS: usize = 4;

/// Built on first use and kept for the process's lifetime, so a pass never
/// pays for spawning threads.
static POOL: OnceLock<Result<ThreadPool, ThreadPoolBuildError>> = OnceLock::new();

/// Runs `op` on the replay pool, so every rayon call inside it uses at most
/// [`REPLAY_THREADS`] threads.
///
/// If the pool cannot be built (the OS refused to start its threads), `op`
/// runs where it was called instead, on the global pool: only the thread
/// cap is lost, never a result, since every parallel step these passes run
/// folds its results back in input order whichever pool runs it.
pub(crate) fn install<R: Send>(op: impl FnOnce() -> R + Send) -> R {
    let pool = POOL.get_or_init(|| {
        ThreadPoolBuilder::new()
            .num_threads(REPLAY_THREADS)
            .thread_name(|index| format!("rimmerge-replay-{index}"))
            .build()
    });
    match pool {
        Ok(pool) => pool.install(op),
        Err(error) => {
            // rim-session has no logger, so a debug build fails loudly here
            // rather than letting a lost thread cap pass unseen in tests; a
            // release build still returns the correct result, uncapped.
            debug_assert!(false, "the replay pool could not be built: {error}");
            op()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_installed_on_the_replay_pool_runs_on_exactly_its_capped_thread_count() {
        let thread_count = install(rayon::current_num_threads);

        assert_eq!(thread_count, REPLAY_THREADS);
    }
}
