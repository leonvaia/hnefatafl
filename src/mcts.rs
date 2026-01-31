//! MCTS algorithm.

use std::io::Write;
use rand::prelude::*;

use crate::zobrist::Zobrist;
use crate::transposition::TT;
use crate::transposition::MAX_ITER;
use crate::transposition::CollisionType;
use crate::hnefatafl::GameState;

/// Negamax values.
const WIN: isize = 1;
const LOSS: isize = 0;
const DRAW: isize = -1;

/// Maximum number of generations (to prevent data corruption) according to current bit layout.
const MAX_GEN: u32 = 1 << 15; // = 2^GEN_BITS

/// Maximum number of moves (estimated). Used to allocate the vector of legal moves efficiently.
pub(crate) const MAX_MOVES: usize = 128;

pub struct MCTS {
    // Configuration.
    iterations_per_move: u32, // == generation_range
    ucb_const: f64,
    
    // Used to age out old TT entries.
    generation: u32,
    pub generation_range: u32,
    generation_bound: u32, // = generation - generation_range

    // Heavy data structures.
    transpositions: TT,
    pub z_table: Zobrist,

    // Evaluation of transposition table.
    written_entries: usize,
    overwritten_entries_in: usize,
    overwritten_entries_out: usize,
}

impl MCTS {
    pub fn new(seed: u64, iterations_per_move: u32, generation_range: u32) -> Self {
        // To prevent overflow check: 2^VISITS_BITS > 2^GEN_BITS * iterations_per_move
        if iterations_per_move >= MAX_ITER {
            panic!("Number of iteration passed might cause an overflow.");
        }

        Self {
            iterations_per_move,
            ucb_const: 1.414,
            generation: 0,
            generation_range,
            generation_bound: 0,
            transpositions: TT::new(),
            z_table: Zobrist::new(seed),
            written_entries: 0,
            overwritten_entries_in: 0,
            overwritten_entries_out: 0,
        }
    }

    /// Helpers for transposition collision handling.
    #[inline]
    fn increase_generation(&mut self) {
        self.generation += 1;
        if self.generation > self.generation_range {
            self.generation_bound += 1; // = generation - generation_range
        }
        if self.generation >= MAX_GEN {
            panic!("Reached maximum generation. To go further you will need to change the bit layout");
        }

        // Reset partial counts of collisions.
        self.written_entries = 0;
        self.overwritten_entries_in = 0;
        self.overwritten_entries_out = 0;
    }
    #[inline]
    fn increase_collision_in(&mut self) {
        self.written_entries += 1;
        self.overwritten_entries_in += 1;
    }
    #[inline]
    fn increase_collision_out(&mut self) {
        self.written_entries += 1;
        self.overwritten_entries_out += 1;
    }
}

/// ======================
///     MCTS Algorithm
/// ======================
impl MCTS {
    /// Apply engine move to state.
    pub fn computer_move<W: Write>(&mut self, state: &mut GameState, writer: &mut W) {
        let m = self.get_move(&state, writer);
        state.move_piece(&m, &self.z_table, true, writer);
    }

    /// Get best move according to MCTS.
    fn get_move<W: Write>(&mut self, root: &GameState, writer: &mut W) -> [usize; 4] {
        // Heuristics.
        if root.player == 'W' {
            // 1.
            if let (true, Some(winning_move)) = root.heuristic_king_to_corner() {
                 return winning_move;
            }
            // 2.
            if let (true, Some(winning_move)) = root.heuristic_king_empty_edge() {
                 return winning_move;
             }
        } else {
            if let (true, Some(winning_move)) = root.heuristic_capture_king() {
                return winning_move;
            }
        }

        // Search game tree.
        self.start_search(root, writer);

        // === CHOOSE BEST MOVE: the most visited child ===
        let mut moves = Vec::with_capacity(MAX_MOVES);
        root.get_legal_moves(&mut moves, true);
        let mut moves_not_cached = 0;

        let mut max_visits = 0;
        let mut max_wins = 0;
        let mut best_move: Option<[usize; 4]> = None;

        // Consider only moves that do NOT result in a loss for current player.
        for m in &moves {
            let child_hash = root.next_hash(m, &self.z_table);
            let child_bucket = self.transpositions.get_bucket(child_hash);
            if let Some(entry) = child_bucket.get_entry(child_hash) {
                if entry.get_n_visits() > max_visits {
                    let mut next_state = root.clone();
                    next_state.move_piece(m, &self.z_table, false, writer);
                    if let Some(winner) = next_state.check_game_over() {
                        if !(root.player != winner) {
                            // Game is over and it is NOT a loss for current player. consider the move.
                            max_visits = entry.get_n_visits();
                            max_wins = entry.get_n_wins();
                            best_move = Some(m.clone());
                        }
                    } else {
                        // Game isn't over, consider the move.
                        max_visits = entry.get_n_visits();
                        max_wins = entry.get_n_wins();
                        best_move = Some(m.clone());
                    }
                }
            } else {
                moves_not_cached += 1;
            }
        }
        
        writeln!(writer, "Number of child moves not cached: {}", moves_not_cached).expect("could not write to output");

        // If found a move, return it and print relative information.
        if let Some(mv) = best_move {
            writeln!(writer, "child wins: {}", max_wins).expect("could not write to output");
            writeln!(writer, "child visits: {}", max_visits).expect("could not write to output");
            return mv;
        }

        // If all moves bring to a loss for current player, return a random one.
        writeln!(writer, "All possible moves bring to game over.").expect("could not write to output");
        writeln!(writer, "Returning random move.").expect("could not write to file");
        let mut rng = rand::rng();
        let random_move = moves.choose(&mut rng).unwrap(); // returns a reference
        return *random_move;        
    }

