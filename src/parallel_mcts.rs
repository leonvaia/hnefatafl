use std::thread;
use std::collections::HashMap;
use crate::mcts::MCTS;
use crate::GameState;

#[derive(Default)]
struct MoveStats {
    visits: u32,
    wins: f32,
}

pub struct ParallelMCTS {
    threads: usize,
    iters_per_thread: usize,
}

impl ParallelMCTS {
    pub fn new(threads: usize, total_iters: usize) -> Self {
        Self {
            threads,
            iters_per_thread: total_iters / threads.max(1),
        }
    }

    pub fn best_move(&self, state: &GameState) -> Option<[usize; 4]> {
        let mut handles = Vec::new();

        for _ in 0..self.threads {
            let gs = state.clone();
            let iters = self.iters_per_thread;

            handles.push(thread::spawn(move || {
                let mut mcts = MCTS::new(&gs);
                mcts.search(&gs, iters);
                mcts.root_stats()
            }));
        }

        // Merge results
        let mut merged: HashMap<[usize; 4], MoveStats> = HashMap::new();

        for h in handles {
            let stats = h.join().unwrap();
            for (mv, (visits, wins)) in stats {
                let entry = merged.entry(mv).or_default();
                entry.visits += visits;
                entry.wins += wins;
            }
        }

        // Pick move with most visits
        merged
            .into_iter()
            .max_by_key(|(_, s)| s.visits)
            .map(|(mv, _)| mv)
    }
}
