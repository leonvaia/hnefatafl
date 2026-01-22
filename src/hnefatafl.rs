/// Game State and rules implementation.
/// The moves are encoded as an array: coords = [start_row, start_col, end_row, end_col]

use std::io::{self, Write};
use crate::zobrist::Zobrist;

/// The maximum number of plies for a game.
/// Used to implement Rule 8 (Perpetual repetitions).
/// Note: For Rule 8, instead of making black win, we forbid
/// repetitions when white moves. Rule 9 makes the rule work anyways.
const MAX_GAME_LENGTH: usize = 512;

#[derive(Clone, Copy)]
pub struct GameState {
    pub board: [[char; 7]; 7],
    pub player: char,
    pub hash: u64,
    // Track king to avoid scanning board in check_game_over() to find it.
    king_pos: (usize, usize),
    // History for Rule 8.
    history: [u64; MAX_GAME_LENGTH],
    history_len: usize,
}

impl GameState {
    pub fn new(z_table: &Zobrist) -> Self {
        let initial_board = [
            ['.', '.', '.', 'B', '.', '.', '.'],
            ['.', '.', '.', 'B', '.', '.', '.'],
            ['.', '.', '.', 'W', '.', '.', '.'],
            ['B', 'B', 'W', 'K', 'W', 'B', 'B'],
            ['.', '.', '.', 'W', '.', '.', '.'],
            ['.', '.', '.', 'B', '.', '.', '.'],
            ['.', '.', '.', 'B', '.', '.', '.'],
        ];

        // Compute the hash for the starting board.
        let mut hash = 0u64;
        for (r, row) in initial_board.iter().enumerate() {
            for (c, &piece_char) in row.iter().enumerate() {
                if let Some(p_idx) = Zobrist::piece_index(piece_char) {
                    hash ^= z_table.table[r][c][p_idx];
                }
            }
        }
        hash ^= z_table.black_to_move;

        // Initialize history array.
        let mut history = [0u64; MAX_GAME_LENGTH];
        history[0] = hash;

        Self {
            board: initial_board,
            player: 'B',
            hash,
            king_pos: (3, 3),
            history,
            history_len: 1,
        }
    }

    /// Display game board in ASCII art.
    pub fn display(&self) {
        for (i, row) in self.board.iter().enumerate() {
            print!("{}", i);
            for cell in row {
                print!(" {}", cell);
            }
            println!();
        }
        println!("  0 1 2 3 4 5 6");
    }

    /// Compute the hash of a move without applying it.
    #[inline]
    pub fn next_hash(&self, coords: &[usize; 4], z_table: &Zobrist) -> u64 {
        let (sr, sc, er, ec) = (coords[0], coords[1], coords[2], coords[3]);
        let piece = self.board[sr][sc];
        
        // Safety check (though engine shouldn't pass empty squares)
        let p_idx = match Zobrist::piece_index(piece) {
            Some(idx) => idx,
            None => return self.hash, 
        };

        let mut h = self.hash;

        // 1. Update hash for the move itself (Remove from start, add to end)
        h ^= z_table.table[sr][sc][p_idx];
        h ^= z_table.table[er][ec][p_idx];
        
        // 2. Update player turn
        h ^= z_table.black_to_move;

        // 3. Calculate captures (Simulated)
        let enemy = match piece {
            'B' => 'W',
            'W' | 'K' => 'B',
            _ => return h, // Return current hash if invalid piece
        };

        let enemy_idx = Zobrist::piece_index(enemy).unwrap();

        // Four orthogonal directions
        let directions: [(isize, isize); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];

        for (dr, dc) in directions {
            // -- Identify Victim (Adjacent to Destination) --
            // Note: We use er/ec (destination), not row/col
            let r_victim_i = er as isize + dr;
            let c_victim_i = ec as isize + dc;

            if r_victim_i < 0 || r_victim_i > 6 || c_victim_i < 0 || c_victim_i > 6 { continue; }

            let r_victim = r_victim_i as usize;
            let c_victim = c_victim_i as usize;

            if self.board[r_victim][c_victim] != enemy { continue; }

            // -- Identify Anvil (Beyond Victim) --
            let r_anvil_i = r_victim_i + dr;
            let c_anvil_i = c_victim_i + dc;

            if r_anvil_i < 0 || r_anvil_i > 6 || c_anvil_i < 0 || c_anvil_i > 6 { continue; }
            
            let r_anvil = r_anvil_i as usize;
            let c_anvil = c_anvil_i as usize;

            // -- Check Sandwich Condition --
            
            // CASE A: The "Ghost" Square.
            // If the anvil is the square we just moved FROM (sr, sc), 
            // it is effectively empty right now, even if self.board says otherwise.
            if r_anvil == sr && c_anvil == sc {
                // Check if this empty square acts as an anvil (Corner or Throne)
                if !self.is_static_hostile(r_anvil, c_anvil, enemy) {
                    continue; 
                }
            } 
            // CASE B: Standard Square.
            // The anvil is elsewhere. The board state is accurate for this square.
            else {
                if !self.is_hostile(r_anvil, c_anvil, enemy) { continue; }
            }

            // If we reached here, it is a capture!
            // Remove the victim from the hash.
            h ^= z_table.table[r_victim][c_victim][enemy_idx];
        }

        h
    }

