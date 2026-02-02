//! Transposition table data structure.

use std::mem;

/// Note: If we change TT_DIM const to not be a power of 2,
/// then we need to change the unsafe code in get_bucket().
const TT_DIM_BITS: u64 = 24; // When decreasing this under 24, modify also bit layout.
const TT_DIM: usize = 1 << TT_DIM_BITS;
const TT_DIM_MINUS_1: usize = TT_DIM - 1;

/// ===================
///        Entry     
/// ===================
/// Bit layout:
/// hash:       u40 = 64 bit - TT_DIM bit
/// generation: u15 / u13
/// n_visits:   u36 / u37
/// n_wins:     i37 / i38
/// total:      128 bit = 16 byte
/// Note: The above layouts make sense when generation is updated
/// at each call of get_move() instead of at each call of start_search().
const HASH_BITS: u32 = 40;
const GEN_BITS: u32 = 13;
const VISITS_BITS: u32 = 37;
pub const WINS_BITS: u32 = 38;
/// To be sure the program doesn't get to an overflow, we force the following condition:
/// 2^VISITS_BITS > 2^GEN_BITS * iterations_per_move
const MAX_ITER_BITS: u32 = VISITS_BITS - GEN_BITS;
pub const MAX_ITER: u32 = 1u32 << MAX_ITER_BITS; // used in mcts.rs
/// Offsets.
const HASH_OFFSET: u32 = 0;
const GEN_OFFSET: u32 = HASH_OFFSET + HASH_BITS;
const VISITS_OFFSET: u32 = GEN_OFFSET + GEN_BITS;
const WINS_OFFSET: u32 = VISITS_OFFSET + VISITS_BITS;
// Masks.
const HASH_MASK: u128 = ((1u128 << HASH_BITS) - 1) << HASH_OFFSET;
const GEN_MASK: u128 = ((1u128 << GEN_BITS) - 1) << GEN_OFFSET;
const VISITS_MASK: u128 = ((1u128 << VISITS_BITS) - 1) << VISITS_OFFSET;
const WINS_MASK: u128 = ((1u128 << WINS_BITS) - 1) << WINS_OFFSET;

/// Entry containing data about a game state packed into 128 bits.
/// We use: repr(C) to ensure memory layout is predictable.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TT_entry {
    data: u128,
}

impl Default for TT_entry {
    fn default() -> Self {
        // Note: the following code is safe until all attributes of the
        // structure are integers and floats. For booleans it might be
        // dangerous. For pointers it causes a CRASH.
        unsafe { mem::zeroed() }
    }
}

/// Values returned by add_entry do keep track of collisions.
pub enum CollisionType {
    // Entry was overwritten inside the range: BAD collision.
    OverwrittenIN,
    // Entry was overwritten outside the range.
    OverwrittenOUT,
    // Empty entry, no overwritten entry.
    EmptyEntry,
    // Entry already existed (no write performed).
    Found,
}

