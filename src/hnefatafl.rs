/// Game State and rules implementation.
/// The moves are encoded as an array: coords = [start_row, start_col, end_row, end_col]

use std::io::{self, Write};
use crate::zobrist::Zobrist;

/// The maximum number of plies for a game.
/// Used to implement Rule 8 (Perpetual repetitions result in a loss for white).
const MAX_GAME_LENGTH: usize = 512;

/// BITBOARD constants.
const BOARD_SIZE: usize = 7;
const TOTAL_SQUARES: usize = BOARD_SIZE * BOARD_SIZE;
/// Masks (for checking the board positions).
/// Corners:
const CORNERS: u64 = (1 << 0) | (1 << 6) | (1 << 42) | (1 << 48);
/// Throne:
const THRONE: u64 = 1 << 24;
/// Restricted squares (Corners + Throne)
const RESTRICTED: u64 = CORNERS | THRONE;
/// Edges:
const ROW_0_MASK: u64 = 0x7F;
const ROW_6_MASK: u64 = 0x7F << 42;
const COL_0_MASK: u64 = (1<<0)|(1<<7)|(1<<14)|(1<<21)|(1<<28)|(1<<35)|(1<<42);
const COL_6_MASK: u64 = COL_0_MASK << 6;

/// Board representation used in history.
/// (black_mask, white_mask, king_mask)
type BoardSnaphot = (u64, u64, u64, usize);

#[derive(Clone, Copy)]
pub struct GameState {
    /// Bitboards: 1 means piece is present, 0 means empty.
    pub black_pieces: u64,
    pub white_pieces: u64, // only white pawns
    pub king_piece: u64, // only the king

    pub player: char,
    pub hash: u64,

    pub ply_count: usize,

    // History for Rule 8 (perpetual repetitions).
    history: [BoardSnaphot; MAX_GAME_LENGTH],
    history_len: usize,
    pub repetition: bool,
    pub repetition_dist: Option<usize>,
}

impl GameState {
    pub fn new(z_table: &Zobrist) -> Self {
        // Initial setup:
        // let initial_board = [
        //     ['.', '.', '.', 'B', '.', '.', '.'],
        //     ['.', '.', '.', 'B', '.', '.', '.'],
        //     ['.', '.', '.', 'W', '.', '.', '.'],
        //     ['B', 'B', 'W', 'K', 'W', 'B', 'B'],
        //     ['.', '.', '.', 'W', '.', '.', '.'],
        //     ['.', '.', '.', 'B', '.', '.', '.'],
        //     ['.', '.', '.', 'B', '.', '.', '.'],
        // ];
        
        // Bitboard.
        let mut black = 0u64;
        let mut white = 0u64;
        let mut king = 0u64;

        let set = |b: &mut u64, r, c| *b |= 1u64 << (r * 7 + c);

        set(&mut black, 0, 3); set(&mut black, 1, 3);
        set(&mut black, 3, 0); set(&mut black, 3, 1); set(&mut black, 3, 5); set(&mut black, 3, 6);
        set(&mut black, 5, 3); set(&mut black, 6, 3);

        set(&mut white, 2, 3); 
        set(&mut white, 3, 2); set(&mut white, 3, 4);
        set(&mut white, 4, 3);

        set(&mut king, 3, 3);

        // Hash.
        let mut hash = 0u64;
        for i in 0..49 {
            let r = i / 7;
            let c = i % 7;
            if (black >> i) & 1 == 1 { hash ^= z_table.table[r][c][0]; }
            if (white >> i) & 1 == 1 { hash ^= z_table.table[r][c][1]; }
            if (king >> i) & 1 == 1  { hash ^= z_table.table[r][c][2]; }
        }
        hash ^= z_table.black_to_move;

        // History.
        let initial_snapshot = (black, white, king, 0);
        let mut history = [(0,0,0,0); MAX_GAME_LENGTH];
        history[0] = initial_snapshot;

        Self {
            black_pieces: black,
            white_pieces: white,
            king_piece: king,
            player: 'B',
            hash,
            ply_count: 0,
            history,
            history_len: 1,
            repetition: false,
            repetition_dist: None,
        }
    }