    fn start_search<W: Write>(&mut self, root: &GameState, writer: &mut W) {
        self.increase_generation();

        // Retrieve stats for root.
        // Root cannot have 0 visits because the first UCB value would be NaN.
        let mut root_visits = 1usize;
        let mut root_wins = 0isize;
        {
            let bucket = self.transpositions.get_bucket(root.hash);
            if let Some(root_entry) = bucket.get_entry(root.hash) {
                root_visits = root_entry.get_n_visits(); // Read value from cache.
                root_wins = root_entry.get_n_wins();
            }
        }
        if root_visits < 1 { root_visits = 1; }

        // SEARCH GAME TREE: SELECTION
        for _ in 1..self.iterations_per_move {
            // Selection and Backpropagation to the root.
            root_wins += self.selection(root, root_visits, writer); // Increment value.
            root_visits += 1;
        }

        // BACKPROPAGATION to root.
        let mut increase_collision_in = false;
        let mut increase_collision_out = false;
        let mut is_new_write = false;
        {
            // Add.
            let bucket = self.transpositions.get_bucket(root.hash);
            match bucket.add_entry(root.hash, self.generation, self.generation_bound) {
                Some(CollisionType::OverwrittenIN) => { increase_collision_in = true; }
                Some(CollisionType::OverwrittenOUT) => { increase_collision_out = true; }
                Some(CollisionType::EmptyEntry) => { is_new_write = true; }
                _ => {}
            }
            // Write values.
            if let Some(root_entry) = bucket.get_entry(root.hash) {
                root_entry.set_n_visits(root_visits); // Update value.
                root_entry.set_n_wins(root_wins);
            } else {
                writeln!(writer, "Error: root not added to transpositions table.").expect("could not write to output");
            }
        }
        if increase_collision_in { self.increase_collision_in(); }
        else if increase_collision_out { self.increase_collision_out(); }
        else if is_new_write { self.written_entries += 1; }

        // The following might be useful to evaluate how the algorithm is performing in the current game.
        writeln!(writer, "Number of written entries {}", self.written_entries).expect("could not write to output");
        writeln!(writer, "Number of bad collisions {}", self.overwritten_entries_in).expect("could not write to output");
        writeln!(writer, "Number of good collisions {}\n", self.overwritten_entries_out).expect("could not write to output");

        writeln!(writer, "parent wins: {}", root_wins).expect("could not write to output");
        writeln!(writer, "parent visits: {}", root_visits).expect("could not write to output");
    }