impl TT_entry {
    /// ================================
    ///            Getters
    /// ================================
    /// Check whether the entry is empty.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.data == 0
    }

    /// Check whether a hash corresponds to an entry.
    /// We verify the upper 40 bits of the hash (since the lower 24 form the index).
    #[inline]
    pub fn hash_equals(&self, hash: u64) -> bool {
        // Extract the stored hash part from our data
        let stored_part = (self.data & HASH_MASK) as u64;
        
        // Calculate the verification part from the query hash.
        // We shift right by TT_DIM_BITS to get the upper 40 bits.
        let query_part = hash >> TT_DIM_BITS;
        
        stored_part == query_part
    }

    #[inline]
    pub fn get_generation(&self) -> u32 {
        ((self.data & GEN_MASK) >> GEN_OFFSET) as u32
    }

    #[inline]
    pub fn get_n_visits(&self) -> usize {
        ((self.data & VISITS_MASK) >> VISITS_OFFSET) as usize
    }

    #[inline]
    pub fn get_n_wins(&self) -> isize {
        // Extract raw unsigned bits
        let raw = ((self.data & WINS_MASK) >> WINS_OFFSET) as u64;
        
        // Sign extension magic:
        // We shift the 37-bit number to the top of the 64-bit container, 
        // then shift back down to drag the sign bit across.        const SHIFT_AMOUNT: u32 = 64 - WINS_BITS;
        const SHIFT_AMOUNT: u32 = 64 - WINS_BITS;
        let extended = (raw as i64) << SHIFT_AMOUNT >> SHIFT_AMOUNT;
        
        extended as isize
    }

    /// =================================
    ///            Setters
    /// =================================
    #[inline]
    pub fn set_hash(&mut self, hash: u64) {
        // Clear the old hash bits
        self.data &= !HASH_MASK;
        
        // Take upper bits of the input hash and place them in the low bits of u128
        let part = (hash >> TT_DIM_BITS) as u128;
        
        // OR them in
        self.data |= part & HASH_MASK;
    }

    #[inline]
    pub fn set_generation(&mut self, generation: u32) {
        self.data &= !GEN_MASK;
        self.data |= (generation as u128) << GEN_OFFSET;
    }

    #[inline]
    pub fn set_n_visits(&mut self, value: usize) {
        // Safety: Mask the input to ensure we don't overflow into 'n_wins'
        // If value > 268 million, this will wrap/truncate.
        let val_clamped = (value as u128) << VISITS_OFFSET;
        
        self.data &= !VISITS_MASK;
        self.data |= val_clamped & VISITS_MASK;
    }

    #[inline]
    pub fn set_n_wins(&mut self, value: isize) {
        // Cast isize to u128 directly (handles two's complement bits correctly)
        let val_encoded = (value as u128) << WINS_OFFSET;
        
        self.data &= !WINS_MASK;
        self.data |= val_encoded & WINS_MASK;
    }

    #[inline]
    pub fn add_n_visits(&mut self, increase: usize) {
        let current = self.get_n_visits();
        self.set_n_visits(current + increase);
    }

    #[inline]
    pub fn add_n_wins(&mut self, increase: isize) {
        let current = self.get_n_wins();
        self.set_n_wins(current + increase);
    }
}

/// ==================
///       Bucket 
/// ==================
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
    #[inline]
    pub fn get_entry(&mut self, hash: u64) -> Option<&mut TT_entry> {
        for entry in &mut self.entries {
            if !entry.is_empty() && entry.hash_equals(hash) {
                return Some(entry); // Found entry.
            }
        }
        None // Not found entry.
    }

    /// Overwrite entry corresponding to the index.
    #[inline]
    fn overwrite(&mut self, index: usize, hash: u64, generation: u32, visits: usize, wins: isize) {
        self.entries[index].set_hash(hash);
        self.entries[index].set_generation(generation);
        self.entries[index].set_n_visits(visits);
        self.entries[index].set_n_wins(wins);
    }

    /// =====================
    ///     MCTS EXPANSION
    /// =====================
    /// Look for the entry in the bucket.
    /// If found, do nothing.
    /// If not found, add it with zero values; overwrite according to collision handling policy:
    /// overwrite the least visited entry among the entries outside the generation range.
    pub fn add_entry(&mut self, hash: u64, generation: u32, generation_bound: u32) -> Option<CollisionType> {
        let mut min_visits = usize::MAX;
        let mut min_index = usize::MAX;

        // Find least visited entry within generation_range.
        for (index, entry) in (&mut self.entries).into_iter().enumerate() {
            // if entry already exists.
            if !entry.is_empty() && entry.hash_equals(hash) {
                entry.set_generation(generation);
                return Some(CollisionType::Found);
            }
            // If empty entry.
            if entry.is_empty() {
                entry.set_hash(hash);
                entry.set_generation(generation);
                entry.set_n_visits(0);
                entry.set_n_wins(0);
                return Some(CollisionType::EmptyEntry);
            }
            // If found entry outside the generation range.
            if entry.get_generation() < generation_bound
            && entry.get_n_visits() < min_visits {
                min_visits = entry.get_n_visits();
                min_index = index;
            }
        }

        // If not found i.e. bucket is full within range => BAD collision
        if min_visits == usize::MAX {
            // Find least visited entry in range.
            for (index, entry) in (&mut self.entries).into_iter().enumerate() {
                if entry.get_n_visits() < min_visits {
                    min_visits = entry.get_n_visits();
                    min_index = index;
                }
            }
            // Overwrite it.
            self.overwrite(min_index, hash, generation, 0, 0);
            return Some(CollisionType::OverwrittenIN);
        }

        // Overwrite the least visited entry out of the generation_range (GOOD collision).
        self.overwrite(min_index, hash, generation, 0, 0);
        return Some(CollisionType::OverwrittenOUT);
    }
}

/// ===========================
///     Transposition table
/// ===========================
pub struct TT {
    pub buckets: Box<[TT_bucket]>,
}

impl Default for TT {
    fn default() -> Self {
        Self::new()
    }
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