    /// Display game board in ASCII art.
    // Inside your impl GameState
    pub fn display<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writeln!(writer, "  0 1 2 3 4 5 6")?;
        for r in 0..7 {
            write!(writer, "{}", r)?;
            for c in 0..7 {
                let mask = 1u64 << (r * 7 + c);
                if (self.black_pieces & mask) != 0 { write!(writer, " B")?; }
                else if (self.white_pieces & mask) != 0 { write!(writer, " W")?; }
                else if (self.king_piece & mask) != 0 { write!(writer, " K")?; }
                else { write!(writer, " .")?; }
            }
            writeln!(writer)?;
        }
        Ok(())
    }

    /// Helpeer to get bit index.
    #[inline(always)]
    fn idx(r: usize, c: usize) -> usize {
        r * 7 + c
    }

    /// ===================
    ///      NEXT HASH
    /// ===================

    /// Compute the hash of a move without applying it.
    /// Used by MCTS for lookups in transpositions table.
    #[inline]
    pub fn next_hash(&self, coords: &[usize; 4], z_table: &Zobrist) -> u64 {
        let (sr, sc, er, ec) = (coords[0], coords[1], coords[2], coords[3]);
        let src_mask = 1 << Self::idx(sr, sc);
        let dst_mask = 1 << Self::idx(er, ec);
        let move_mask = src_mask | dst_mask;
        
        let mut h = self.hash;

        // Identify piece.
        let p_idx = if (self.black_pieces & src_mask) != 0 { 0 }
                    else if (self.white_pieces & src_mask) != 0 { 1 }
                    else if (self.king_piece & src_mask) != 0 { 2 }
                    // Safety check (though engine shouldn't pass empty squares).
                    else { return self.hash; };

        // Update hash for the move itself (Remove from start, add to end).
        h ^= z_table.table[sr][sc][p_idx];
        h ^= z_table.table[er][ec][p_idx];
        
        // Update player turn.
        h ^= z_table.black_to_move;

        // === Calculate captures ===
        // We need to simulate the board state *after* the move to check captures.

        let mut sim_black = self.black_pieces;
        let mut sim_white = self.white_pieces;
        let mut sim_king = self.king_piece;

        // Move the bits in simulation
        if p_idx == 0 { sim_black ^= move_mask; }
        else if p_idx == 1 { sim_white ^= move_mask; }
        else { sim_king ^= move_mask; }

        let mover_is_black = p_idx == 0;
        let dst_idx = Self::idx(er, ec);

        // Check the four neighbors.
        let neighbors = self.get_orthogonal_neighbors(dst_idx);
        for &victim_idx in &neighbors {
            // Check if this neighbor is an enemy.
            let is_victim_black = (sim_black & (1 << victim_idx)) != 0;
            let is_victim_white = (sim_white & (1 << victim_idx)) != 0;
            let is_victim_king = (sim_king & (1 << victim_idx)) != 0;
            if !is_victim_black && !is_victim_white && !is_victim_king { continue; }
            // Define Enemy/Friend based on Mover
            let is_enemy = if mover_is_black { is_victim_white || is_victim_king } 
                           else { is_victim_black };
            if !is_enemy { continue; }

            // King capture.
            if is_victim_king {
                if self.check_king_captured_sim(sim_black, sim_king) {
                    h ^= z_table.table[victim_idx/7][victim_idx%7][2];
                }
                continue;
            }

            // Pawn captures.
            if let Some(anvil_idx) = self.get_anvil_index(dst_idx, victim_idx) {
                // Check if Anvil is hostile to the victim
                if self.is_hostile_sim(anvil_idx, is_victim_black, sim_black, sim_white, sim_king) {
                    // Capture!
                    let v_r = victim_idx / 7;
                    let v_c = victim_idx % 7;
                    let v_pidx = if is_victim_black { 0 } else { 1 };
                    h ^= z_table.table[v_r][v_c][v_pidx];
                }
            }
        }

        h
    }

    /// Get neighbors as indices (up to four).
    #[inline]
    fn get_orthogonal_neighbors(&self, idx: usize) -> Vec<usize> {
        let mut n = Vec::with_capacity(4);
        let r = idx / 7;
        let c = idx % 7;
        
        if r > 0 { n.push(idx - 7); } // North
        if r < 6 { n.push(idx + 7); } // South
        if c > 0 { n.push(idx - 1); } // West
        if c < 6 { n.push(idx + 1); } // East
        
        n
    }

    /// Helper for King Capture in simulation.
    fn check_king_captured_sim(&self, black: u64, king: u64) -> bool {
        let k_idx = king.trailing_zeros() as usize; // Get king index
        if k_idx >= 64 { return false; } // Should not happen if king exists

        let neighbors = self.get_orthogonal_neighbors(k_idx);
        
        // If on Throne (24), needs 4 attackers
        if k_idx == 24 {
            if neighbors.len() < 4 { return false; } // Should be 4
            for n in neighbors {
                if (black & (1 << n)) == 0 { return false; }
            }
            return true;
        }

        // If next to Throne (Right, Left, Up, Down of 24)
        // 25, 23, 17, 31
        let is_next_throne = k_idx == 23 || k_idx == 25 || k_idx == 17 || k_idx == 31;
        if is_next_throne {
            // Needs 3 attackers + Throne acting as anvil
            for n in neighbors {
                // If neighbor is throne, it counts as hostile
                if n == 24 { continue; }
                // Otherwise needs black piece
                if (black & (1 << n)) == 0 { return false; }
            }
            return true;
        }

        // Standard capture (2 sides)
        // Check horizontal pair
        let r = k_idx / 7;
        let c = k_idx % 7;
        
        // Check Horizontal (West/East)
        if c > 0 && c < 6 {
            let w = k_idx - 1;
            let e = k_idx + 1;
            let w_hostile = ((black & (1<<w)) != 0) || ((CORNERS & (1<<w)) != 0);
            let e_hostile = ((black & (1<<e)) != 0) || ((CORNERS & (1<<e)) != 0);
            if w_hostile && e_hostile { return true; }
        }

        // Check Vertical (North/South)
        if r > 0 && r < 6 {
            let n = k_idx - 7;
            let s = k_idx + 7;
            let n_hostile = ((black & (1<<n)) != 0) || ((CORNERS & (1<<n)) != 0);
            let s_hostile = ((black & (1<<s)) != 0) || ((CORNERS & (1<<s)) != 0);
            if n_hostile && s_hostile { return true; }
        }

        false
    }

    /// Get the index "behind" the victim from the perspective of source.
    /// Src -> Victim -> Anvil
    #[inline]
    fn get_anvil_index(&self, src: usize, victim: usize) -> Option<usize> {
        let diff = victim as isize - src as isize;
        // diff is -7, +7, -1, or +1
        let anvil = victim as isize + diff;
        
        // Bounds check
        if anvil < 0 || anvil >= 49 { return None; }
        
        // Check row wrapping for horizontal moves
        let v_c = victim % 7;
        let a_c = anvil as usize % 7;
        
        // If moving horizontal (diff 1 or -1), col distance must be 1
        if diff.abs() == 1 && (v_c as isize - a_c as isize).abs() != 1 {
            return None;
        }

        Some(anvil as usize)
    }

    /// Check if a square is hostile to a victim (Simulated version for next_hash/move)
    #[inline]
    fn is_hostile_sim(&self, idx: usize, victim_is_black: bool, b: u64, w: u64, k: u64) -> bool {
        let mask = 1 << idx;
        let occupied_black = (b & mask) != 0;
        let occupied_white = (w & mask) != 0;
        let occupied_king = (k & mask) != 0;
        
        // 1. Piece Hostility
        if victim_is_black {
            // Hostile if occupied by White or King
            if occupied_white || occupied_king { return true; }
            // Or if it's a corner
            if (CORNERS & mask) != 0 { return true; }
            // Or Throne (always hostile to black)
            if (THRONE & mask) != 0 { return true; }
        } else {
            // Victim is White
            // Hostile if occupied by Black
            if occupied_black { return true; }
            // Corners hostile to everyone
            if (CORNERS & mask) != 0 { return true; }
            // Throne hostile to white ONLY if empty (King left it)
            if (THRONE & mask) != 0 && !occupied_king { return true; }
        }
        false
    }

    /// ========================
    ///      MOVE EXECUTION
    /// ========================

    /// Move piece on the board and update hash and history.
    /// The logic assumes the move to be legal.
    /// coords = [start_row, start_col, end_row, end_col]
    #[inline]
    pub fn move_piece<W: Write>(&mut self, coords: &[usize; 4], z_table: &Zobrist, is_sim_move: bool, writer: &mut W) {
        let (sr, sc, er, ec) = (coords[0], coords[1], coords[2], coords[3]);
        let src_mask = 1 << Self::idx(sr, sc);
        let dst_mask = 1 << Self::idx(er, ec);
        let move_mask = src_mask | dst_mask;

        // Update ply count.
        self.ply_count += 1;

        // Update board.
        let mut p_idx = 0; // 0:B, 1:W, 2:K
        if (self.black_pieces & src_mask) != 0 {
            self.black_pieces ^= move_mask;
            p_idx = 0;
        } else if (self.white_pieces & src_mask) != 0 {
            self.white_pieces ^= move_mask;
            p_idx = 1;
        } else if (self.king_piece & src_mask) != 0 {
            self.king_piece ^= move_mask;
            p_idx = 2;
        }

        // Update hash.
        self.hash ^= z_table.table[sr][sc][p_idx];
        self.hash ^= z_table.table[er][ec][p_idx];
        self.hash ^= z_table.black_to_move;
        
        // Apply captures.
        self.apply_captures_bits(er, ec, p_idx, z_table, is_sim_move, writer);

        // Update history (Sorted Insert).
        let current_state_key = (self.black_pieces, self.white_pieces, self.king_piece);
        if self.history_len < MAX_GAME_LENGTH {
            let slice = &self.history[0..self.history_len];
            let res = slice.binary_search_by(|entry| {
                entry.0.cmp(&current_state_key.0)
                    .then(entry.1.cmp(&current_state_key.1))
                    .then(entry.2.cmp(&current_state_key.2))
            });

            match res {
                Ok(idx) => {
                    self.repetition = true;
                    // Retrieve when this state occurred.
                    let old_ply = self.history[idx].3;
                    self.repetition_dist = Some(self.ply_count - old_ply);
                }
                Err(idx) => {
                    self.repetition = false;
                    self.repetition_dist = None;
                    // Shift history.
                    if idx < self.history_len {
                        self.history.copy_within(idx..self.history_len, idx + 1);
                    }
                    // Insert new state.
                    self.history[idx] = (self.black_pieces, self.white_pieces, self.king_piece, self.ply_count);
                    self.history_len += 1;
                }
            }
        }
        
        // Update player.
        self.player = if self.player == 'B' { 'W' } else { 'B' };
    }

    /// Apply all captures.
    #[inline]
    fn apply_captures_bits<W: Write>(&mut self, r: usize, c: usize, mover_type: usize, z_table: &Zobrist, is_sim_move: bool, writer: &mut W) {
        // mover_type: 0=B, 1=W, 2=K
        let dst_idx = Self::idx(r, c);

        // Check the four neighbors.
        let neighbors = self.get_orthogonal_neighbors(dst_idx);
        for &v_idx in &neighbors {
             let mask = 1 << v_idx;
             let is_b = (self.black_pieces & mask) != 0;
             let is_w = (self.white_pieces & mask) != 0;
             let is_k = (self.king_piece & mask) != 0;

             if !is_b && !is_w && !is_k { continue; }

             // Determine if enemy.
             let is_enemy = if mover_type == 0 { is_w || is_k } else { is_b };
             if !is_enemy { continue; }

             // King Capture.
             if is_k {
                 if self.check_king_captured_sim(self.black_pieces, self.king_piece) {
                     self.king_piece &= !mask; // Remove King from board.
                     self.hash ^= z_table.table[v_idx/7][v_idx%7][2];
                     if !is_sim_move { writeln!(writer, "King got captured").expect("could not write to output"); }
                 }
                 continue;
             }

             // Pawn Captures.
             if let Some(anvil_idx) = self.get_anvil_index(dst_idx, v_idx) {
                 if self.is_hostile_sim(anvil_idx, is_b, self.black_pieces, self.white_pieces, self.king_piece) {
                     // Remove Piece and update hash.
                     if is_b { 
                         self.black_pieces &= !mask; 
                         self.hash ^= z_table.table[v_idx/7][v_idx%7][0];
                         if !is_sim_move { writeln!(writer, "Black piece got captured").expect("could not write to output"); }
                     } else { 
                         self.white_pieces &= !mask;
                         self.hash ^= z_table.table[v_idx/7][v_idx%7][1];
                         if !is_sim_move { writeln!(writer, "White piece got captured").expect("could not write to output"); }
                     }
                 }
             }
        }
    }

    /// =========================
    ///      MOVE GENERATION
    /// =========================
    
    /// Return true if the given player has at least one legal move.
    /// Function called only by check_game_over()
    fn has_legal_move(&self, player: char) -> bool {
        let occupied = self.black_pieces | self.white_pieces | self.king_piece;
        
        let my_pieces = if player == 'B' { self.black_pieces } 
                        else { self.white_pieces | self.king_piece };

        for i in 0..TOTAL_SQUARES {
            // If I have a piece at i
            if (my_pieces & (1 << i)) != 0 {
                let r = i / 7;
                let c = i % 7;
                
                // Try directions
                // UP
                for rr in (0..r).rev() {
                    let dest = Self::idx(rr, c);
                    if (occupied & (1 << dest)) != 0 { break; } // Blocked
                    if !self.is_restricted_violation(rr, c, i) { return true; }
                }
                // DOWN
                for rr in r+1..7 {
                    let dest = Self::idx(rr, c);
                    if (occupied & (1 << dest)) != 0 { break; }
                    if !self.is_restricted_violation(rr, c, i) { return true; }
                }
                // LEFT
                for cc in (0..c).rev() {
                    let dest = Self::idx(r, cc);
                    if (occupied & (1 << dest)) != 0 { break; }
                    if !self.is_restricted_violation(r, cc, i) { return true; }
                }
                // RIGHT
                for cc in c+1..7 {
                    let dest = Self::idx(r, cc);
                    if (occupied & (1 << dest)) != 0 { break; }
                    if !self.is_restricted_violation(r, cc, i) { return true; }
                }
            }
        }
        false
    }

    /// Modify in place the vector of legal moves from the current state.
    /// Avoids allocating a vector each time (the function is called multiple times during Simulation).
    /// Algorithm from has_legal_move() modified to guarantee that indices are usize (and avoid casting).
    /// If no_repetition is true and player is White, avoids moves that cause history repetition.
    pub fn get_legal_moves(&self, moves: &mut Vec<[usize; 4]>, no_repetition: bool) {
        moves.clear();
        let occupied = self.black_pieces | self.white_pieces | self.king_piece;
        let my_pieces = if self.player == 'B' { self.black_pieces } 
                        else { self.white_pieces | self.king_piece };

        let filter_repetition = no_repetition && self.player == 'W';

        // Function used below.
        let mut add_move = |r, c, er, ec| {
            // Repetition check logic.
            if filter_repetition {
                let coords = [r, c, er, ec];
                let (nb, nw, nk) = self.predict_next_boards(&coords);
                
                // Binary search in history.
                let target_key = (nb, nw, nk);
                let slice = &self.history[0..self.history_len];
                let found = slice.binary_search_by(|entry| {
                    entry.0.cmp(&target_key.0)
                        .then(entry.1.cmp(&target_key.1))
                        .then(entry.2.cmp(&target_key.2))
                }).is_ok();

                // If the move is a repetition, we skip it.
                if found { return; }
            }

            moves.push([r, c, er, ec]);
        };

        for i in 0..TOTAL_SQUARES {
            if (my_pieces & (1 << i)) != 0 {
                let r = i / 7;
                let c = i % 7;
                
                // UP
                if r > 0 {
                    for rr in (0..r).rev() {
                        let dest = Self::idx(rr, c);
                        if (occupied & (1 << dest)) != 0 { break; } 
                        if !self.is_restricted_violation(rr, c, i) { add_move(r, c, rr, c); }
                    }
                }
                // DOWN
                if r < 6 {
                    for rr in r+1..7 {
                        let dest = Self::idx(rr, c);
                        if (occupied & (1 << dest)) != 0 { break; } 
                        if !self.is_restricted_violation(rr, c, i) { add_move(r, c, rr, c); }
                    }
                }
                // LEFT
                if c > 0 {
                    for cc in (0..c).rev() {
                        let dest = Self::idx(r, cc);
                        if (occupied & (1 << dest)) != 0 { break; } 
                        if !self.is_restricted_violation(r, cc, i) { add_move(r, c, r, cc); }
                    }
                }
                // RIGHT
                if c < 6 {
                    for cc in c+1..7 {
                        let dest = Self::idx(r, cc);
                        if (occupied & (1 << dest)) != 0 { break; } 
                        if !self.is_restricted_violation(r, cc, i) { add_move(r, c, r, cc); }
                    }
                }
            }
        }
    }

    /// Checks whether a piece different than the king is entering a restricted square.
    #[inline]
    fn is_restricted_violation(&self, r: usize, c: usize, src_idx: usize) -> bool {
        let dest_mask = 1 << Self::idx(r, c);
        // If dest is not restricted, it's fine
        if (RESTRICTED & dest_mask) == 0 { return false; }
        
        // If dest IS restricted, only King can go there.
        // Check if the piece at src_idx is the King.
        if (self.king_piece & (1 << src_idx)) != 0 { return false; }
        
        true
    }

    /// Simulate what the boards would look like after a move (B, W, K).
    /// Used for checking repetitions without mutating state.
    fn predict_next_boards(&self, coords: &[usize; 4]) -> (u64, u64, u64) {
        let (sr, sc, er, ec) = (coords[0], coords[1], coords[2], coords[3]);
        let src_mask = 1 << Self::idx(sr, sc);
        let dst_mask = 1 << Self::idx(er, ec);
        let move_mask = src_mask | dst_mask;

        let mut next_black = self.black_pieces;
        let mut next_white = self.white_pieces;
        let mut next_king = self.king_piece;

        // 1. Move the piece
        let mut p_idx = 0; // 0:B, 1:W, 2:K
        if (self.black_pieces & src_mask) != 0 {
            next_black ^= move_mask;
            p_idx = 0;
        } else if (self.white_pieces & src_mask) != 0 {
            next_white ^= move_mask;
            p_idx = 1;
        } else if (self.king_piece & src_mask) != 0 {
            next_king ^= move_mask;
            p_idx = 2;
        }

        let mover_is_black = p_idx == 0;
        let dst_idx = Self::idx(er, ec);

        // 2. Apply Captures (Logic adapted from next_hash)
        let neighbors = self.get_orthogonal_neighbors(dst_idx);
        for &victim_idx in &neighbors {
            let v_mask = 1 << victim_idx;
            
            // Check if neighbor is occupied in the SIMULATED board
            let is_victim_black = (next_black & v_mask) != 0;
            let is_victim_white = (next_white & v_mask) != 0;
            let is_victim_king = (next_king & v_mask) != 0;

            if !is_victim_black && !is_victim_white && !is_victim_king { continue; }

            // Define Enemy based on Mover
            let is_enemy = if mover_is_black { is_victim_white || is_victim_king } 
                           else { is_victim_black };
            if !is_enemy { continue; }

            // King capture check
            if is_victim_king {
                if self.check_king_captured_sim(next_black, next_king) {
                    next_king &= !v_mask; // Remove King
                }
                continue;
            }

            // Pawn capture check
            if let Some(anvil_idx) = self.get_anvil_index(dst_idx, victim_idx) {
                if self.is_hostile_sim(anvil_idx, is_victim_black, next_black, next_white, next_king) {
                    // Remove victim
                    if is_victim_black { next_black &= !v_mask; }
                    else { next_white &= !v_mask; }
                }
            }
        }

        (next_black, next_white, next_king)
    }

    // ===============================
    //            GAME OVER
    // ===============================

    /// Check if game is over, given a state and a move.
    /// Returns:
    /// None - Game is not over
    /// W - White wins
    /// B - Black wins
    /// D - Draw
    pub fn check_game_over(&self) -> Option<char> {
        // === Check if King is at a corner => White wins ===
        if (self.king_piece & CORNERS) != 0 { return Some('W'); }

        // === Check if King is captured => Black wins ===
        // We rely on the fact that if the King was captured,
        // he was removed from the board in apply_captures.
        if self.king_piece == 0 { return Some('B'); }

        // === Rule 8: Repetition => Black wins (White loses) ===
        // We check if the current board exists previously in the history.
        if self.repetition { return Some('B'); }

        // === Rule 9: If the player to move has no legal move, he loses. ===
        if !self.has_legal_move(self.player) {
            let winner = if self.player == 'B' { 'W' } else { 'B' };
            return Some(winner);
        }

        // === Rule 10: Draw due to "impossible to end the game" / insufficient material ===
        if self.is_insufficient_material_draw() { return Some('D'); }

        None
    }
    /// Same as above, but prints the repetition distance.
    /// Used only for the actual game being played, for analysis purposes.
    pub fn check_game_over_log<W: Write>(&self, writer: &mut W) -> Option<char> {
        if (self.king_piece & CORNERS) != 0 { return Some('W'); }
        if self.king_piece == 0 { return Some('B'); }
        if self.repetition {
            if let Some(dist) = self.repetition_dist {
                // Only print if distance in full moves (plies / 2) is > 3.
                writeln!(writer, "Repetition detected! The state first occurred {} plies ago.", dist).expect("Could not write repetition message to buffer.");
            }
            return Some('B');
        }
        if !self.has_legal_move(self.player) {
            let winner = if self.player == 'B' { 'W' } else { 'B' };
            return Some(winner);
        }
        if self.is_insufficient_material_draw() { return Some('D'); }

        None
    }

    /// Simple heuristic for rule 10: declare draw if both sides have very few pieces left.
    /// Copenhagen: "If it is not possible to end the game, fx. because both sides have too few pieces left, it is a draw."
    /// This rule is intentionally vague; adjust DRAW_PIECE_THRESHOLD as desired.
    #[inline]
    fn is_insufficient_material_draw(&self) -> bool {
        // Count bits
        let attackers = self.black_pieces.count_ones();
        let defenders = self.white_pieces.count_ones();
        
        // Thresholds
        attackers <= 2 && defenders <= 1
    }

    // ======================================
    //            HEURISTICS BLACK           
    // ======================================

    /// Returns true if Black can capture the King immediately.
    pub fn heuristic_capture_king(&self) -> (bool, Option<[usize; 4]>) {
        // Black must be the player moving.
        if self.player != 'B' { return (false, None); }
        
        // Find King.
        let k_idx = self.king_piece.trailing_zeros() as usize;
        if k_idx >= 64 { return (false, None); } // Should not happen.

        // Get neighbors.
        let neighbors = self.get_orthogonal_neighbors(k_idx);
        let occupied = self.black_pieces | self.white_pieces | self.king_piece;

        // === CASE 1: King on Throne (24) ===
        // Needs 4 attackers. We need 3 present + 1 reachable empty spot.
        if k_idx == 24 {
            let mut black_count = 0;
            let mut empty_spot = None;

            for &n in &neighbors {
                if (self.black_pieces & (1 << n)) != 0 {
                    black_count += 1;
                } else if (occupied & (1 << n)) == 0 {
                    empty_spot = Some(n);
                }
            }

            if black_count == 3 {
                if let Some(target) = empty_spot {
                    if let Some(mv) = self.get_black_move_to(target) {
                        return (true, Some(mv));
                    }
                }
            }
            return (false, None);
        }

        // === CASE 2: King Next to Throne (23, 25, 17, 31) ===
        // Needs 3 attackers + Throne (which acts as the 4th anvil).
        // We need 2 present (excluding throne) + 1 reachable empty spot.
        let is_next_throne = k_idx == 23 || k_idx == 25 || k_idx == 17 || k_idx == 31;
        if is_next_throne {
            let mut black_count = 0;
            let mut empty_spot = None;

            for &n in &neighbors {
                if n == 24 { continue; } // Skip Throne (it's the anvil, not an attacker)
                
                if (self.black_pieces & (1 << n)) != 0 {
                    black_count += 1;
                } else if (occupied & (1 << n)) == 0 {
                    empty_spot = Some(n);
                }
            }

            if black_count == 2 {
                if let Some(target) = empty_spot {
                    if let Some(mv) = self.get_black_move_to(target) {
                        return (true, Some(mv));
                    }
                }
            }
            return (false, None);
        }

        // === CASE 3: Standard Capture (Sandwich) ===
        // Check Horizontal Pair
        let r = k_idx / 7;
        let c = k_idx % 7;

        // Check Horizontal (West/East)
        if c > 0 && c < 6 {
            let w = k_idx - 1;
            let e = k_idx + 1;
            // Check West as Killer, East as Anvil
            if self.is_hostile_anvil_for_heuristic(e) && (occupied & (1 << w)) == 0 {
                if let Some(mv) = self.get_black_move_to(w) { return (true, Some(mv)); }
            }
            // Check East as Killer, West as Anvil
            if self.is_hostile_anvil_for_heuristic(w) && (occupied & (1 << e)) == 0 {
                if let Some(mv) = self.get_black_move_to(e) { return (true, Some(mv)); }
            }
        }

        // Check Vertical (North/South)
        if r > 0 && r < 6 {
            let n = k_idx - 7;
            let s = k_idx + 7;
            // Check North as Killer, South as Anvil
            if self.is_hostile_anvil_for_heuristic(s) && (occupied & (1 << n)) == 0 {
                if let Some(mv) = self.get_black_move_to(n) { return (true, Some(mv)); }
            }
            // Check South as Killer, North as Anvil
            if self.is_hostile_anvil_for_heuristic(n) && (occupied & (1 << s)) == 0 {
                if let Some(mv) = self.get_black_move_to(s) { return (true, Some(mv)); }
            }
        }

        (false, None)
    }

    /// Helper: Checks if a square acts as an Anvil for Black (Black piece, Corner, or Throne).
    #[inline]
    fn is_hostile_anvil_for_heuristic(&self, idx: usize) -> bool {
        let mask = 1 << idx;
        if (self.black_pieces & mask) != 0 { return true; }
        if (CORNERS & mask) != 0 { return true; }
        if (THRONE & mask) != 0 { return true; } // Throne is hostile to King if empty (logic derived from game rules)
        false
    }

    /// Helper: Checks if ANY Black piece can legally move to `target_idx`.
    /// Returns the move [sr, sc, er, ec] if found.
    fn get_black_move_to(&self, target_idx: usize) -> Option<[usize; 4]> {
        // Black cannot capture by landing ON a restricted square (Throne/Corners).
        if (RESTRICTED & (1 << target_idx)) != 0 { return None; }

        let r = target_idx / 7;
        let c = target_idx % 7;
        let occupied = self.black_pieces | self.white_pieces | self.king_piece;

        // Scan OUTWARDS from target to find a source piece.
        // We stop at the first occupied square. If it's Black, we found a move.
        // (Note: In your engine, pieces *can* slide through empty restricted squares, so we only break on occupied).

        // UP
        for rr in (0..r).rev() {
            let curr = Self::idx(rr, c);
            if (occupied & (1 << curr)) != 0 {
                if (self.black_pieces & (1 << curr)) != 0 { return Some([rr, c, r, c]); }
                break; // Blocked by White/King
            }
        }
        // DOWN
        for rr in r+1..7 {
            let curr = Self::idx(rr, c);
            if (occupied & (1 << curr)) != 0 {
                if (self.black_pieces & (1 << curr)) != 0 { return Some([rr, c, r, c]); }
                break;
            }
        }
        // LEFT
        for cc in (0..c).rev() {
            let curr = Self::idx(r, cc);
            if (occupied & (1 << curr)) != 0 {
                if (self.black_pieces & (1 << curr)) != 0 { return Some([r, cc, r, c]); }
                break;
            }
        }
        // RIGHT
        for cc in c+1..7 {
            let curr = Self::idx(r, cc);
            if (occupied & (1 << curr)) != 0 {
                if (self.black_pieces & (1 << curr)) != 0 { return Some([r, cc, r, c]); }
                break;
            }
        }
        None
    }

    // =======================================
    //            HEURISTICS WHITE            
    // =======================================

    /// Returns true if from the current state white can win, whatever black does.
    pub fn heuristic_wins_w(&self) -> bool {
        // White must be the player moving.
        if self.player != 'W' { return false; }

        // 1. King to corner.
        if self.heuristic_king_to_corner().0 { return true; }

        // 2. King to empty edge.
        if self.heuristic_king_empty_edge().0 { return true; }

        false
    }

    /// 1. The King has a clear path to a corner.
    pub fn heuristic_king_to_corner(&self) -> (bool, Option<[usize; 4]>) {
        let k_idx = self.king_piece.trailing_zeros() as usize;
        if k_idx >= 64 { return (false, None); }

        let r = k_idx / 7;
        let c = k_idx % 7;
        let occupied = self.black_pieces | self.white_pieces | self.king_piece;

        // Helper to check linear path (exclusive of start, inclusive of end)
        let check_path = |start_idx: usize, step: isize, count: usize| -> bool {
            let mut curr = start_idx as isize + step;
            for _ in 0..count {
                if (occupied & (1u64 << curr)) != 0 { return false; }
                curr += step;
            }
            true
        };

        // 1. Top-Left (0,0)
        // Check West (if on row 0)
        if r == 0 && c > 0 {
            if check_path(k_idx, -1, c) { return (true, Some([r, c, 0, 0])); }
        }
        // Check North (if on col 0)
        if c == 0 && r > 0 {
            if check_path(k_idx, -7, r) { return (true, Some([r, c, 0, 0])); }
        }

        // 2. Top-Right (0,6)
        // Check East (if on row 0)
        if r == 0 && c < 6 {
            if check_path(k_idx, 1, 6 - c) { return (true, Some([r, c, 0, 6])); }
        }
        // Check North (if on col 6)
        if c == 6 && r > 0 {
            if check_path(k_idx, -7, r) { return (true, Some([r, c, 0, 6])); }
        }

        // 3. Bottom-Left (6,0)
        // Check West (if on row 6)
        if r == 6 && c > 0 {
            if check_path(k_idx, -1, c) { return (true, Some([r, c, 6, 0])); }
        }
        // Check South (if on col 0)
        if c == 0 && r < 6 {
            if check_path(k_idx, 7, 6 - r) { return (true, Some([r, c, 6, 0])); }
        }

        // 4. Bottom-Right (6,6)
        // Check East (if on row 6)
        if r == 6 && c < 6 {
            if check_path(k_idx, 1, 6 - c) { return (true, Some([r, c, 6, 6])); }
        }
        // Check South (if on col 6)
        if c == 6 && r < 6 {
            if check_path(k_idx, 7, 6 - r) { return (true, Some([r, c, 6, 6])); }
        }

        (false, None)
    }

    /// 2. The King has a clear path to an empty edge (cannot be protected by black anymore).
    pub fn heuristic_king_empty_edge(&self) -> (bool, Option<[usize; 4]>) {
        let k_idx = self.king_piece.trailing_zeros() as usize;
        if k_idx >= 64 { return (false, None); }

        let r = k_idx / 7;
        let c = k_idx % 7;
        let occupied = self.black_pieces | self.white_pieces | self.king_piece;

        let check_path = |start_idx: usize, step: isize, count: usize| -> bool {
            let mut curr = start_idx as isize + step;
            for _ in 0..count {
                if (occupied & (1u64 << curr)) != 0 { return false; }
                curr += step;
            }
            true
        };

        // 1. Top Edge (Row 0) -> Move to (0, c)
        if (occupied & ROW_0_MASK) == 0 {
            if check_path(k_idx, -7, r) { 
                return (true, Some([r, c, 0, c])); 
            }
        }

        // 2. Bottom Edge (Row 6) -> Move to (6, c)
        if (occupied & ROW_6_MASK) == 0 {
            if check_path(k_idx, 7, 6 - r) { 
                return (true, Some([r, c, 6, c])); 
            }
        }

        // 3. Left Edge (Col 0) -> Move to (r, 0)
        if (occupied & COL_0_MASK) == 0 {
            if check_path(k_idx, -1, c) { 
                return (true, Some([r, c, r, 0])); 
            }
        }

        // 4. Right Edge (Col 6) -> Move to (r, 6)
        if (occupied & COL_6_MASK) == 0 {
            if check_path(k_idx, 1, 6 - c) { 
                return (true, Some([r, c, r, 6])); 
            }
        }

        (false, None)
    }

    // =================================
    //            HUMAN INPUT
    // =================================

    /// Gets a move from CLI.
    /// If valid then moves the piece.
    pub fn human_move<W: Write>(&mut self, z_table: &Zobrist, writer: &mut W) {
        loop {
            writeln!(writer, "\nCurrent Player: {}", self.player).expect("could not write to output");
            write!(writer, "Enter move: ").expect("could not write to output");
            writer.flush().expect("Flush failed");
            // Get input string.
            io::stdout().flush().unwrap();
            let mut input = String::new();
            io::stdin().read_line(&mut input).expect("Failed to read line");

            // Create array of length 4
            let res: Result<[usize; 4], _> = input
                .split_whitespace()
                .filter_map(|s| s.parse().ok())
                .collect::<Vec<usize>>()
                .try_into();

            match res {
                Ok(coords) => {
                    // Check if the move is valid and do it.
                    if self.is_legal_move_human(&coords) {
                        self.move_piece(&coords, &z_table, false, writer);
                        return;
                    } else {
                        continue;
                    }
                }
                Err(_) => {
                    writeln!(writer, "Invalid input. Try again.\n").expect("could not write to output");
                    continue;
                }
            }
        }
    }

    /// Check if the given move (coords) is legal.
    /// Used only for user moves.
    #[inline]
    fn is_legal_move_human(&self, coords: &[usize; 4]) -> bool {
        let (sr, sc, er, ec) = (coords[0], coords[1], coords[2], coords[3]);

        if sr > 6 || sc > 6 || er > 6 || ec > 6 { return false; }

        // If start == end
        if sr == er && sc == ec { return false; }

        let src = Self::idx(sr, sc);
        let dst = Self::idx(er, ec);

        // Check piece ownership.
        let is_mine = if self.player == 'B' { (self.black_pieces & (1<<src)) != 0 }
                      else { ((self.white_pieces | self.king_piece) & (1<<src)) != 0 };
        if !is_mine { return false; }

        // Check destination empty.
        let occupied = self.black_pieces | self.white_pieces | self.king_piece;
        if (occupied & (1<<dst)) != 0 { return false; }

        // Check for straight-line movement.
        if sr != er && sc != ec { return false; } // Not orthogonal

        // Check if the movement goes through occupied squares.
        let (dr, dc) = (er as isize - sr as isize, ec as isize - sc as isize);
        let step_r = dr.signum();
        let step_c = dc.signum();

        let mut curr_r = sr as isize + step_r;
        let mut curr_c = sc as isize + step_c;
        while curr_r != er as isize || curr_c != ec as isize {
             let idx = Self::idx(curr_r as usize, curr_c as usize);
             if (occupied & (1<<idx)) != 0 { return false; } // Path blocked
             curr_r += step_r;
             curr_c += step_c;
        }

        // Restricted squares may only be occupied by the king.
        if self.is_restricted_violation(er, ec, src) { return false; }
        
        // println!("Valid move.\n");
        true
    }
}
