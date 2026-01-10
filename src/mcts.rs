use std::collections::HashMap;
use rand::prelude::*;

use crate::hnefatafl::GameState; // adjust path if needed

const EXPLORATION: f32 = 1.414;

/// One node in the Monte Carlo Tree
pub struct MCTSNode {
    pub visits: u32,
    pub wins: f32, // from root player's perspective
    pub parent: Option<usize>,
    pub children: HashMap<[usize; 4], usize>,
    pub untried_moves: Vec<[usize; 4]>,
    pub player_to_move: char,
}

/// The MCTS tree
pub struct MCTS {
    pub nodes: Vec<MCTSNode>,
    pub root_player: char,
}

impl MCTS {
    /// Create a new MCTS tree from a root game state
    pub fn new(root_state: &GameState) -> Self {
        let root_moves = root_state.generate_moves(root_state.player);

        let root = MCTSNode {
            visits: 0,
            wins: 0.0,
            parent: None,
            children: HashMap::new(),
            untried_moves: root_moves,
            player_to_move: root_state.player,
        };

        Self {
            nodes: vec![root],
            root_player: root_state.player,
        }
    }

    /// Run MCTS for a given number of iterations
    pub fn search(&mut self, root_state: &GameState, iterations: usize) {
        for _ in 0..iterations {
            let mut state = root_state.clone();
            let node_idx = self.select_and_expand(&mut state);
            let result = self.rollout(state);
            self.backpropagate(node_idx, result);
        }
    }

    /// Pick the best move after search (highest visit count)
    pub fn best_move(&self) -> Option<[usize; 4]> {
        let root = &self.nodes[0];

        root.children
            .iter()
            .max_by_key(|&(_, &idx)| self.nodes[idx].visits)
            .map(|(&mv, _)| mv)
    }

    // ============================
    // Core MCTS steps
    // ============================

    fn select_and_expand(&mut self, state: &mut GameState) -> usize {
        let mut node_idx = 0;

        loop {
            let node_visits = self.nodes[node_idx].visits;

            // Expansion
            if let Some(mv) = self.nodes[node_idx].untried_moves.pop() {
                state.move_piece(&mv);

                let child_moves = state.generate_moves(state.player);
                let child_idx = self.nodes.len();

                self.nodes.push(MCTSNode {
                    visits: 0,
                    wins: 0.0,
                    parent: Some(node_idx),
                    children: HashMap::new(),
                    untried_moves: child_moves,
                    player_to_move: state.player,
                });

                self.nodes[node_idx].children.insert(mv, child_idx);
                return child_idx;
            }

            // Terminal node
            if self.nodes[node_idx].children.is_empty() {
                return node_idx;
            }

            // Selection (UCT)
            let (&best_move, &best_child) = self.nodes[node_idx]
                .children
                .iter()
                .max_by(|&(_, &a), &(_, &b)| {
                    let ua = self.uct(a, node_visits);
                    let ub = self.uct(b, node_visits);
                    ua.partial_cmp(&ub).unwrap()
                })
                .unwrap();

            state.move_piece(&best_move);
            node_idx = best_child;
        }
    }

    fn rollout(&self, mut state: GameState) -> f32 {
        let mut rng = rand::rng();

        loop {
            if let Some(result) = state.check_game_over() {
                return self.result_to_score(result);
            }

            let moves = state.generate_moves(state.player);
            if moves.is_empty() {
                return 0.0;
            }

            let mv = moves[rng.gen_range(0..moves.len())];
            state.move_piece(&mv);
        }
    }

    fn backpropagate(&mut self, mut node_idx: usize, result: f32) {
        loop {
            let node = &mut self.nodes[node_idx];
            node.visits += 1;
            node.wins += result;

            match node.parent {
                Some(p) => node_idx = p,
                None => break,
            }
        }
    }

    // ============================
    // Helpers
    // ============================

    fn uct(&self, node_idx: usize, parent_visits: u32) -> f32 {
        let node = &self.nodes[node_idx];

        if node.visits == 0 {
            return f32::INFINITY;
        }

        (node.wins / node.visits as f32)
            + EXPLORATION
            * ((parent_visits as f32).ln() / node.visits as f32).sqrt()
    }

    fn result_to_score(&self, result: char) -> f32 {
        match result {
            'W' => {
                if self.root_player == 'W' { 1.0 } else { 0.0 }
            }
            'B' => {
                if self.root_player == 'B' { 1.0 } else { 0.0 }
            }
            'D' => 0.5,
            _ => 0.0,
        }
    }

    pub fn root_stats(&self) -> HashMap<[usize; 4], (u32, f32)> {
        let mut stats = HashMap::new();

        let root = &self.nodes[0];

        for (mv, &child_idx) in &root.children {
            let child = &self.nodes[child_idx];
            stats.insert(*mv, (child.visits, child.wins));
        }

        stats
    }
}
