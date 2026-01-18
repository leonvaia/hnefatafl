
pub mod hnefatafl;
pub mod zobrist;
pub mod transposition;
pub mod mcts;

use hnefatafl::GameState;
use mcts::MCTS;

fn main() {
    let mut engine = MCTS::new(0xCAFEBABE);

    // Play a game.

    let mut state = GameState::new(&engine.z_table);

    let human_player: char = 'B';

    println!("Welcome to Hnefatafl!\n");
    println!("Enter positions in the following format:");
    println!("start_row start_col end_row end_col");

    let winner = loop {
        println!();
        state.display();
        
        if let Some(player_char) = state.check_game_over(&engine.z_table) {
            break player_char; // Exit the loop and return the winner.
        }

        if state.player == human_player {
            // Get human move and apply it to the state.
            state.human_move(&engine.z_table);
        } else {
            engine.computer_move(&mut state);
        }
    };

    println!("\nGame Over! The winner is: {}", winner);
}
