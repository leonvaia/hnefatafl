use std::collections::VecDeque;
use std::io::{self, Write};
use crate::zobrist::Zobrist;

const REPS: usize = 5;

pub struct GameState {
    pub board: [[char; 7]; 7],
    pub player: char,

    // Zobrist-related
    zobrist: Zobrist,
    hash: u64,

    // Hashes of last REPS game states stored as history
    history: VecDeque<u64>,
}

impl GameState {
    fn compute_hash(&self) -> u64 {
        let mut h = 0u64;

        for r in 0..7 {
            for c in 0..7 {
                if let Some(p) = Zobrist::piece_index(self.board[r][c]) {
                    h ^= self.zobrist.table[r][c][p];
                }
            }
        }

        if self.player == 'B' {
            h ^= self.zobrist.black_to_move;
        }

        h
    }

    pub fn new() -> Self {
        let initial_board = [
            ['.', '.', '.', 'B', '.', '.', '.'],
            ['.', '.', '.', 'B', '.', '.', '.'],
            ['.', '.', '.', 'W', '.', '.', '.'],
            ['B', 'B', 'W', 'K', 'W', 'B', 'B'],
            ['.', '.', '.', 'W', '.', '.', '.'],
            ['.', '.', '.', 'B', '.', '.', '.'],
            ['.', '.', '.', 'B', '.', '.', '.'],
        ];

        let zobrist = Zobrist::new(0xCAFEBABE);

        let mut gs = GameState {
            board: initial_board,
            player: 'B',
            zobrist,
            hash: 0,
            history: VecDeque::with_capacity(2),
        };

        gs.hash = gs.compute_hash();
        gs.history.push_back(gs.hash);

        gs
    }

