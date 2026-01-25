
pub mod hnefatafl;
pub mod zobrist;
pub mod transposition;
pub mod mcts;

use std::fs::File;
use std::{fs, io};
use std::io::{BufWriter, Write};
use rand::prelude::IndexedRandom;
use hnefatafl::GameState;
use mcts::MCTS;

#[derive(Copy, Clone)]
enum GameMode {
    HumanVsHuman,
    HumanVsBot,
    BotVsRandom,
}

const BOT_SIDE: char = 'W'; // or 'W'

fn play_game(engine: &mut MCTS, mode: GameMode, to_file: bool, file_name: &str) {
    let mut game = GameState::new(&engine.z_table);

    // 1. Create the base writer (Stdout or File)
    let writer: Box<dyn Write> = if to_file {
        Box::new(File::create(file_name).expect("Failed to create log file"))
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
            announce_result(result, &mut buffered_writer).expect("could not writer ending message");
            buffered_writer.flush().expect("Flush failed");
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

            GameMode::BotVsRandom => {
                if game.player == BOT_SIDE {
                    write!(buffered_writer, "Bot is thinking...").expect("could not write to output");
                    buffered_writer.flush().expect("Flush failed");

                    engine.computer_move(&mut game, &mut buffered_writer);
                } else {
                    writeln!(buffered_writer, "Playing random move").expect("could not write to output");
                    let mut rng = rand::rng();
                    let mut moves = Vec::with_capacity(mcts::MAX_MOVES);
                    game.get_legal_moves(&mut moves);
                    let random_move = moves.choose(&mut rng).unwrap();
                    game.move_piece(random_move, &engine.z_table);
                }
            }
        }
    }
}

fn play_games(mut engine: &mut MCTS, mode: GameMode, game_count: usize, folder_name: &str) {
    fs::create_dir(folder_name).expect("could not create folder");
    for i in 0..game_count {
        let file_name = folder_name.to_string() + "/" + &i.to_string();
        play_game(&mut engine, mode,true, &file_name);
    }
}

fn announce_result<W: Write>(result: char, writer: &mut W) -> io::Result<()> {
    match result {
        'W' => writeln!(writer, "White wins!")?,
        'B' => writeln!(writer, "Black wins!")?,
        'D' => writeln!(writer, "Draw.")?,
        'E' => writeln!(writer, "Game ended with an error.")?,
        _ => {}
    }
    Ok(())
}

fn main() {

    println!("Welcome to Hnefatafl!\n");
    println!("Enter positions in the following format:");
    println!("start_row start_col end_row end_col");

    let mut engine = MCTS::new(0xCAFEBABE);

    println!("For games between two humans type 2");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    let mode = match input.trim() {
        "3" => GameMode::BotVsRandom,
        "2" => GameMode::HumanVsHuman,
        _ => GameMode::HumanVsBot,
    };

    if input.trim() == "3" {
        println!("How many games should be played?");
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();

        let game_count : usize = input.trim().parse().expect("amount of games has to be given as a number");
        play_games(&mut engine, mode, game_count, "random_vs_engine_on_white");
    } else {
        play_game(&mut engine, mode, false, "");
    }
}

