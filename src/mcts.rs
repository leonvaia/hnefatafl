//! MCTS algorithm.

use std::io::Write;
use rand::prelude::*;
use rayon::prelude::*;

use crate::zobrist::Zobrist;
use crate::transposition::TT;
use crate::transposition::MAX_ITER;
use crate::transposition::WINS_BITS;
use crate::transposition::CollisionType;
use crate::hnefatafl::GameState;

#[derive(Clone, Copy, Debug)]
pub enum SimulationType {
    Light,                // Single random playout
    Heavy,                // Single hard (heuristic) playout
    ParallelLight(usize), // N random playouts in parallel
    ParallelHeavy(usize), // N hard playouts in parallel
}
impl SimulationType {
    /// Returns the number of threads/simulations run in a single batch.
    #[inline]
    pub fn batch_size(&self) -> usize {
        match *self {
            SimulationType::Light | SimulationType::Heavy => 1,
            SimulationType::ParallelLight(n) | SimulationType::ParallelHeavy(n) => n,
        }
    }
}

/// Negamax values.
const WIN: isize = 1;
const LOSS: isize = -1;
const DRAW: isize = 0;
/// Threshold to consider a node "Solved" in the TT.
const SOLVED_THRESHOLD: usize = 1 << (WINS_BITS - 2);

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

    pub sim_type: SimulationType,
}