    /// Helper to determine if a specific square coordinate is hostile by virtue of the map
    /// (Throne/Corners) assuming the square is currently empty or treated as such.
    #[inline]
    fn is_static_hostile(&self, row: usize, col: usize, victim: char) -> bool {
        // Corners are hostile to everyone
        if (row == 0 || row == 6) && (col == 0 || col == 6) { return true; }

        // Throne rules
        if row == 3 && col == 3 {
            match victim {
                'B' => true,          // Throne always hostile to black
                'W' => true,          // Throne hostile to white if empty
                _ => false
            }
        } else {
            false
        }
    }

    /// Move piece on the board and update hash and history.
    /// The logic assumes the move to be legal.
    /// coords = [start_row, start_col, end_row, end_col]
    #[inline]
    pub fn move_piece(&mut self, coords: &[usize; 4], z_table: &Zobrist) {
        let (sr, sc, er, ec) = (coords[0], coords[1], coords[2], coords[3]);
        let piece = self.board[sr][sc];
        let p_idx = Zobrist::piece_index(piece).unwrap();

        // Update board.
        self.board[er][ec] = piece;
        self.board[sr][sc] = '.';

        // Update hash.
        self.hash ^= z_table.table[sr][sc][p_idx];
        self.hash ^= z_table.table[er][ec][p_idx];
        self.hash ^= z_table.black_to_move;        

        // Update king position.
        if piece == 'K' {
            self.king_pos = (er, ec);
        }

        // Update history (only after updating self.hash).
        if self.history_len < MAX_GAME_LENGTH {
            self.history[self.history_len] = self.hash;
            self.history_len += 1;
        }
        
        // Update player.
        self.player = if self.player == 'B' { 'W' } else { 'B' };

        // Apply captures.
        self.apply_captures(er, ec, z_table);
    }

    /// Apply all captures.
    /// King capture is checked only in check_game_over()
    #[inline]
    fn apply_captures(&mut self, row: usize, col: usize, z_table: &Zobrist) {
        let mover = self.board[row][col];

        if mover == '.' {
            println!("Error: piece moved is empty.");
            return;
        }

        let targets = match mover {
            'B' => vec!['W', 'K'],
            'W' | 'K' => vec!['B'],
            _ => return,
        };

        // Four orthogonal directions
        let directions: [(isize, isize); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];

        for (dr, dc) in directions {
            // Check if adjacent square contains enemy.
            let er = row as isize + dr;
            let ec = col as isize + dc;

            if er < 0 || er > 6 || ec < 0 || ec > 6 { continue; }

            let er = er as usize;
            let ec = ec as usize;

            let neighbor = self.board[er][ec];

            // Check if neighbor is a target.
            if !targets.contains(&neighbor) { continue; }

            // === Handle King capture separately ===
            if neighbor == 'K' {
                if self.check_king_capture(er, ec) {
                    // Remove King.
                    self.board[er][ec] = '.';
                    // Update Hash.
                    let k_idx = Zobrist::piece_index('K').unwrap();
                    self.hash ^= z_table.table[er][ec][k_idx];
                    // Note: we don't return here, a move might capture multiple pieces.
                }
                continue;
            }

            // === Standard capture ===
            // Check square for the anvil (the piece beyond the victim).
            let br = er as isize + dr;
            let bc = ec as isize + dc;

            if br < 0 || br > 6 || bc < 0 || bc > 6 { continue; }
            
            let br = br as usize;
            let bc = bc as usize;

            if !self.is_hostile(br, bc, neighbor) { continue; }

            // Update board.
            self.board[er][ec] = '.';

            // Update hash.
            let enemy_idx = Zobrist::piece_index(neighbor).unwrap();
            self.hash ^= z_table.table[er][ec][enemy_idx];
        }
    }

