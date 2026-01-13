use rand::thread_rng;
use rand:prelude::*;

use crate::zobrist::Zobrist;
use crate::transposition::TT;
use crate::hnefatafl::GameState;

// Maximum number of moves (estimated). Used to allocate the vector of legal moves efficiently.
const MAX_MOVES: usize = 128;

pub struct MCTS {
    // Configuration.
    pub iterations_per_move: u32, // == generation_range
    pub ucb_const: f64,
    
    // Used to age out old TT entries.
    pub generation: u32,
    pub generation_bound: u32, // == generation - generation_range

    // Heavy data structures.
    pub transpositions: TT,
    pub zobrist: Zobrist,
}

impl MCTS {
    pub fn new(seed: u64) -> Self {
        Self {
            iterations_per_move: 1_000_000,
            ucb_const: 1.414,
            generation: 0,
            generation_bound: 0,
            transpositions: TT::new(),
            zobrist: Zobrist::new(seed),
        }
    }

    #[inline]
    fn update_generation(&self) {
        self.generation += 1;
        if self.generation > self.iterations_per_move {
            self.generation_bound += 1;
        }
    }
}

/// ======================
/// === MCTS Algorithm ===
/// ======================
impl MCTS {
    pub fn get_move(&self, root: GameState) {
        // Explore game tree.
        self.start_search(&root);

        // Choose best move: the most visited child.

    }

    fn start_search(&self, root: GameState) {
        // === Store the root in the transposition table. ===
        let bucket = self.transpositions.get_bucket(root.hash);
        bucket.add_entry(root.hash, self.generation, self.generation_bound);
        // Root cannot have 0 visits because the first UCB value would be NaN.
        if let Some(root_entry) = bucket.get_entry(root.hash) {
            if root_entry.n_visits == 0 { root_entry.n_visits = 1; }
        } else {
            println!("Error: root not added to transpositions table.");
        }

        // === Explore the game tree. ===
        // Selection.
    }

    /// SELECTION.
    /// Returns the result with the perspective of state.player
    ///  1 if state.player won.
    /// -1 if state.player lost.
    ///  0 if it was a draw.
    fn selection(&mut self, state: &GameState, current_hash: u64, node_visits: u32) -> i32 {
        // === TERMINAL CHECKS ===
        // If game is over.
        if let Some(winner) = state.check_game_over() {
            // If we are at a terminal node during selection,
            // it means the *previous* player made a winning move
            if winner == 'T' { return 0; } // Draw
            else { return -1; } // Loss
        }

        // If heuristic_wins_B

        // If heuristic_wins_W

        // === SELECTION ===
        let selected_move: [usize; 4];
        let selected_hash: u64;
        let is_expansion_phase;
        let mut best_move_visits = 0;
        {
            // === COMPUTE UCB ===
            let mut moves = Vec::with_capacity(MAX_MOVES);
            state.get_legal_moves(&moves);

            let mut max_ucb_value = -1.0;
            let mut best_move: Option<[usize; 4]> = None;
            let mut best_move_hash: u64 = 0;
            
            let mut unvisited_moves = Vec::new();

            for m in &moves {
                let child_hash = self.zobrist.update_for_move(current_hash, m, state);

                // Try to retrieve the child from the Transposition Table.
                let child_bucket = self.transpositions.get_bucket(child_hash);
                let mut is_visited = false;
                let mut child_visits = 0u32;
                let mut child_wins = 0i32;
                if let Some(entry) = self.transpositions.get_entry(child_hash) {
                    if entry.n_visits > 0 {
                        is_visited = true;
                        child_visits = entry.n_visits;
                        child_wins = entry.n_wins;
                    }
                }

                if is_visited {
                    // === UCB FORMULA ===
                    // Q_normalized = ((wins / visits) + 1) / 2
                    let q_val = (child_wins as f64) / (child_visits as f64);
                    let qnorm = (q_val + 1.0) / 2.0;

                    // UCB = Q + C * sqrt(ln(node_visits) / child_visits)
                    let exploration = self.ucb_const * ((node_visits as f64).ln() / (child_visits as f64)).sqrt();
                    let ucb = q_norm + exploration;

                    if ucb > max_ucb_value {
                        max_ucb_value = ucb;
                        best_move = Some(m.clone());
                        best_move_hash = child_hash;
                        best_move_visits = child_visits;
                    }
                } else {
                    // If unvisited, store it for later decision.
                    unvisited_moves.push((m.clone(), child_hash));
                }
            }

            // === CHOICE ===
            if !unvisited_moves.is_empty() {
                // Pick random unvisited child.
                let idx = rand::thread_rng().gen_range(0..unvisited_moves.len());
                let (m, h) = unvisited_moves[idx].clone();
                selected_move = m;
                selected_hash = h;
                is_expansion_phase = true;
            
            } else if let Some(m) = best_move {
                selected_move = m;
                selected_hash = best_move_hash;
                is_expansion_phase = false;
            } else {
                // No moves available. Should be caught by terminal check.
                println!("Error: Selection step has no moves but game over wasn't caught.");
                return -1; // Loss for current player.
            }
        }
        
        // === EXECUTE MOVE ===
        let next_state = state.clone();
        next_state.move_piece(&selected_move, &self.zobrist);
        let result_for_current_node: i32;

        if is_expansion_phase {
            // === EXPANSION ===
            {
                let bucket = self.transpositions.get_bucket(selected_hash);
                bucket.add_entry(selected_hash, self.generation, self.generation_bound);
            }

            // === SIMULATION ===
            // result_for_current_node = simulation()
        } else {
            // === RECURSIVE SELECTION ===
            let child_result = self.selection(&next_state, selected_hash, best_move_visits);
            result_for_current_node = -child_result;
        }

        // === BACKPROPAGATION ===
        {
            let bucket = self.transpositions.get_bucket(selected_hash);
            if let Some(entry) = bucket.get_entry(root.hash) {
                entry.generation += self.generation;
                entry.n_visits += 1;
                entry.n_wins += result_for_current_node;
            } else {
                println!("Error: Entry wasn't found during backpropagation.");
                println!("This means there is a problem with the overwriting policy.");
            }
        }

        // Return result with the perspective of the current node.
        return result_for_current_node;
    }

    /// SIMULATION.
    /// Returns the result with the perspective of state.player
    fn simulation(&self, state: &GameState) -> i32 {
        let mut temp_state = state.clone();
        let mut moves = Vec::with_capacity(MAX_MOVES);
        let mut rng = thread_rng();

        // Play random moves until the game is over.
        loop {
            // Check game over.
            if let Some(winner) = temp_state.check_game_over() {
                if winner == 'T' { return 0; }
                else if winner == state.player { return 1; }
                else { return -1; }
            }

            // Available moves.
            temp_state.get_legal_moves(&moves);
            if moves.is_empty() {
                println!("Error: Simulation step has no moves but game over wasn't caught.");
                // Current player loses (Rule 9: If a player cannot move, he loses the game).
                if state.player == temp_state.player { return -1; }
                else { return 1; }
            }

            // Random move.
            let random_move = moves.choose(&mut rng).unwrap(); // returns a reference

            // Apply move.
            temp_state.move_piece(random_move, &self.zobrist);
        }
    }
}