impl MCTS {
    pub fn new(seed: u64, iterations_per_move: u32, sim_type: SimulationType) -> Self {
        // To prevent overflow check: 2^VISITS_BITS > 2^GEN_BITS * iterations_per_move * batch_size
        let batch_size = sim_type.batch_size() as u32;
        if iterations_per_move * batch_size >= MAX_ITER {
            panic!("Number of iteration passed might cause an overflow.");
        }

        Self {
            iterations_per_move,
            ucb_const: 1.414,
            generation: 0,
            generation_range: 1,
            generation_bound: 0,
            transpositions: TT::new(),
            z_table: Zobrist::new(seed),
            written_entries: 0,
            overwritten_entries_in: 0,
            overwritten_entries_out: 0,
            sim_type,
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

    /// Mark a node as terminal (SOLVED) in the Transposition Table.
    /// This prevents re-searching a known Win/Loss/Draw.
    /// score is usually 1, 0, or -1.
    fn mark_terminal(&mut self, hash: u64, score: isize) {
        let bucket = self.transpositions.get_bucket(hash);

        // Ensure the entry exists.
        let mut increase_collision_in = false;
        let mut increase_collision_out = false;
        let mut is_new_write = false;
        match bucket.add_entry(hash, self.generation, self.generation_bound) {
            Some(CollisionType::OverwrittenIN) => { increase_collision_in = true; }
            Some(CollisionType::OverwrittenOUT) => { increase_collision_out = true; }
            Some(CollisionType::EmptyEntry) => { is_new_write = true; }
            _ => {}
        }
        if increase_collision_in { self.increase_collision_in(); }
        else if increase_collision_out { self.increase_collision_out(); }
        else if is_new_write { self.written_entries += 1; }

        // Set the values to SOLVED.
        let bucket = self.transpositions.get_bucket(hash);
        if let Some(entry) = bucket.get_entry(hash) {
            entry.set_generation(self.generation);
            
            // We set visits to the threshold. 
            // In UCB, a huge visit count makes the exploration term near zero, 
            // effectively "pinning" the node value.
            entry.set_n_visits(SOLVED_THRESHOLD);

            // We scale the wins to match the ratio of the score.
            entry.set_n_wins(score * (SOLVED_THRESHOLD as isize));
        }
    }
}

// ======================
//     MCTS Algorithm
// ======================
impl MCTS {
    /// Apply engine move to state.
    pub fn computer_move<W: Write>(&mut self, state: &mut GameState, writer: &mut W) {
        let m = self.get_move(state, writer);
        state.move_piece(&m, &self.z_table, true, writer);
    }

    /// Get best move according to MCTS.
    fn get_move<W: Write>(&mut self, root: &GameState, writer: &mut W) -> [usize; 4] {
        // Heuristics.
        #[allow(clippy::collapsible_else_if)]
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

        // === CHOOSE BEST MOVE: the most visited child, considering solved childs ===
        let mut moves = Vec::with_capacity(MAX_MOVES);
        root.get_legal_moves(&mut moves, true);
        
        let mut moves_not_cached = 0;

        let mut best_move: Option<[usize; 4]> = None;
        let mut best_metric = -1.0; // We will use a mixed metric
        let mut best_wins = 0;
        let mut forced_loss_move: Option<[usize; 4]> = None; // Fallback if everything is lost
        
        let mut proven_losses = 0;

        // Consider only moves that do NOT result in a loss for current player.
        for m in &moves {
            let child_hash = root.next_hash(m, &self.z_table);
            let child_bucket = self.transpositions.get_bucket(child_hash);

            let mut visits = 0;
            let mut raw_wins = 0;
            let mut score = 0.5;
            let mut is_solved = false;

            if let Some(entry) = child_bucket.get_entry(child_hash) {
                visits = entry.get_n_visits();
                raw_wins = entry.get_n_wins();

                // Check if solved.
                if visits >= SOLVED_THRESHOLD {
                    is_solved = true;
                    // Normalize score: 1.0 (Opponent Win), 0.0 (Opponent Loss), -1.0 (Draw)
                    // Note: raw_wins for SOLVED is scaled by threshold.
                    if raw_wins > 0 { score = 1.0; }      // Opponent Wins (BAD for us)
                    else if raw_wins == 0 { score = 0.0; } // Opponent Loses (GOOD for us)
                    else { score = -0.5; }                 // Draw
                }
            } else {
                moves_not_cached += 1;
            }

            // CHOICE: 3 cases.

            if is_solved {
                if score == 0.0 {
                    // Case 1: proven win (opponent loses).
                    writeln!(writer, "Found PROVEN WIN move!").ok();
                    return *m;
                } else if score == 1.0 {
                    // Case 2: proven loss (opponent wins).
                    proven_losses += 1;
                    forced_loss_move = Some(*m); // Keep one as a fallback.
                    continue;
                }
            }

            // Case 3: standard choice (most visited child).
            if (visits as f64) > best_metric {
                best_metric = visits as f64;
                best_wins = raw_wins;
                best_move = Some(*m);
            }
        }
        
        writeln!(writer, "Number of child moves not cached: {}", moves_not_cached).expect("could not write to output");
        writeln!(writer, "Proven Losses avoided: {}", proven_losses).ok();

        // Return Best Move.
        if let Some(mv) = best_move {
            writeln!(writer, "child wins: {}", best_wins).expect("could not write to output");
            writeln!(writer, "child visits: {}\n", best_metric).expect("could not write to output");
            return mv;
        }

        // Survival Mode.
        // If we are here, it means EITHER:
        // a) We didn't explore anything (bug?)
        // b) ALL moves are "Proven Losses" (We are checkmated).
        if let Some(loss_mv) = forced_loss_move {
            writeln!(writer, "Resigning... (All moves lead to proven loss)").ok();
            return loss_mv;
        }

        // Failsafe (Random).
        writeln!(writer, "Warning: No evaluated moves found. Returning random.").ok();
        let mut rng = rand::rng();
        *moves.choose(&mut rng).unwrap()
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
        let batch_size = self.sim_type.batch_size();
        for _ in 1..self.iterations_per_move {
            // Selection and Backpropagation to the root.
            root_wins += self.selection(root, root_visits, writer); // Increment value.
            root_visits += batch_size;
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
        writeln!(writer, "\nNumber of written entries {}", self.written_entries).expect("could not write to output");
        writeln!(writer, "Number of bad collisions {}", self.overwritten_entries_in).expect("could not write to output");
        writeln!(writer, "Number of good collisions {}\n", self.overwritten_entries_out).expect("could not write to output");

        writeln!(writer, "parent wins: {}", root_wins).expect("could not write to output");
        writeln!(writer, "parent visits: {}", root_visits).expect("could not write to output");
    }

    // ========================
    //        SELECTION        
    // ========================

    /// Returns the result with the perspective of state.player
    fn selection<W: Write>(&mut self, state: &GameState, node_visits: usize, writer: &mut W) -> isize {
        let batch_size = self.sim_type.batch_size(); // <--- Get batch size
        let scaled_win = WIN * (batch_size as isize);
        let scaled_loss = LOSS * (batch_size as isize);
        let scaled_draw = DRAW * (batch_size as isize);
        
        // === CHECK IF STATE IS ALREADY SOLVED IN TT ===
        // If we found this state in the TT with high visit count,
        // it means we already determined it is terminal in a previous path/search.
        {
            let bucket = self.transpositions.get_bucket(state.hash);
            if let Some(entry) = bucket.get_entry(state.hash)
            && entry.get_n_visits() >= SOLVED_THRESHOLD {
                // RETURN SCALED SCORE
                return if entry.get_n_wins() > 0 { scaled_win }
                        else if entry.get_n_wins() < 0 { scaled_loss }
                        else { scaled_draw };
            }
        }
        
        // === TERMINAL & HEURISTICS CHECKS ===
        let mut terminal_score = None;

        // Game over.
        if let Some(winner) = state.check_game_over() {
            let score = match winner {
                'D' => scaled_draw,
                w if w == state.player => scaled_win,
                _ => scaled_loss,
            };

            // IMPORTANT: If loss is due to Repetition, it is context-dependent (history).
            // We should NOT cache it as "globally terminal" in the TT (which uses context-free hash).
            if !state.repetition {
                terminal_score = Some(score);
            } else {
                return score; // Return result, but do not mark TT as Solved.
            }
        }

        // Heuristics for White.
        if state.heuristic_wins_w() {
            terminal_score = Some(if state.player == 'W' { scaled_win } else { scaled_loss });
        }

        // Heuristics for Black.
        if state.player == 'B'
        && state.heuristic_capture_king().0 {
            terminal_score = Some(scaled_win);
        }

        // If found a terminal state, mark it and return.
        if let Some(score) = terminal_score {
            // Note: Mark terminal uses standard 1/-1, that's fine, it handles scaling internally via SOLVED_THRESHOLD
            // But we must return the scaled score up the stack
            self.mark_terminal(state.hash, if score > 0 { WIN } else { LOSS });
            return score;
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
                if let Some(entry) = child_bucket.get_entry(child_hash)
                && entry.get_n_visits() > 0 {
                    is_visited = true;
                    child_visits = entry.get_n_visits();
                    child_wins = entry.get_n_wins();
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
                        best_move = Some(*m);
                        best_move_hash = child_hash;
                        best_move_visits = child_visits;
                    }
                } else {
                    // If unvisited, store it for later decision.
                    unvisited_moves.push((*m, child_hash));
                }
            }

            // === CHOICE ===
            if !unvisited_moves.is_empty() {
                // Pick random unvisited child.
                let idx = rand::rng().random_range(0..unvisited_moves.len());
                let (m, h) = unvisited_moves[idx];
                selected_move = m;
                selected_hash = h;
                is_expansion_phase = true;
            
            } else if let Some(m) = best_move {
                selected_move = m;
                selected_hash = best_move_hash;
                is_expansion_phase = false;
            } else {
                // No moves available. Should be caught by terminal check.
                // Result: The current player loses immediately.
                // writeln!(writer, "Error: Selection step has no moves but game over wasn't caught.").expect("could not write to output");
                
                // Define the result (LOSS for the current player)
                let batch_size = self.sim_type.batch_size();
                let scaled_score = LOSS * (batch_size as isize);

                // Mark this node as SOLVED in the Transposition Table
                // We pass the unscaled 'LOSS' (-1) because mark_terminal handles the scaling internally.
                self.mark_terminal(state.hash, LOSS);

                return scaled_score;
            }
        }
        
        // === EXECUTE MOVE ===
        let mut next_state = *state;
        next_state.move_piece(&selected_move, &self.z_table, true, writer);
        let result_for_child_node: isize;

        let visits_added = batch_size;

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
            let sim_score = match self.sim_type {
                SimulationType::Light => self.simulation(&next_state),
                SimulationType::Heavy => self.simulation_hard(&next_state),
                SimulationType::ParallelLight(batch) => self.simulation_parallel(&next_state, batch, false),
                SimulationType::ParallelHeavy(batch) => self.simulation_parallel(&next_state, batch, true),
            };

            result_for_child_node = sim_score;
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
                entry.add_n_visits(visits_added);
                entry.add_n_wins(result_for_child_node);
            } else {
                writeln!(writer, "Error: Entry wasn't found during backpropagation.").expect("could not write to output");
                writeln!(writer, "This means there is a problem with the overwriting policy.").expect("could not write to output");
            }
        }

