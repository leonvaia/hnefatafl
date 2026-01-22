/// Zobrist hashing.

use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;

// Padded to 4 (power of 2) to ensure 32-byte alignment per cell,
// preventing cache line splits and allowing bit-shift indexing.
const PIECE_TYPES: usize = 4; // B, W, K, padding.
const BOARD_SIZE: usize = 7;

#[derive(Clone)]
pub struct Zobrist {
    // 7 * 7 * 4 * 8 bytes = 1568 bytes (Fits in L1 Cache)
    pub table: [[[u64; PIECE_TYPES]; BOARD_SIZE]; BOARD_SIZE],
    pub black_to_move: u64,
}

impl Zobrist {
    pub fn new(seed: u64) -> Self {
        let mut rng = StdRng::seed_from_u64(seed);

        let mut table = [[[0u64; PIECE_TYPES]; BOARD_SIZE]; BOARD_SIZE];
        for r in 0..BOARD_SIZE {
            for c in 0..BOARD_SIZE {
                for p in 0..PIECE_TYPES {
                    table[r][c][p] = rng.random::<u64>();
                }
            }
        }

        Self {
            table,
            black_to_move: rng.random::<u64>(),
        }
    }
}