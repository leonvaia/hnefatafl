
pub mod hnefatafl;
pub mod zobrist;
pub mod transposition;
pub mod mcts;

use std::fs::File;
use std::io;
use std::io::{BufWriter, Write};
use hnefatafl::GameState;
use mcts::MCTS;

enum GameMode {
    HumanVsHuman,
    HumanVsBot,
}

const BOT_SIDE: char = 'W'; // or 'W'

fn play_game(engine: &mut MCTS, mode: GameMode, to_file: bool) {
    let mut game = GameState::new(&engine.z_table);

    // 1. Create the base writer (Stdout or File)
    let writer: Box<dyn Write> = if to_file {
        Box::new(File::create("hnefatafl_log.txt").expect("Failed to create log file"))
    } else {
        Box::new(io::stdout())
    };

    // 2. Wrap it in a BufWriter for efficiency
    let mut buffered_writer = BufWriter::new(writer);

    loop {
        // 3. Pass the buffered writer to display
        game.display(&mut buffered_writer).expect("Output failed");

        // 4. IMPORTANT: Manual flush
        // Because BufWriter holds data until it's full (usually 8KB),
        // you must flush to ensure the board actually appears to the user.
        buffered_writer.flush().expect("Flush failed");

        if let Some(result) = game.check_game_over_log(&mut buffered_writer) {
            announce_result(result);
            break;
        }

        match mode {
            GameMode::HumanVsHuman => {
                game.human_move(&engine.z_table, &mut buffered_writer);
            }

            GameMode::HumanVsBot => {
                if game.player == BOT_SIDE {
                    write!(buffered_writer, "Bot is thinking...").expect("could not write to output");
                    buffered_writer.flush().expect("Flush failed");

                    engine.computer_move(&mut game, &mut buffered_writer);
                } else {
                    game.human_move(&engine.z_table, &mut buffered_writer);
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

    println!("For games between two humans type 2");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();

    let mode = match input.trim() {
        "2" => GameMode::HumanVsHuman,
        _ => GameMode::HumanVsBot,
    };

    play_game(&mut engine, mode, false);
}

