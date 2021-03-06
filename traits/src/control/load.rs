//! Types and friends that are part of the load API (a part of the
//! [`Control` trait](crate::control::Control).)
//!
//! TODO!

// TODO: as with the types in metadata.rs, the stuff in this file should
// probably eventually move out of control but I'm not sure to where. We seem to
// be accumulating an increasing amount of stuff that is fairly basic
// foundational stuff (which is what the `isa` crate is for) but very much has
// nothing to do with the ISA.

use super::Control;

use lc3_isa::{Addr, Word, Bits, util::MemoryDump, MEM_MAPPED_START_ADDR, ADDR_MAX_VAL, ADDR_SPACE_SIZE_IN_WORDS};

use core::mem::size_of;
use core::hash::{Hash, Hasher};
#[allow(deprecated)] use core::hash::SipHasher;
use core::sync::atomic::{AtomicUsize, Ordering::{SeqCst, Relaxed}};
use core::time::Duration;
use core::convert::TryInto;
use core::ops::Range;

use serde::{Deserialize, Serialize};


pub type PageIndex = u8;
pub type PageOffset = u8;
sa::const_assert_eq!(size_of::<PageIndex>() + size_of::<PageOffset>(), size_of::<Word>());

pub const PAGE_SIZE_IN_WORDS: Addr = (PageOffset::max_value() as Addr) + 1;
pub const NUM_PAGES: usize = (ADDR_SPACE_SIZE_IN_WORDS) / (PAGE_SIZE_IN_WORDS as usize);
pub const NUM_MEM_MAPPED_PAGES: usize = (PageIndex::max_value() - (MEM_MAPPED_START_ADDR >> 8) as PageIndex) as usize + 1;

// The number of words in a chunk must be picked such that there are a whole
// number of chunks in a page.
pub const CHUNK_SIZE_IN_WORDS: PageOffset = 8;
pub const CHUNKS_IN_A_PAGE: usize = (PAGE_SIZE_IN_WORDS as usize) / (CHUNK_SIZE_IN_WORDS as usize);
sa::const_assert_eq!(CHUNKS_IN_A_PAGE * (CHUNK_SIZE_IN_WORDS as usize), (PAGE_SIZE_IN_WORDS as usize));