    /// ========================
    ///        SELECTION        
    /// ========================
    /// Returns the result with the perspective of state.player
    fn selection<W: Write>(&mut self, state: &GameState, node_visits: usize, writer: &mut W) -> isize {
        // === TERMINAL CHECKS ===
        match state.check_game_over() {
            Some('D') => return DRAW,
            Some(winner) if winner == state.player => return WIN,
            Some(_) => return LOSS,
            None => {},
        }

        // === HEURISTICS ===
        if state.heuristic_wins_w() {
            return if state.player == 'W' { WIN } else { LOSS };
        }
        if state.player == 'B' {
            if state.heuristic_capture_king().0 {
                return WIN;
            }
        }

        // === SELECTION ===
        let selected_move: [usize; 4];
        let selected_hash: u64;
        let is_expansion_phase;
        let mut best_move_visits = 0;
        {
            // === COMPUTE UCB ===
            let mut moves = Vec::with_capacity(MAX_MOVES);
            state.get_legal_moves(&mut moves, true);

            let mut max_ucb_value = -1.0;
            let mut best_move: Option<[usize; 4]> = None;
            let mut best_move_hash: u64 = 0;
            
            let mut unvisited_moves = Vec::new();

            for m in &moves {
                let child_hash = state.next_hash(m, &self.z_table);
                let child_bucket = self.transpositions.get_bucket(child_hash);
                let mut is_visited = false;
                let mut child_visits = 0;
                let mut child_wins = 0isize;
                // Try to retrieve the child from the Transposition Table.
                if let Some(entry) = child_bucket.get_entry(child_hash) {
                    if entry.get_n_visits() > 0 {
                        is_visited = true;
                        child_visits = entry.get_n_visits();
                        child_wins = entry.get_n_wins();
                    }
                }

                if is_visited {
                    // === UCB FORMULA ===
                    // Q_normalized = ((wins / visits) + 1) / 2
                    // Negate the value because child's win = parent's loss.
                    let q_val = -(child_wins as f64) / (child_visits as f64);
                    let q_norm = (q_val + 1.0) / 2.0;

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
                let idx = rand::rng().random_range(0..unvisited_moves.len());
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
                writeln!(writer, "Error: Selection step has no moves but game over wasn't caught.").expect("could not write to output");
                if state.player == 'W' { return LOSS; }
                else { return WIN; }
            }
        }
        
        // === EXECUTE MOVE ===
        let mut next_state = state.clone();
        next_state.move_piece(&selected_move, &self.z_table, true, writer);
        let result_for_child_node: isize;

        if is_expansion_phase {
            // === EXPANSION ===
            let mut increase_collision_in = false;
            let mut increase_collision_out = false;
            let mut is_new_write = false;
            {
                // Add.
                let bucket = self.transpositions.get_bucket(selected_hash);
                match bucket.add_entry(selected_hash, self.generation, self.generation_bound) {
                    Some(CollisionType::OverwrittenIN) => { increase_collision_in = true; }
                    Some(CollisionType::OverwrittenOUT) => { increase_collision_out = true; }
                    Some(CollisionType::EmptyEntry) => { is_new_write = true; }
                    _ => {}
                }
            }
            if increase_collision_in { self.increase_collision_in(); }
            else if increase_collision_out { self.increase_collision_out(); }
            else if is_new_write { self.written_entries += 1; }

            // === SIMULATION ===
            result_for_child_node = self.simulation(&next_state, writer);
        } else {
            // === RECURSIVE SELECTION ===
            result_for_child_node = self.selection(&next_state, best_move_visits, writer);
        }

        // === BACKPROPAGATION ===
        // Store in the child entry the result for the child.
        {
            let bucket = self.transpositions.get_bucket(selected_hash);
            if let Some(entry) = bucket.get_entry(selected_hash) {
                entry.set_generation(self.generation);
                entry.add_n_visits(1);
                entry.add_n_wins(result_for_child_node);
            } else {
                writeln!(writer, "Error: Entry wasn't found during backpropagation.").expect("could not write to output");
                writeln!(writer, "This means there is a problem with the overwriting policy.").expect("could not write to output");
            }
        }

        // Return result with the perspective of the current node.
        return -result_for_child_node;
    }

    /// =========================
    ///        SIMULATION        
    /// =========================
    /// Returns the result with the perspective of state.player
    fn simulation<W: Write>(&self, state: &GameState, writer: &mut W) -> isize {
        let mut temp_state = state.clone();
        let mut moves = Vec::with_capacity(MAX_MOVES);
        let mut rng = rand::rng();

        // Play random moves until the game is over.
        loop {
            // Check game over.
            if let Some(winner) = temp_state.check_game_over() {
                if winner == 'T' { return DRAW; }
                else if winner == state.player { return WIN; }
                else { return LOSS; }
            }
            // Heuristics.
            if state.heuristic_wins_w() {
                return if state.player == 'W' { WIN } else { LOSS };
            }
            if state.player == 'B' {
                if state.heuristic_capture_king().0 {
                    return WIN;
                }
            }

            // Available moves.
            temp_state.get_legal_moves(&mut moves, true);
            if moves.is_empty() {
                writeln!(writer, "Error: Simulation step has no moves but game over wasn't caught.").expect("could not write to output");
                writeln!(writer, "Applying rule 9 anyways...\n").expect("could not write to output");
                // Current player loses (Rule 9: If a player cannot move, he loses the game).
                // (Combined with Rule 8: If white repeats a move, he loses.)
                if state.player == temp_state.player { return LOSS; }
                else { return WIN; }
            }

            // Random move.
            let random_move = moves.choose(&mut rng).unwrap(); // returns a reference

            // Apply move.
            temp_state.move_piece(random_move, &self.z_table, true, writer);
        }
    }
}