    /// Check if the square is hostile to the victim (i.e. hostile square or enemy piece).
    /// victim can only be 'W' or 'B'
    #[inline]
    fn is_hostile(&self, row: usize, col: usize, victim: char) -> bool {
        if victim != 'B' && victim != 'W' {
            println!("Error: is_hostile() called for wrong piece: {}", victim);
            return false;
        }
        
        let square = self.board[row][col];

        // Enemy piece is always hostile
        if victim == 'B' {
            if square != '.' && square != victim { return true; }
        } else {
            if square != '.' && square != victim && square != 'K' { return true; }
        }
        
        // Corners are hostile to everyone
        if (row == 0 || row == 6) && (col == 0 || col == 6) { return true; }

        // Throne rules
        if row == 3 && col == 3 {
            match victim {
                'B' => true,               // throne always hostile to black
                'W' => square == '.',      // hostile to white only if empty
                _ => false,
            }
        } else {
            false
        }
    }
    
    /// Returns true if the King at (r, c) is captured.
    #[inline]
    fn check_king_capture(&self, r: usize, c: usize) -> bool {
        // If the king is on the throne,
        // he has to be surrounded on all four sides.
        if r == 3 && c == 3 {
            let neighbors = [(2,3), (3,2), (3,4), (4,3)];
            for (nr, nc) in neighbors {
                if self.board[nr][nc] != 'B' { return false; }
            }
            return true;
        }

        // If the king is next to the throne,
        // he has to be surrounded on the remaining three sides.
        if (r == 2 && c == 3) || (r == 3 && c == 2) || (r == 3 && c == 4) || (r == 4 && c == 3) {
            let neighbors = [
                (r as isize - 1, c as isize),
                (r as isize + 1, c as isize),
                (r as isize, c as isize - 1),
                (r as isize, c as isize + 1),
            ];
            for (nr, nc) in neighbors {
                let piece = self.board[nr as usize][nc as usize];
                // Hostile if it's Black or the Throne (3,3)
                let is_hostile = piece == 'B' || (nr == 3 && nc == 3);
                if !is_hostile { return false; }
            }
            return true;
        }

        // If the king is not at or next to the throne,
        // he can be captured like any other piece, with two enemies at the sides.
        // Note: The corner fields are hostile to all, including the King.
        let neighbors = [
            [
                (r as isize - 1, c as isize), // North
                (r as isize + 1, c as isize), // South
            ],
            [
                (r as isize, c as isize - 1), // West
                (r as isize, c as isize + 1), // East
            ]
        ];
        for pair in neighbors {
            let mut hostile_count = 0;
            for (er, ec) in pair {
                if er < 0 || er > 6 || ec < 0 || ec > 6 { continue; }
                let piece = self.board[er as usize][ec as usize];
                // A side is "hostile" if it is an Attacker OR a corner.
                if piece == 'B' || ((er == 0 || er == 6) && (ec == 0 || ec == 6)) {
                    hostile_count += 1;
                }
            }
            if hostile_count == 2 { return true; }
        }

        return false;
    }

    /// Checks whether a piece different than the king is entering a restricted square.
    #[inline]
    fn is_nonking_entering_restricted(&self, coords: &[usize; 4]) -> bool {
        if self.board[coords[0]][coords[1]] != 'K' {
            if ((coords[2] == 0 || coords[2] == 6) && (coords[3] == 0 || coords[3] == 6)) ||
                (coords[2] == 3 && coords[3] == 3) {
                    // println!("Invalid move: Only the king may occupy restricted squares.");
                    return true;
                }
        }
        return false;
    }

    /// Checks whether the move repeats a state that was already visited.
    #[inline]
    fn is_illegal_repetition(&self, coords: &[usize; 4], next_hash: &u64) -> bool {
        return self.history[0..self.history_len].contains(&next_hash);
    }

    /// Check if game is over, given a state and a move.
    /// Returns:
    /// None - Game is not over
    /// W - White wins
    /// B - Black wins
    /// D - Draw
    pub fn check_game_over(&self, z_table: &Zobrist) -> Option<char> {
        // === Check if King is at a corner => White wins ===
        let corners = [(0,0), (0,6), (6,0), (6,6)];
        for (r, c) in corners {
            if self.board[r][c] == 'K' {
                return Some('W');
            }
        }

        // === Check if King is captured => Black wins ===
        // We rely on the fact that if the King was captured,
        // he was removed from the board in apply_captures.
        let (kr, kc) = self.king_pos;
        if self.board[kr][kc] != 'K' {
            return Some('B');
        }

        // === Rule 9: If the player to move has no legal move, he loses. ===
        if !self.has_legal_move(self.player, &z_table) {
            let winner = if self.player == 'B' { 'W' } else { 'B' };
            return Some(winner);
        }

        // === Rule 10: Draw due to "impossible to end the game" / insufficient material ===
        if self.is_insufficient_material_draw() {
            return Some('D');
        }

        None
    }

