//! The small rayon pool the def-replaying passes run on: `VerifyOrder`'s
//! def keys and `Session::prefetch_clean_merge_previews`' merge previews.
//!
//! Capped rather than the global pool (one thread per logical CPU) so these
//! passes leave most of the CPU to the game when it is running alongside,
//! and so their cost stays bounded on a machine with more cores. The scan
//! and the analyzer stay on the global pool.
//!
//! The cap was re-measured once each def file was parsed once per pass
//! (`rim_io::FileDefSourceReader`) and checked on disk once per call, on a
//! real install of about 1,000 active mods (8 cores, 16 threads, release
//! build, the install loaded first on the global pool as in the app, one
//! reader shared by every call, median of 3 interleaved runs of an
//! in-process sweep of the pool size). Verifying one load order,
//! counterfactual included, took about 7.2 s on 2 threads, 4.4 s on 4,
//! 4.0 s on 6, 3.7 s on 8 and 3.7 s on the 16-thread global pool. Compare
//! these only with each other: the sweep ran in its own session, on a
//! quieter machine than the base-versus-new comparison in the changelog,
//! whose verify of the same code on 4 threads took about 5.1 s; the
//! processor time it used grew with the thread count (about 13 s
//! on 2 and 4 threads, 15 s on 6, 16 to 18 s on 8, 19 s on 16). Peak working
//! set during a verify was about 800 MB on 2 to 8 threads and about 830 to
//! 860 MB on 16, rising with each verify. Building the merge mod took about
//! 0.8 s on 2 threads and about 0.6 s on 4 or more. More threads are faster
//! but not at a lower peak memory: 6 saves about 0.4 s per verify for about
//! 10% more processor time, and 8 would take half of this machine's logical
//! CPUs away from a running game, so the cap stays at 4.
//!
//! Before files were parsed once per pass, these passes got slower past
//! about 4 threads (a verify took about 6.6 s on 4 threads and 10 to 12 s on
//! 16) and their working set grew by about 50 MB on 4 threads and 150 MB on
//! 16; that is why the cap was first set at 4.

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
