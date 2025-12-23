
pub mod hnefatafl;
pub mod zobrist;

use hnefatafl::GameState;

fn main() {
    let mut state = GameState::new();

    println!("Welcome to Hnefatafl!\n");
    println!("Enter positions in the following format:");
    println!("start_row start_col end_row end_col");

    let winner = loop {
        println!();
        state.display();
        
        if let Some(player_char) = state.check_game_over() {
            break player_char; // Exit the loop and return the winner.
        }

        state.get_human_move();
    };

    println!("\nGame Over! The winner is: {}", winner);
}