        // === SOLVER PROPAGATION ===
        // Check if the child we just explored (in the recursive selection) is now SOLVED.
        let child_bucket = self.transpositions.get_bucket(selected_hash);
        
        if let Some(e) = child_bucket.get_entry(selected_hash) {
            let child_visits = e.get_n_visits();
            let child_wins = e.get_n_wins();

            // Check if child is solved.
            if child_visits >= SOLVED_THRESHOLD {
                // Case 1: Child is a PROVEN LOSS for the opponent (score == 0).
                // If the opponent loses in that state, it means we WIN by making this move.
                // We assume 0 represents LOSS based on your const definitions.
                if child_wins < 0 {
                    self.mark_terminal(state.hash, WIN);
                    // Since we found a winning move, we return WIN immediately.
                    return scaled_win; 
                }

                // Case 2: Child is a PROVEN WIN for the opponent.
                // This means 'selected_move' is a blunder.
                // WE DO NOT MARK THE PARENT AS A LOSS HERE. 
                // We would have to check ALL siblings to prove the parent is a loss.
                // However, the UCB formula will naturally avoid this move in future 
                // iterations because its value will be poor.
            }
        }

        // Return result with the perspective of the current node.
        -result_for_child_node
    }

    // =========================
    //        SIMULATION        
    // =========================
    
    /// Returns the result with the perspective of state.player
    fn simulation(&self, state: &GameState) -> isize {
        let mut temp_state = *state;
        let mut moves = Vec::with_capacity(MAX_MOVES);
        let mut rng = rand::rng();

        let mut sink = std::io::sink();

        // Play random moves until the game is over.
        loop {
            // Check game over.
            if let Some(winner) = temp_state.check_game_over() {
                if winner == 'D' { return DRAW; }
                else if winner == state.player { return WIN; }
                else { return LOSS; }
            }
            // Heuristics.
            if state.heuristic_wins_w() {
                return if state.player == 'W' { WIN } else { LOSS };
            }
            if state.player == 'B' 
            && state.heuristic_capture_king().0 {
                return WIN;
            }

            // Available moves.
            temp_state.get_legal_moves(&mut moves, true);
            if moves.is_empty() {
                // writeln!(writer, "Error: Simulation step has no moves but game over wasn't caught.").expect("could not write to output");
                // writeln!(writer, "Applying rule 9 anyways...\n").expect("could not write to output");
                // Current player loses (Rule 9: If a player cannot move, he loses the game).
                // (Combined with Rule 8: If white repeats a move, he loses.)
                if state.player == temp_state.player { return LOSS; }
                else { return WIN; }
            }

            // Random move.
            let random_move = moves.choose(&mut rng).unwrap(); // returns a reference

            // Apply move.
            temp_state.move_piece(random_move, &self.z_table, true, &mut sink);
        }
    }

    fn simulation_hard(&self, state: &GameState) -> isize {
        let mut temp_state = *state;
        let mut moves = Vec::with_capacity(MAX_MOVES);
        let mut capture_moves = Vec::with_capacity(16);
        let mut rng = rand::rng();

        let mut sink = std::io::sink();

        // Play random moves until the game is over.
        loop {
            // Check game over.
            if let Some(winner) = temp_state.check_game_over() {
                if winner == 'D' { return DRAW; }
                else if winner == state.player { return WIN; }
                else { return LOSS; }
            }
            // Heuristics (instant wins).
            if state.heuristic_wins_w() {
                return if state.player == 'W' { WIN } else { LOSS };
            }
            if state.player == 'B'
            && state.heuristic_capture_king().0 {
                return WIN;
            }

            // Available moves.
            temp_state.get_legal_moves(&mut moves, true);
            if moves.is_empty() {
                // writeln!(writer, "Error: Simulation step has no moves but game over wasn't caught.").expect("could not write to output");
                // writeln!(writer, "Applying rule 9 anyways...\n").expect("could not write to output");
                // Current player loses (Rule 9: If a player cannot move, he loses the game).
                // (Combined with Rule 8: If white repeats a move, he loses.)
                if state.player == temp_state.player { return LOSS; }
                else { return WIN; }
            }

            // === HARD PLAYOUTS ===
            capture_moves.clear();

            // Filter for captures
            for m in &moves {
                if temp_state.is_capture_move(m) {
                    capture_moves.push(*m);
                }
            }

            let selected_move = if !capture_moves.is_empty() {
                // 80% chance to pick a capture move, 20% random (Exploration)
                if rng.random_bool(0.8) {
                    capture_moves.choose(&mut rng).unwrap()
                } else {
                    moves.choose(&mut rng).unwrap()
                }
            } else {
                // No captures available, play random
                moves.choose(&mut rng).unwrap()
            };

            // Apply move.
            temp_state.move_piece(selected_move, &self.z_table, true, &mut sink);
        }
    }

    /// Run multiple simulations in parallel using Rayon.
    /// Returns: (Total Score, Count of Simulations)
    fn simulation_parallel(&self, state: &GameState, batch_size: usize, use_hard: bool) -> isize {
        // Parallel iterator using Rayon
        let total_score: isize = (0..batch_size)
            .into_par_iter()
            .map(|_| {
                if use_hard {
                    self.simulation_hard(state)
                } else {
                    self.simulation(state)
                }
            })
            .sum();

        total_score
    }
}