    /// Serialize board and player to a small string for history comparisons.
    fn serialize_state(&self) -> String {
        let mut s = String::with_capacity(1 + 7*7);
        s.push(self.player);
        s.push('|');
        for row in &self.board {
            for &c in row {
                s.push(c);
            }
        }
        s
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

    /// Check if game is over.
    /// Returns:
    /// None - Game is not over
    /// W - White wins
    /// B - Black wins
    /// D - Draw
    /// E - Error
    pub fn check_game_over(&self) -> Option<char> {
        // === Check if King is at a corner -> White wins ===
        let corners = [(0,0), (0,6), (6,0), (6,6)];
        for (r, c) in corners {
            if self.board[r][c] == 'K' {
                return Some('W');
            }
        }

        // === Find king on the board. ===
        let mut k_row: usize = 7;
        let mut k_col: usize = 7;
        for (i, row) in self.board.iter().enumerate() {
            for (j, cell) in row.iter().enumerate() {
                if *cell == 'K' {
                    k_row = i;
                    k_col = j;
                }
            }
        }
        if k_row == 7 || k_col == 7 {
            println!("\nError: King not found on the board.");
            return Some('E');
        }

        // === King capture logic (your existing rules, kept) ===
        // If the king is on the throne (3,3) he must be surrounded on all four sides.
        if k_row == 3 && k_col == 3 {
            if self.board[2][3] == 'B' && self.board[3][2] == 'B' &&
                self.board[3][4] == 'B' && self.board[4][3] == 'B' {
                return Some('B');
            }
        }
        // Next to throne: surrounded on remaining three sides.
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
        // Not at or next to throne: capture like a normal piece (two enemies on opposite sides).
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

        // === Rule 8: Perpetual repetition detection ===
        // Copenhagen: "Perpetual repetitions are forbidden. A perpetual repetition in the last few plies results in a loss for white."
        // Implementation choice: if the current (player+board) state has appeared before -> repetition -> Black wins.
        let occurrences = self.history.iter().filter(|&&s| s == self.hash).count();
        if occurrences >= 2 {
            return Some('B');
        }

        // === Rule 9: If the player to move has no legal move, he loses. ===
        if !self.has_any_valid_move(self.player) {
            let winner = if self.player == 'B' { 'W' } else { 'B' };
            return Some(winner);
        }

        // === Rule 10: Draw due to "impossible to end the game" / insufficient material ===
        if self.is_insufficient_material_draw() {
            return Some('D');
        }

        None
    }

    /// Move piece on the board and record history.
    /// The logic expects the array to contain:
    /// 0 -> start_row
    /// 1 -> start_col
    /// 2 -> end_row
    /// 3 -> end_col
    pub fn move_piece(&mut self, coords: &[usize; 4]) {
        let (sr, sc, er, ec) = (coords[0], coords[1], coords[2], coords[3]);
        let piece = self.board[sr][sc];

        let p_idx = Zobrist::piece_index(piece).unwrap();

        // XOR out piece from start square
        self.hash ^= self.zobrist.table[sr][sc][p_idx];

        // XOR in piece on end square
        self.hash ^= self.zobrist.table[er][ec][p_idx];

        // Update board
        self.board[er][ec] = piece;
        self.board[sr][sc] = '.';

        // Toggle side to move
        self.hash ^= self.zobrist.black_to_move;
        self.player = if self.player == 'B' { 'W' } else { 'B' };

        // Store hash (keep only last 2)
        self.history.push_back(self.hash);
        while self.history.len() > REPS {
            self.history.pop_front();
        }
    }


    /// Check if the move is valid for the *current* player. (Kept for CLI use.)
    pub fn move_is_valid(&self, coords: &[usize; 4]) -> bool {
        self.move_is_valid_for(coords, self.player)
    }

    /// Move validity but for a given player (so we can generate moves without mutating player).
    pub fn move_is_valid_for(&self, coords: &[usize; 4], player: char) -> bool {
        // start != end
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

        // Restricted squares may only be occupied by the king.
        if self.board[coords[0]][coords[1]] != 'K' &&
            (((coords[2] == 0 || coords[2] == 6) && (coords[3] == 0 || coords[3] == 6)) ||
                (coords[2] == 3 && coords[3] == 3)) {
            // println!("Invalid move: Only the king may occupy restricted squares.");
            return false;
        }

        // Check if the piece belongs to the current (given) player.
        if player == 'B' && piece != 'B' {
            // println!("Invalid move: Black must move.");
            return false;
        }
        if player == 'W' && (piece != 'W' && piece != 'K') {
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

        true
    }

    /// Return true if the given player has at least one legal move.
    pub fn has_any_valid_move(&self, player: char) -> bool {
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
                    if self.move_is_valid_for(&coords, player) { return true; }
                    rr -= 1;
                }
                // down
                let mut rr = r as isize + 1;
                while rr < 7 {
                    if self.board[rr as usize][c] != '.' { break; }
                    let coords = [r, c, rr as usize, c];
                    if self.move_is_valid_for(&coords, player) { return true; }
                    rr += 1;
                }
                // left
                let mut cc = c as isize - 1;
                while cc >= 0 {
                    if self.board[r][cc as usize] != '.' { break; }
                    let coords = [r, c, r, cc as usize];
                    if self.move_is_valid_for(&coords, player) { return true; }
                    cc -= 1;
                }
                // right
                let mut cc = c as isize + 1;
                while cc < 7 {
                    if self.board[r][cc as usize] != '.' { break; }
                    let coords = [r, c, r, cc as usize];
                    if self.move_is_valid_for(&coords, player) { return true; }
                    cc += 1;
                }
            }
        }
        false
    }

    /// Simple heuristic for rule 10: declare draw if both sides have very few pieces left.
    /// Copenhagen: "If it is not possible to end the game, fx. because both sides have too few pieces left, it is a draw."
    /// This rule is intentionally vague; adjust DRAW_PIECE_THRESHOLD as desired.
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

    /// Gets a move from CLI. If valid then moves the piece.
    pub fn get_human_move(&mut self) {
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
                    if self.move_is_valid(&coords) {
                        self.move_piece(&coords);
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
}
