
pub mod hnefatafl;
pub mod zobrist;
pub mod mcts;
mod parallel_mcts;

use hnefatafl::GameState;
use mcts::MCTS;

enum GameMode {
    HumanVsHuman,
    HumanVsBot,
}

const BOT_SIDE: char = 'B'; // or 'W'
const MCTS_ITERS: usize = 10000000;

fn play_game(mut game: GameState, mode: GameMode) {
    loop {
        game.display();

        if let Some(result) = game.check_game_over() {
            announce_result(result);
            break;
        }

        match mode {
            GameMode::HumanVsHuman => {
                game.get_human_move();
            }

            GameMode::HumanVsBot => {
                if game.player == BOT_SIDE {
                    println!("Bot is thinking...");

                    use crate::parallel_mcts::ParallelMCTS;

                    let threads = std::thread::available_parallelism()
                        .map(|n| n.get())
                        .unwrap_or(4);

                    let pmcts = ParallelMCTS::new(threads, MCTS_ITERS);

                    if let Some(best_move) = pmcts.best_move(&game) {
                        game.move_piece(&best_move);
                    } else {
                        println!("Bot has no legal moves.");
                        break;
                    }
                } else {
                    game.get_human_move();
                }
            }
        }
    }
}

fn announce_result(result: char) {
    match result {
        'W' => println!("White wins!"),
        'B' => println!("Black wins!"),
        'D' => println!("Draw."),
        'E' => println!("Game ended with an error."),
        _ => {}
    }
}

fn main() {
    let mut game = GameState::new();

    println!("Welcome to Hnefatafl!\n");
    println!("Enter positions in the following format:");
    println!("start_row start_col end_row end_col");

    // let winner = loop {
    //     println!();
    //     state.display();
    //
    //     if let Some(player_char) = state.check_game_over() {
    //         break player_char; // Exit the loop and return the winner.
    //     }
    //
    //     state.get_human_move();
    // };
    //
    // println!("\nGame Over! The winner is: {}", winner);
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();

    let mode = match input.trim() {
        "2" => GameMode::HumanVsHuman,
        _ => GameMode::HumanVsBot,
    };

    play_game(game, mode);
}
