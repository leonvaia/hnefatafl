use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;

const BOARD_SIZE: usize = 7;
const PIECE_TYPES: usize = 3; // B, W, K

#[derive(Clone)]
pub struct Zobrist {
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

    #[inline]
    pub(crate) fn piece_index(piece: char) -> Option<usize> {
        match piece {
            'B' => Some(0),
            'W' => Some(1),
            'K' => Some(2),
            _ => None,
        }
    }
}