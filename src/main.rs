
pub mod hnefatafl;
pub mod zobrist;
pub mod transposition;
pub mod mcts;

use hnefatafl::GameState;
use mcts::MCTS;

enum GameMode {
    HumanVsHuman,
    HumanVsBot,
}

const BOT_SIDE: char = 'W'; // or 'W'

fn play_game(mut game: GameState, mut engine: MCTS, mode: GameMode) {
    loop {
        game.display();

        if let Some(result) = game.check_game_over(&engine.z_table) {
            announce_result(result);
            break;
        }

        match mode {
            GameMode::HumanVsHuman => {
                game.human_move(&engine.z_table);
            }

            GameMode::HumanVsBot => {
                if game.player == BOT_SIDE {
                    println!("Bot is thinking...");

                    engine.computer_move(&mut game);
                } else {
                    game.human_move(&engine.z_table);
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

    println!("Welcome to Hnefatafl!\n");
    println!("Enter positions in the following format:");
    println!("start_row start_col end_row end_col");

    let mut engine = MCTS::new(0xCAFEBABE);

    // Play a game.

    let mut game = GameState::new(&engine.z_table);

    let human_player: char = 'B';

    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();

    let mode = match input.trim() {
        "2" => GameMode::HumanVsHuman,
        _ => GameMode::HumanVsBot,
    };

    play_game(game, engine, mode);
}