// TODO: Ideally this would take a reference to an array and not a slice, but
// alas; since we can't easily go from, for example, a slice of a memory dump
// to [Word; PAGE_SIZE_IN_WORDS as usize], we're just taking a size instead.
//
// TODO: perhaps switch to something better suited to being a checksum
pub fn hash_page(page: &[Word]) -> u64 {
    assert_eq!(page.len(), PAGE_SIZE_IN_WORDS as usize);

    #[allow(deprecated)]
    let mut hasher = SipHasher::new(); // TODO: deprecated but what can we do...

    Word::hash_slice(page, &mut hasher);
    hasher.finish()
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Index(pub PageIndex);

impl Index {
    pub const fn with_offset(&self, offset: PageOffset) -> Addr {
        ((self.0 as Addr) << 8) + (offset as Addr)
    }

    pub const fn as_index(&self) -> usize {
        (self.0 as usize) << 8
    }

    pub const fn as_index_range(&self) -> Range<usize> {
        self.as_index()..(Self(self.0 + 1).as_index())
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Offset(pub PageOffset);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[derive(lc3_macros::DisplayUsingDebug)] // TODO: do better
pub enum StartPageWriteError {
    InvalidPage { page: PageIndex }, // Only the mem-mapped page should be invalid...
    UnfinishedSessionExists { unfinished_page: PageIndex },
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[derive(lc3_macros::DisplayUsingDebug)] // TODO: do better
pub enum PageChunkError {
    NoCurrentSession,
    WrongPage { expected_page: PageIndex, received_address: Addr, },
    ChunkCrossesPageBoundary { page: PageIndex, received_address: Addr, },
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[derive(lc3_macros::DisplayUsingDebug)] // TODO: do better
pub enum FinishPageWriteError {
    NoCurrentSession,
    SessionMismatch { current_session_page: PageIndex, received_page: PageIndex },
    ChecksumMismatch { page: PageIndex, given_checksum: u64, computed_checksum: u64 },
}

using_std! {
    impl std::error::Error for StartPageWriteError { }
    impl std::error::Error for PageChunkError { }
    impl std::error::Error for FinishPageWriteError { }
}

// Need this newtype to have `LoadApiSession<Index>` be different than
// `LoadApiSession<Start>`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PageWriteStart(pub PageIndex);

// Private field so users can't construct this manually.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadApiSession<State>(State);

impl<T: Copy> LoadApiSession<T> { pub fn get(&self) -> T { self.0 } }

// The order goes:
//
// -> LoadApiSession<PageWriteStart> -> Result<LoadApiSession<Index>, _>
//                       /----------------------->  |
//                      /                           v
//             LoadApiSession<Offset>  <--  LoadApiSession<Index>
//                      |                           |
//                      v                           v
//     send_page_chunk(...) /* moves */     finish(...) /* _consumes_ */

// Alternatively, to make it so that you can't start a new session without the
// old one being finished (this is a Trade Off):
//
// static TOKEN: Cell<Option<LoadApiSession<Empty>>> = Cell::new(Some(...));
//
//             LoadApiSession<Empty>
//                      |
//                      v
// LoadApiSession::new(..., page: PageIndex) -> Result<LoadApiSession<PageWriteStart>, _>
//                                                                 |
// /---------------------------------------------------------------/
// |
// \-> LoadApiSession<PageWriteStart> -> Result<LoadApiSession<Index>, _>
//                        /----------------------->  |
//                       /                           v
//              LoadApiSession<Offset>  <--  LoadApiSession<Index>
//                       |                           |
//                       v                           v  /* _consumes_ */
//      send_page_chunk(...) /* moves */     finish(...) -> Result<LoadApiSession<Empty>, LoadApiSession<Empty>>
//                                                                          |                    |
//                                                                          v                    V
//                                         (can be used to start a new session)    (return the same on error
//                                                                                    so you can start again)
//

pub trait PageAccess {
    fn page_idx(self) -> PageIndex;
    fn page_offset(self) -> PageOffset;
}

impl PageAccess for Addr {
    fn page_idx(self) -> PageIndex { self.u8(8..15) }
    fn page_offset(self) -> PageOffset { self.u8(0..7) }
}

impl LoadApiSession<PageWriteStart> {
    /// This is still unsafe since there's one major error case that we don't
    /// try to detect or report: calling other functions whilst in the middle of
    /// a load session.
    ///
    /// We still recommend that you just use [the porcelain](load_memory_dump) but if you
    /// want to do things manually, you must ensure that you do not call any
    /// other functions on [`Control`](crate::control::Control). If this
    /// invariant is not upheld, while memory safety issues will *not* occur
    /// (there is no _real_ unsafe code anywhere here), we make no guarantees
    /// about the data in [`Memory`](crate::memory::Memory).
    ///
    /// Additionally, we recommend you use [`load_memory_dump`] so you don't
    /// have to do the hashes yourself though note that computing the hashes
    /// wrong on the client side isn't an issue the way the previous thing
    /// (calling other functions during a load) is. At worst, you'll end up with
    /// the wrong data in the current page and not know it.
    #[allow(unsafe_code)]
    pub unsafe fn new(page: PageIndex) -> Result<LoadApiSession<PageWriteStart>, StartPageWriteError> {
        if page >= MEM_MAPPED_START_ADDR.page_idx() {
            Err(StartPageWriteError::InvalidPage { page })
        } else {
           Ok(LoadApiSession(PageWriteStart(page)))
        }
    }

    // The device side version of the above.
    //
    // Ideally this would be a private function that only things within this
    // module can call, thereby ensuring that 'user code' can't construct a
    // `LoadApiSession<Index>` and circumvent the steps in the Control
    // interface.
    //
    // Unfortunately, if we made this private all implementors would have to
    // live in this module (or this crate) which is unacceptable.
    //
    // What we really want is some way for users of this function to prove to
    // us that they are a Control interface implementor (this is still
    // problematic since users could go and have a 'fake' Control impl for the
    // purpose of circumventing the steps) which we have no way to do.
    //
    // So for now, we just mark this function as unsafe. Control interface
    // implementors can safely this with no problems.
    #[allow(unsafe_code)]
    pub unsafe fn approve(self) -> Result<LoadApiSession<PageIndex>, StartPageWriteError> {
        // If they managed to construct a `LoadApiSession<PageIndex>`, the page
        // is valid, but it never hurts to check again:
        let page = self.get().0;

        if page >= MEM_MAPPED_START_ADDR.page_idx() {
            Err(StartPageWriteError::InvalidPage { page })
        } else {
            Ok(LoadApiSession(page))
        }
    }
}

impl LoadApiSession<PageIndex> {
    // NoCurrentSession isn't possible since you can only get the parent type
    // from a successfully started session.
    pub fn with_offset(&self, addr: Addr) -> Result<LoadApiSession<Offset>, PageChunkError> {
        if addr.page_idx() == self.0 {
            if let Some(last) = addr.checked_add((CHUNK_SIZE_IN_WORDS - 1) as Addr) {
                if addr.page_idx() == last.page_idx() {
                    Ok(LoadApiSession(Offset(addr.page_offset())))
                } else {
                    Err(PageChunkError::ChunkCrossesPageBoundary { page: self.0, received_address: addr })
                }
            } else {
                // This should never happens since we don't allow writes to the
                // end of the address space (i.e. mem mapped space) anyways and
                // don't give you a `LoadApiSession<PageIndex>` if you try to do
                // this.
                unreachable!("Attempted to write past the end of the address space!");
            }
        } else {
            Err(PageChunkError::WrongPage { expected_page: self.0, received_address: addr })
        }
    }
}

mod private {
    pub trait LoadMemoryProgressSource {
        fn total_number_of_pages_to_send(&self, pages: usize);

        fn chunk_attempt(&self);

        fn page_attempt(&self);
        fn page_success(&self, num_successful_chunks: super::PageOffset); // should be < CHUNKS_IN_A_PAGE
    }
}

use private::LoadMemoryProgressSource;

pub trait LoadMemoryProgress: LoadMemoryProgressSource {
    fn progress(&self) -> f32;
    fn success_rate(&self) -> f32;
}

pub struct Progress {
    /// Duration between start time and the Unix Epoch
    pub start_time: Option<Duration>,
    /// Number of chunks sent, including failures
    pub sent_chunks: AtomicUsize/*usize*/,
    /// Number of chunks sent (including failures) since the last successful
    /// page write
    pub sent_chunks_for_page: AtomicUsize,
    /// Number of unique chunks sent (each chunk is counted only once even if it
    /// had to be sent multiple times due to failures)
    pub sent_unique_chunks: AtomicUsize/*usize*/,
    /// Number of pages sent, including empties and failures
    pub sent_pages: AtomicUsize/*usize*/,
    /// Number of pages remaining, including empties
    pub remaining_pages: AtomicUsize/*usize*/,
    /// Number of total pages to be sent: the a priori estimate
    pub total_pages: AtomicUsize,
}

impl LoadMemoryProgressSource for Progress {
    fn total_number_of_pages_to_send(&self, pages: usize) {
        self.total_pages.store(pages, SeqCst);
        self.remaining_pages.store(pages, SeqCst);

        self.sent_chunks.store(0, SeqCst);
        self.sent_chunks_for_page.store(0, SeqCst);
        self.sent_unique_chunks.store(0, SeqCst);
        self.sent_pages.store(0, SeqCst);
    }

    fn chunk_attempt(&self) {
        self.sent_chunks.store(self.sent_chunks.load(Relaxed) + 1, Relaxed);
        self.sent_chunks_for_page.store(self.sent_chunks_for_page.load(Relaxed) + 1, Relaxed);
    }

    fn page_attempt(&self) {
        self.sent_pages.store(self.sent_pages.load(Relaxed) + 1, Relaxed);
    }

    // should be < CHUNKS_IN_A_PAGE
    fn page_success(&self, num_successful_chunks: PageOffset) {
        self.remaining_pages.store(self.remaining_pages.load(Relaxed) - 1, Relaxed);
        self.sent_chunks_for_page.store(0, Relaxed);

        self.sent_unique_chunks.store(self.sent_unique_chunks.load(Relaxed) + (num_successful_chunks as usize), Relaxed);
    }
}

impl LoadMemoryProgress for Progress {
    fn progress(&self) -> f32 {
        // We have options here.

        // An easy one is just:
        // `(1f32 - (self.remaining_pages / self.total_pages))`

        // A pessimistic one is:
        // ```
        // let remaining_chunks = self.remaining_pages * CHUNKS_IN_A_PAGE;
        // let total_chunks = remaining_chunks + self.sent_unique_chunks;
        // (1f32 - (remaining_chunks / total_chunks))
        // ```

        // One that updates as we send chunks (not just as we send pages) and
        // also tries to factor in the ratio of failed chunks:
        // let sent_unique_chunks = self.sent_unique_chunks.load(Relaxed);
        // let success_ratio: f32 = (sent_chunks as f32) / (sent_unique_chunks as f32);

        let remaining_pages = self.remaining_pages.load(Relaxed);
        let total_pages = self.total_pages.load(Relaxed);
        let successfully_sent_pages = total_pages - remaining_pages;
        let base: f32 = successfully_sent_pages as f32 / total_pages as f32;
        // let base: f32 = 1f32 - ((remaining_pages as f32) / (total_pages as f32));

        // Alternative ratio:
        // let sent_pages = self.sent_pages.load(Relaxed);
        // let chunks_per_page = ((sent_pages as f32) / (successfully_sent_pages as f32)) * (CHUNKS_IN_A_PAGE as f32);

        // Ratio:
        let sent_chunks = self.sent_chunks.load(Relaxed);
        let chunks_per_page: f32 = (sent_chunks as f32) / (successfully_sent_pages as f32);
        // ^ factors in the success ratio for chunks and assumes that the
        // non-zero data density is the same across all pages (a flawed
        // assumption, probably)

        // Based on the above ratio and the number of chunks we are into the
        // current page, estimate our progress *for the current page*:
        let chunks_into_current_page = self.sent_chunks_for_page.load(Relaxed);
        let current_page_progress: f32 = chunks_into_current_page as f32 / chunks_per_page;
        let current_page_progress = current_page_progress.max(1.0);

        // Scale it and add to the total percentage:
        base + (current_page_progress / (total_pages as f32))
    }

    fn success_rate(&self) -> f32 {
        // (num successful chunks) / (num total sent chunks)
        let successful = self.sent_unique_chunks.load(Relaxed);
        let total_sent = self.sent_chunks.load(Relaxed);

        (successful as f32) / (total_sent as f32)
    }
}

impl Progress {
    pub const fn new() -> Progress {
        Progress {
            start_time: None,
            sent_chunks: AtomicUsize::new(0),
            sent_chunks_for_page: AtomicUsize::new(0),
            sent_unique_chunks: AtomicUsize::new(0),
            sent_pages: AtomicUsize::new(0),
            remaining_pages: AtomicUsize::new(0),
            total_pages: AtomicUsize::new(0),
        }
    }

    // Delegate to `LoadMemoryProgress::progress` (since inherent functions
    // have priority over trait functions, this won't cause name resolution
    // problems and will make it so users don't have to `use` the trait).
    pub fn progress(&self) -> f32 {
        <Self as LoadMemoryProgress>::progress(self)
    }
}

using_std! {
    use std::time::{SystemTime, SystemTimeError};

    impl Progress {
        pub fn new_with_time() -> Result<Progress, SystemTimeError> {
            let time = SystemTime::now();
            let time = time.duration_since(SystemTime::UNIX_EPOCH)?;

            Ok(Progress {
                start_time: Some(time),
                ..Self::new()
            })
        }

        // TODO: we could offer no_std Duration based versions of these
        // functions, but does anyone care? Would anyone need them?

        pub fn time_elapsed(&self) -> Option<Duration> {
            let start = self.start_time?;
            let start = SystemTime::UNIX_EPOCH.checked_add(start)?;

            start.elapsed().ok()
        }

        pub fn estimate_time_remaining(&self) -> Option<Duration> {
            // no hysteresis for now, just simple scaling
            // (progress / 1) = (elapsed / total time)
            // ((1 / progress) * (elapsed)) - elapsed

            let progress = self.progress();
            let elapsed = self.time_elapsed()?;

            Some(elapsed.mul_f32((1f32 / progress).min(1_000_000.0f32)).max(elapsed) - elapsed)
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[derive(lc3_macros::DisplayUsingDebug)] // TODO: do better
pub enum LoadMemoryDumpError {
    MemMappedPagesNotEmpty,
    ExistingUnfinishedSession { unfinished_page: PageIndex },
}

using_std! {
    impl std::error::Error for LoadMemoryDumpError { }
}

#[inline]
pub fn load_memory_dump<C: Control + ?Sized, P: LoadMemoryProgress>(sim: &mut C, dump: &MemoryDump, previous: Option<&MemoryDump>, progress: Option<&P>) -> Result<(), LoadMemoryDumpError> {
    // Because this takes a mutable reference to the Control impl, we're
    // basically guaranteeing exclusive access to the Control impl so we can
    // ensure that there are no calls to other functions on the Control trait
    // once we start a session (allowing us to be sure that we're maintaining
    // the invariant required to make constructing and using a
    // LoadApiSession<PageWriteStart> safe).

    macro_rules! p {
        ($p:ident -> $($all:tt)*) => { if let Some($p) = progress { $($all)* }};
    }

    let mut num_to_write: usize = NUM_PAGES - NUM_MEM_MAPPED_PAGES;
    let mut write_or_not = [true; NUM_PAGES - NUM_MEM_MAPPED_PAGES]; // : [bool; NUM_PAGES - NUM_MEM_MAPPED_PAGES]

    // First, let's check that we're not being told to write to the mem mapped
    // area (which we can't do):
    if (MEM_MAPPED_START_ADDR..=ADDR_MAX_VAL).map(|addr| dump[addr as usize]).any(|v| v != 0) {
        return Err(LoadMemoryDumpError::MemMappedPagesNotEmpty)
    }

    // Next, if we were given a previous MemoryDump to diff against, do that:
    // (mark unmodified pages as being the same)
    if let Some(previous) = previous {
        for p_idx in 0..(MEM_MAPPED_START_ADDR.page_idx()) {
            write_or_not[p_idx as usize] = (0..=PAGE_SIZE_IN_WORDS)
                .map(|offset| Index(p_idx).with_offset(offset as PageOffset) as usize)
                .any(|addr| dump[addr] != previous[addr]);

            if !write_or_not[p_idx as usize] { num_to_write -= 1; }
        }
    }

    p!(p -> p.total_number_of_pages_to_send(num_to_write));

    // Now, go and send all the marked pages:
    for (p_idx, _) in write_or_not.iter().enumerate().filter(|(_, to_write)| **to_write) {
        let page = &dump[(p_idx * (PAGE_SIZE_IN_WORDS as usize))..((p_idx + 1) * (PAGE_SIZE_IN_WORDS as usize))];
        let checksum = hash_page(page); // We'll use a hash of the page as our checksum for now.

        loop {
            // Start the page write:
            let token = /*loop*/ {
                // (this is safe; see the blurb at the top of this function)
                #[allow(unsafe_code)]
                let page = unsafe { LoadApiSession::new(p_idx as PageIndex) }.unwrap();

                p!(p -> p.page_attempt());
                match sim.start_page_write(page, checksum) {
                    Ok(token) => token,
                    Err(StartPageWriteError::InvalidPage { .. }) => unreachable!(),
                    Err(StartPageWriteError::UnfinishedSessionExists { unfinished_page }) => {
                        // Bail:
                        return Err(LoadMemoryDumpError::ExistingUnfinishedSession { unfinished_page })
                    }
                }
            };

            let mut non_empty_pages = 0;

            // Now try to go write all the (non-empty) pages:
            for (idx, chunk) in page.chunks_exact(CHUNK_SIZE_IN_WORDS as usize).enumerate() {
                if chunk.iter().any(|w| *w != 0) {
                    non_empty_pages += 1;

                    let offset = token.with_offset(Index(p_idx as PageIndex).with_offset(idx as PageOffset * CHUNK_SIZE_IN_WORDS)).unwrap();
                    let chunk = chunk.try_into().unwrap();

                    p!(p -> p.chunk_attempt());
                    match sim.send_page_chunk(offset, chunk) {
                        Ok(()) => { },
                        Err(PageChunkError::ChunkCrossesPageBoundary { .. }) |
                        Err(PageChunkError::NoCurrentSession) |
                        Err(PageChunkError::WrongPage { .. }) => unreachable!(),
                    }
                }
            }

            // Finally, finish the page:
            match sim.finish_page_write(token) {
                Ok(()) => { p!(p -> p.page_success(non_empty_pages)); break; }
                Err(FinishPageWriteError::NoCurrentSession) |
                Err(FinishPageWriteError::SessionMismatch { .. }) => unreachable!(),
                Err(FinishPageWriteError::ChecksumMismatch { page, given_checksum, computed_checksum }) => {
                    assert_eq!(page, p_idx as u8);
                    assert_eq!(checksum, given_checksum);
                    assert_ne!(checksum, computed_checksum);

                    // We'll try again...
                }
            }
        }
    }

    sim.reset();
    Ok(())
}

pub fn load_whole_memory_dump<C: Control + ?Sized, P: LoadMemoryProgress>(sim: &mut C, dump: &MemoryDump, progress: Option<&P>) -> Result<(), LoadMemoryDumpError> {
    load_memory_dump(sim, dump, None, progress)
}

pub fn load_memory_dump_without_progress<C: Control + ?Sized>(sim: &mut C, dump: &MemoryDump, previous: &MemoryDump) -> Result<(), LoadMemoryDumpError> {
    load_memory_dump::<_, Progress>(sim, dump, Some(previous), None)
}

pub fn load_whole_memory_dump_without_progress<C: Control + ?Sized>(sim: &mut C, dump: &MemoryDump) -> Result<(), LoadMemoryDumpError> {
    load_whole_memory_dump::<_, Progress>(sim, dump, None)
}
