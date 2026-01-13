/// Game State and rules implementation.
/// The moves are encoded as an array: coords = [start_row, start_col, end_row, end_col]

use std::io::{self, Write};
use crate::zobrist::Zobrist;

/// The maximum number of move repetition White can perform.
/// Note: For Rule 8 (Perpetual repetitions), instead of making black win, we forbid
/// repetitions when white moves. Rule 9 makes the rule work anyways.
const MAX_REPEATING_MOVES: usize = 5;

#[derive(Clone, Copy)]
pub struct GameState {
    pub board: [[char; 7]; 7],
    pub player: char,
    pub hash: u64,
    // Track king to avoid scanning board in check_game_over() to find it.
    king_pos: (usize, usize),
    // History for Rule 8.
    last_move_white: [usize; 4],
    last_move_white_counter: usize,
}

impl GameState {
    pub fn new(zobrist: &Zobrist) -> Self {
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
                    hash ^= zobrist.table[r][c][p_idx];
                }
            }
        }
        hash ^= zobrist.black_to_move;

        Self {
            board: initial_board,
            player: 'B',
            hash,
            king_pos: (3, 3),
            last_move_white: [0; 4],
            last_move_white_counter: 0,
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

    /// Move piece on the board and update hash and history.
    /// The logic assumes the move to be legal.
    /// coords = [start_row, start_col, end_row, end_col]
    #[inline]
    pub fn move_piece(&mut self, coords: &[usize; 4], zobrist: &Zobrist) {
        let (sr, sc, er, ec) = (coords[0], coords[1], coords[2], coords[3]);
        let piece = self.board[sr][sc];
        let p_idx = Zobrist::piece_index(piece).unwrap();

        // Update board.
        self.board[er][ec] = piece;
        self.board[sr][sc] = '.';

        // Update hash.
        self.hash ^= zobrist.table[sr][sc][p_idx];
        self.hash ^= zobrist.table[er][ec][p_idx];
        self.hash ^= zobrist.black_to_move;        

        // Update king position.
        if piece == 'K' {
            self.king_pos = (er, ec);
        }

        // Update last move.
        if self.is_repetition(coords) {
            self.last_move_white_counter += 1;
        } else {
            self.last_move_white = *coords;
            self.last_move_white_counter = 1;
        }
        
        // Update player (only after updating last move).
        self.player = if self.player == 'B' { 'W' } else { 'B' };

        // Apply captures.
        self.apply_captures(er, ec, zobrist);
    }

    /// Checks whether White is repeating the move.
    #[inline]
    fn is_repetition(&self, coords: &[usize; 4]) -> bool {
        if self.player == 'W' {
            if (self.last_move_white == coords) ||
            (self.last_move_white == [coords[2], coords[3], coords[0], coords[1]]) {
                return true;
            }
        }
        return false;
    }

    /// Apply captures to nonking pieces.
    /// King capture is checked only in check_game_over()
    #[inline]
    fn apply_captures(&mut self, row: usize, col: usize, zobrist: &Zobrist) {
        let mover = self.board[row][col];

        if mover == '.' {
            println!("Error: piece moved is empty.");
            return;
        }

        let enemy = match mover {
            'B' => 'W',
            'W' | 'K' => 'B',
            _ => return,
        };

        let enemy_idx = Zobrist::piece_index(enemy).unwrap();

        // Four orthogonal directions
        let directions = [(-1, 0), (1, 0), (0, -1), (0, 1)];

        for (dr, dc) in directions {
            // Check if adjacent square contains enemy.
            let er = row as isize + dr;
            let ec = col as isize + dc;

            if er < 0 || er > 6 || ec < 0 || ec > 6 { continue; }

            let er = er as usize;
            let ec = ec as usize;

            if self.board[er][ec] != enemy { continue; }

            // Check square beyond the enemy for a sandwich.
            let br = er as isize + dr;
            let bc = ec as isize + dc;

            if br < 0 || br > 6 || bc < 0 || bc > 6 { continue; }
            
            let br = br as usize;
            let bc = bc as usize;

            if !self.is_hostile(br, bc, enemy) { continue; }

            // Update board.
            self.board[er][ec] = '.';

            // Update hash.
            self.hash ^= zobrist.table[er][ec][enemy_idx];
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
            }
        } else {
            false
        }
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

    /// Checks whether White is repeating the move exceeding the repetition limit.
    #[inline]
    fn is_illegal_repetition(&self, coords: &[usize; 4]) -> bool {
        if self.player == 'W' {
            if (self.last_move_white == coords) ||
            (self.last_move_white == [coords[2], coords[3], coords[0], coords[1]]) {
                if self.last_move_white_counter == (MAX_REPEATING_MOVES - 1) {
                    return true;
                }
            }
        }
        return false;
    }

    /// Check if game is over.
    /// Returns:
    /// None - Game is not over
    /// W - White wins
    /// B - Black wins
    /// D - Draw
    pub fn check_game_over(&self) -> Option<char> {
        // === Check if King is at a corner => White wins ===
        let corners = [(0,0), (0,6), (6,0), (6,6)];
        for (r, c) in corners {
            if self.board[r][c] == 'K' {
                return Some('W');
            }
        }

        // === Check if King is captured => Black wins ===
        let k_row: usize = self.king_pos.0;
        let k_col: usize = self.king_pos.1;

        // If the king is on the throne,
        // he has to be surrounded on all four sides.
        if k_row == 3 && k_col == 3 {
            if self.board[2][3] == 'B' && self.board[3][2] == 'B' &&
                self.board[3][4] == 'B' && self.board[4][3] == 'B' {
                return Some('B');
            }
        }

        // If the king is next to the throne,
        // he has to be surrounded on the remaining three sides.
        else if (k_row == 2 && k_col == 3) || (k_row == 3 && k_col == 2) ||
            (k_row == 3 && k_col == 4) || (k_row == 4 && k_col == 3) {
            let neighbors = [
                (k_row as isize - 1, k_col as isize), // North
                (k_row as isize + 1, k_col as isize), // South
                (k_row as isize, k_col as isize - 1), // West
                (k_row as isize, k_col as isize + 1), // East
            ];
            let mut hostile_count = 0;
            for (r, c) in neighbors {
                if r < 0 || r > 6 || c < 0 || c > 6 { continue; }
                let piece = self.board[r as usize][c as usize];
                // A side is hostile if it is an Attacker OR the Throne.
                if piece == 'B' || (r == 3 && c == 3) {
                    hostile_count += 1;
                }
            }
            if hostile_count == 4 { return Some('B'); }
        }

        // If the king is not at or next to the throne,
        // he can be captured like any other piece, with two enemies at the sides.
        // Note: The corner fields are hostile to all, including the King.
        else {
            let neighbors = [
                [
                    (k_row as isize - 1, k_col as isize), // North
                    (k_row as isize + 1, k_col as isize), // South
                ],
                [
                    (k_row as isize, k_col as isize - 1), // West
                    (k_row as isize, k_col as isize + 1), // East
                ]
            ];
            for pair in neighbors {
                let mut hostile_count = 0;
                for (r, c) in pair {
                    if r < 0 || r > 6 || c < 0 || c > 6 { continue; }
                    let piece = self.board[r as usize][c as usize];
                    // A side is "hostile" if it is an Attacker OR a corner.
                    if piece == 'B' || ((r == 0 || r == 6) && (c == 0 || c == 6)) {
                        hostile_count += 1;
                    }
                }
                if hostile_count == 2 { return Some('B'); }
            }
        }

        // === Rule 9: If the player to move has no legal move, he loses. ===
        if !self.has_legal_move(self.player) {
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
    fn has_legal_move(&self, player: char) -> bool {
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
                    if !self.is_nonking_entering_restricted(&coords)
                    && !self.is_illegal_repetition(&coords) {return true; }
                    rr -= 1;
                }
                // down
                let mut rr = r as isize + 1;
                while rr < 7 {
                    if self.board[rr as usize][c] != '.' { break; }
                    let coords = [r, c, rr as usize, c];
                    if !self.is_nonking_entering_restricted(&coords)
                    && !self.is_illegal_repetition(&coords) {return true; }
                    rr += 1;
                }
                // left
                let mut cc = c as isize - 1;
                while cc >= 0 {
                    if self.board[r][cc as usize] != '.' { break; }
                    let coords = [r, c, r, cc as usize];
                    if !self.is_nonking_entering_restricted(&coords)
                    && !self.is_illegal_repetition(&coords) {return true; }
                    cc -= 1;
                }
                // right
                let mut cc = c as isize + 1;
                while cc < 7 {
                    if self.board[r][cc as usize] != '.' { break; }
                    let coords = [r, c, r, cc as usize];
                    if !self.is_nonking_entering_restricted(&coords)
                    && !self.is_illegal_repetition(&coords) {return true; }
                    cc += 1;
                }
            }
        }
        false
    }

    /// Modify in place the vector of legal moves from the current state.
    /// Avoids allocating a vector each time (the function is called multiple times during Simulation).
    /// Algorithm from has_legal_move() modified to guarantee that indices are usize (and avoid casting).
    pub fn get_legal_moves(&self, moves: &mut Vec<[usize; 4]>) {
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
                        // Restricted squares may only be occupied by the king.
                        // All other check for move validity are already guaranteed.
                        if !self.is_nonking_entering_restricted(&coords)
                        && !self.is_illegal_repetition(&coords) {
                            moves.push(coords);
                        }
                    }
                }
                // down
                if r < 6 {
                    for rr in (r+1..7) {
                        if self.board[rr][c] != '.' { break; }
                        let coords = [r, c, rr, c];
                        if !self.is_nonking_entering_restricted(&coords)
                        && !self.is_illegal_repetition(&coords) {
                            moves.push(coords);
                        }
                    }
                }
                // left
                if c > 0 {
                    for cc in (0..c).rev() {
                        if self.board[r][cc] != '.' { break; }
                        let coords = [r, c, r, cc];
                        if !self.is_nonking_entering_restricted(&coords)
                        && !self.is_illegal_repetition(&coords) {
                            moves.push(coords);
                        }
                    }
                }
                // right
                if c < 6 {
                    for cc in (c+1..7) {
                        if self.board[r][cc] != '.' { break; }
                        let coords = [r, c, r, cc];
                        if !self.is_nonking_entering_restricted(&coords)
                        && !self.is_illegal_repetition(&coords) {
                            moves.push(coords);
                        }
                    }
                }
            } 
        }
    }

    /// Gets a move from CLI.
    /// If valid then moves the piece.
    pub fn human_move(&mut self, zobrist: &Zobrist) {
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
                    if self.is_legal_move(&coords) {
                        self.move_piece(&coords, zobrist);
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
    fn is_legal_move(&self, coords: &[usize; 4]) -> bool {
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
        if self.is_illegal_repetition(&coords) {
            return false;
        }

        println!("Valid move.\n");
        return true;
    }
}