    /// Simple heuristic for rule 10: declare draw if both sides have very few pieces left.
    /// Copenhagen: "If it is not possible to end the game, fx. because both sides have too few pieces left, it is a draw."
    /// This rule is intentionally vague; adjust DRAW_PIECE_THRESHOLD as desired.
    #[inline]
    fn is_insufficient_material_draw(&self) -> bool {
        const DRAW_PIECE_THRESHOLD: usize = 1; // <= 1 attackers AND <=1 defenders => draw
        let mut attackers = 0usize;
        let mut defenders = 0usize; // counts white pawns (not king)
        for row in &self.board {
            for &c in row {
                match c {
                    'B' => attackers += 1,
                    'W' => defenders += 1,
                    _ => {}
                }
            }
        }
        attackers <= DRAW_PIECE_THRESHOLD+1 && defenders <= DRAW_PIECE_THRESHOLD
    }

    /// Return true if the given player has at least one legal move.
    /// Function called only by check_game_over()
    #[inline]
    fn has_legal_move(&self, player: char, z_table: &Zobrist) -> bool {
        for r in 0..7 {
            for c in 0..7 {
                let piece = self.board[r][c];
                if piece == '.' { continue; }
                if player == 'B' && piece != 'B' { continue; }
                if player == 'W' && !(piece == 'W' || piece == 'K') { continue; }

                // try moves along 4 directions until blocked
                // up
                let mut rr = r as isize - 1;
                while rr >= 0 {
                    if self.board[rr as usize][c] != '.' { break; }
                    let coords = [r, c, rr as usize, c];
                    let next_hash = self.next_hash(&coords, &z_table);
                    if !self.is_nonking_entering_restricted(&coords)
                    && !self.is_illegal_repetition(&coords, &next_hash) { return true; }
                    rr -= 1;
                }
                // down
                let mut rr = r as isize + 1;
                while rr < 7 {
                    if self.board[rr as usize][c] != '.' { break; }
                    let coords = [r, c, rr as usize, c];
                    let next_hash = self.next_hash(&coords, &z_table);
                    if !self.is_nonking_entering_restricted(&coords)
                    && !self.is_illegal_repetition(&coords, &next_hash) { return true; }
                    rr += 1;
                }
                // left
                let mut cc = c as isize - 1;
                while cc >= 0 {
                    if self.board[r][cc as usize] != '.' { break; }
                    let coords = [r, c, r, cc as usize];
                    let next_hash = self.next_hash(&coords, &z_table);
                    if !self.is_nonking_entering_restricted(&coords)
                    && !self.is_illegal_repetition(&coords, &next_hash) { return true; }
                    cc -= 1;
                }
                // right
                let mut cc = c as isize + 1;
                while cc < 7 {
                    if self.board[r][cc as usize] != '.' { break; }
                    let coords = [r, c, r, cc as usize];
                    let next_hash = self.next_hash(&coords, &z_table);
                    if !self.is_nonking_entering_restricted(&coords)
                    && !self.is_illegal_repetition(&coords, &next_hash) { return true; }
                    cc += 1;
                }
            }
        }
        false
    }

