
pub mod hnefatafl;
pub mod zobrist;
pub mod mcts;

use hnefatafl::GameState;
use zobrist::Zobrist;
use transposition::TT_entry;
use transposition::TT_bucket;
use transposition::TT;

fn main() {
    let mut engine = MCTS::new(0xCAFEBABE);

    // Play a game.

    let mut state = GameState::new(&engine.zobrist);

    let human_player: char = 'B';
    let computer_player: char = 'W';

    println!("Welcome to Hnefatafl!\n");
    println!("Enter positions in the following format:");
    println!("start_row start_col end_row end_col");

    let winner = loop {
        println!();
        state.display();
        
        if let Some(player_char) = state.check_game_over() {
            break player_char; // Exit the loop and return the winner.
        }

        if state.player == human_player {
            // Get human move and apply it to the state.
            state.human_move(&zobrist);
            // Update the hash.
            // forse ha senso mettere anche questa parte dentro human_move in
            // modo che la funzioni ritorni l'hash
        } else {
            
        }

        // get_MCTS_move();
    };

    println!("\nGame Over! The winner is: {}", winner);
}
