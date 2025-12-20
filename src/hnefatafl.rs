use std::io::{self, Write};

pub struct GameState {
    pub board: [[char; 7]; 7],
    pub player: char,
}

impl GameState {
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

        GameState {
            board: initial_board,
            player: 'B',
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

        // === Check if King is captured -> Black wins ===
        
        // Find king on the board.
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
                if r < 0 || r > 6 || c < 0 || c > 6 {
                    continue;
                }

                let piece = self.board[r as usize][c as usize];

                // A side is "hostile" if it is an Attacker OR the Throne.
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
            
            let mut hostile_count = 0;
            
            for pair in neighbors {
                for (r, c) in pair {
                    if r < 0 || r > 6 || c < 0 || c > 6 {
                        continue; // It applies to inner "for loop" on.
                    }

                    let piece = self.board[r as usize][c as usize];

                    // A side is "hostile" if it is an Attacker OR a corner.
                    if piece == 'B' || ((r == 0 || r == 6) && (c == 0 || c == 6)) {
                        hostile_count += 1;
                    }
                }

                if hostile_count == 2 { return Some('B'); }
            }
        }

        // If a player cannot move, he loses the game.


        None
    }

    /// Move piece on the board.
    /// The logic expects the array to contain:
    /// 0 -> start_row
    /// 1 -> start_col
    /// 2 -> end_row
    /// 3 -> end_col
    pub fn move_piece(&mut self, coords: &[usize; 4]) {
        let piece = self.board[coords[0]][coords[1]];

        self.board[coords[2]][coords[3]] = piece;
        self.board[coords[0]][coords[1]] = '.';

        if self.player == 'B' { self.player = 'W'; }
        else { self.player = 'B'; }
    }

    /// Check if the move is valid (allowed by the rules).
    /// The logic expects the array to contain:
    /// 0 -> start_row
    /// 1 -> start_col
    /// 2 -> end_row
    /// 3 -> end_col
    pub fn move_is_valid(&self, coords: &[usize; 4]) -> bool {

        // Check if it the piece remains in the same position.
        if coords[0] == coords[2] && coords[1] == coords[3] {
            println!("Invalid move: Piece must move in a new square.");
        }

        // Check Bounds.
        if coords.iter().all(|&c| c > 6) {
            println!("Invalid move: Out of bounds.");
            return false;
        }

        // Check if there is a piece at the starting position.
        let piece = self.board[coords[0]][coords[1]];
        if piece == '.' {
            println!("Invalid move: No piece at start.");
            return false;
        }

        // Check if there already is a piece at the final position.
        if self.board[coords[2]][coords[3]] != '.' {
            println!("Invalid move: Final square already occupied.");
            return false;
        }

        // Restricted squares may only be occupied by the king.
        if self.board[coords[0]][coords[1]] != 'K' &&
           (((coords[2] == 0 || coords[2] == 6) && (coords[3] == 0 || coords[3] == 6)) ||
            (coords[2] == 3 && coords[3] == 3)) {
            println!("Invalid move: Only the king may occupy restricted squares.");
            return false;
           }

        // Check if the piece belongs to the current player.
        if self.player == 'B' && piece != 'B' {
            println!("Invalid move: Black must move.");
            return false;
        }
        if self.player == 'W' && (piece != 'W' && piece != 'K') {
            println!("Invalid move: White must move.");
            return false;
        }

        // Check for straight-line movement.
        if coords[0] != coords[2] && coords[1] != coords[3] {
            println!("Invalid move: Non straight-line movement.");
            println!("Pieces move any number of vacant squares along a row or a column, like a rook in chess.");
            return false;
        }

        // Check if the movement goes through occupied squares.
        if coords[0] == coords[2] {
            // Horizontal movement.
            let clear_start = coords[1].min(coords[3]);
            let clear_end = coords[1].max(coords[3]);
            for i in (clear_start + 1)..clear_end {
                if self.board[coords[0]][i] != '.' {
                    println!("Invalid move: Path occupied.");
                    return false;
                }
            }
        } else {
            // Vertical movement.
            let clear_start = coords[0].min(coords[2]);
            let clear_end = coords[0].max(coords[2]);
            for i in (clear_start + 1)..clear_end {
                if self.board[i][coords[1]] != '.' {
                    println!("Invalid move: Path occupied.");
                    return false;
                }
            }
        }

        true
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