    /// Modify in place the vector of legal moves from the current state.
    /// Avoids allocating a vector each time (the function is called multiple times during Simulation).
    /// Algorithm from has_legal_move() modified to guarantee that indices are usize (and avoid casting).
    pub fn get_legal_moves(&self, moves: &mut Vec<[usize; 4]>, z_table: &Zobrist) {
        moves.clear();
        for r in 0..7 {
            for c in 0..7 {
                let piece = self.board[r][c];
                if piece == '.' { continue; }
                if self.player == 'B' && piece != 'B' { continue; }
                if self.player == 'W' && !(piece == 'W' || piece == 'K') { continue; }

                // Try moves along 4 directions.
                // up
                if r > 0 {
                    for rr in (0..r).rev() { // rr goes from r-1 down to 0
                        if self.board[rr][c] != '.' { break; }
                        // All indices are of type usize.
                        let coords = [r, c, rr, c];
                        let next_hash = self.next_hash(&coords, &z_table);
                        if !self.is_nonking_entering_restricted(&coords)
                        && !self.is_illegal_repetition(&coords, &next_hash) {
                            moves.push(coords);
                        }
                    }
                }
                // down
                if r < 6 {
                    for rr in r+1..7 {
                        if self.board[rr][c] != '.' { break; }
                        let coords = [r, c, rr, c];
                        let next_hash = self.next_hash(&coords, &z_table);
                        if !self.is_nonking_entering_restricted(&coords)
                        && !self.is_illegal_repetition(&coords, &next_hash) {
                            moves.push(coords);
                        }
                    }
                }
                // left
                if c > 0 {
                    for cc in (0..c).rev() {
                        if self.board[r][cc] != '.' { break; }
                        let coords = [r, c, r, cc];
                        let next_hash = self.next_hash(&coords, &z_table);
                        if !self.is_nonking_entering_restricted(&coords)
                        && !self.is_illegal_repetition(&coords, &next_hash) {
                            moves.push(coords);
                        }
                    }
                }
                // right
                if c < 6 {
                    for cc in c+1..7 {
                        if self.board[r][cc] != '.' { break; }
                        let coords = [r, c, r, cc];
                        let next_hash = self.next_hash(&coords, &z_table);
                        if !self.is_nonking_entering_restricted(&coords)
                        && !self.is_illegal_repetition(&coords, &next_hash) {
                            moves.push(coords);
                        }
                    }
                }
            } 
        }
    }

    /// Gets a move from CLI.
    /// If valid then moves the piece.
    pub fn human_move(&mut self, z_table: &Zobrist) {
        loop {
            println!("\nCurrent Player: {}", self.player);
            print!("Enter move: ");

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
                    if self.is_legal_move(&coords, &z_table) {
                        self.move_piece(&coords, &z_table);
                        return;
                    } else {
                        continue;
                    }
                }
                Err(_) => {
                    println!("Invalid input. Try again.\n");
                    continue;
                }
            }
        }
    }

    /// Check if the given move (coords) is legal.
    /// Used only for user moves.
    #[inline]
    fn is_legal_move(&self, coords: &[usize; 4], z_table: &Zobrist) -> bool {
        // If start == end
        if coords[0] == coords[2] && coords[1] == coords[3] {
            // println!("Invalid move: Piece must move in a new square.");
            return false;
        }

        // Bounds check: any coordinate > 6 is invalid.
        if coords.iter().any(|&c| c > 6) {
            // println!("Invalid move: Out of bounds.");
            return false;
        }

        // Check if there is a piece at the starting position.
        let piece = self.board[coords[0]][coords[1]];
        if piece == '.' {
            // println!("Invalid move: No piece at start.");
            return false;
        }

        // Check if there already is a piece at the final position.
        if self.board[coords[2]][coords[3]] != '.' {
            // println!("Invalid move: Final square already occupied.");
            return false;
        }

        // Check if the piece belongs to the current (given) player.
        if self.player == 'B' && piece != 'B' {
            // println!("Invalid move: Black must move.");
            return false;
        }
        if self.player == 'W' && (piece != 'W' && piece != 'K') {
            // println!("Invalid move: White must move.");
            return false;
        }

        // Check for straight-line movement.
        if coords[0] != coords[2] && coords[1] != coords[3] {
            // println!("Invalid move: Non straight-line movement.");
            return false;
        }

        // Check if the movement goes through occupied squares.
        if coords[0] == coords[2] {
            // Horizontal movement.
            let clear_start = coords[1].min(coords[3]);
            let clear_end = coords[1].max(coords[3]);
            for i in (clear_start + 1)..clear_end {
                if self.board[coords[0]][i] != '.' {
                    // println!("Invalid move: Path occupied.");
                    return false;
                }
            }
        } else {
            // Vertical movement.
            let clear_start = coords[0].min(coords[2]);
            let clear_end = coords[0].max(coords[2]);
            for i in (clear_start + 1)..clear_end {
                if self.board[i][coords[1]] != '.' {
                    // println!("Invalid move: Path occupied.");
                    return false;
                }
            }
        }

        // Restricted squares may only be occupied by the king.
        if self.is_nonking_entering_restricted(&coords) {
            return false;
        }

        // White player cannot repeat move.
        let next_hash = self.next_hash(&coords, &z_table);
        if self.is_illegal_repetition(&coords, &next_hash) {
            return false;
        }

        println!("Valid move.\n");
        return true;
    }
}
