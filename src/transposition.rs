use std::mem;

/// ===========================================
/// ==== Transposition table data structure ===
/// ===========================================

/// If we change this const to not be a power of 2,
/// then we need to change the unsafe code in get_bucket().
const TT_DIM: usize = 1 << 24;
const TT_DIM_MINUS_1: usize = TT_DIM - 1;

/// === Entry ===

/// Entry containing data about a game state.
/// We use: repr(C) to ensure memory layout is predictable.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TT_entry {
    pub hash_last32: u32,
    pub hash_mid16: u16,
    pub generation: u32, // used in the collision handling
    pub n_visits: u32,
    pub n_wins: i32,
    // TO DO: rethink size
    // Then update add_entry()
}

impl Default for TT_entry {
    fn default() -> Self {
        // Note: the following code is safe until all attributes of the
        // structure are integers and floats. For booleans it might be
        // dangerous. For pointers it causes a CRASH.
        unsafe { mem::zeroed() }
    }
}

impl TT_entry {
    /// Check whether a hash corresponds to an entry.
    pub fn hash_equals(&self, hash: u64) -> bool {
        
    }

    /// Store the relevant bits of the hash in the table entry.

    /// Compute next hash given a move.

}

/// === Bucket ===

/// align(64) aligns to cache lines (optimized and avoids False Sharing).
#[repr(C, align(64))]
#[derive(Clone, Copy)]
pub struct TT_bucket {
    pub entries: [TT_entry; 4],
}

impl Default for TT_bucket {
    fn default() -> Self {
        unsafe { mem::zeroed() }
    }
}

impl TT_bucket {
    /// Get entry corresponding to a hash.
    pub fn get_entry(&mut self, hash: u64) -> Option<&mut TT_entry> {
        for entry in &mut self.entries {
            if entry.hash_equals(hash) {
                return Some(entry); // Found entry.
            }
        }
        None // Not found entry.
    }

    /// Look for the entry in the bucket.
    /// If found, do nothing.
    /// If not found, add it with zero values; overwrite according to collision handling policy:
    /// overwrite the least visited entry among the entries outside the generation range.
    /// (MCTS - Expand)
    pub fn add_entry(&mut self, hash: u64, generation: u32, generation_bound: u32) {
        let mut min_visits = u32::MAX;
        let mut min_index = usize::MAX;

        for (index, entry) in (&mut self.entries).into_iter().enumerate() {
            if entry.hash_equals(hash) {
                return; // Already exists, do nothing.
            }
            // If empty entry.
            if entry.hash_equals(0) {
                entry.set_hash(hash);
                entry.generation = generation;
                entry.n_visits = 0;
                entry.n_wins = 0;
                return;
            }
            // If found entry outside the generation range.
            if entry.generation < generation_bound {
                if entry.n_visits < min_visits {
                    min_visits = entry.n_visits;
                    min_index = index;
                }
            }
        }

        // If bucket is full.
        if min_visits == u32::MAX {
            println!("Error: Bucket full at hash {}", hash);
            println!("Overwrite least visited entry inside range.");
            for (index, entry) in (&mut self.entries).into_iter().enumerate() {
                if entry.n_visits < min_visits {
                    min_visits = entry.n_visits;
                    min_index = index;
                }
            }
        }

        // Overwrite.
        self.entries[min_index].set_hash(hash);
        self.entries[min_index].generation = generation;
        self.entries[min_index].n_visits = 0;
        self.entries[min_index].n_wins = 0;
    }
}

/// === Transposition table ===
pub struct TT {
    pub buckets: Box<[TT_bucket]>,
}

impl TT {
    pub fn new() -> Self {
        let buckets = vec![TT_bucket::default(); TT_DIM].into_boxed_slice(); // Similar to calloc.
        Self { buckets }
    }

    pub fn get_bucket(&mut self, hash: u64) -> &mut TT_bucket {
        let index = (hash as usize) & TT_DIM_MINUS_1;

        // Safety: index is guaranteed to be within bounds by the mask.
        // We can use get_unchecked_mut for maximum speed in Release mode.
        unsafe { self.buckets.get_unchecked_mut(index) }
    }
}
