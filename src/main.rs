
pub mod hnefatafl;
pub mod zobrist;
pub mod transposition;
pub mod mcts;

use std::fs::File;
use std::{fs, io};
use std::io::{BufWriter, Write};
use std::time::Instant;
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

fn play_game(engine: &mut MCTS, mode: GameMode, bot_side: char, to_file: bool, file_name: &str) {
    let mut game = GameState::new(&engine.z_table);

    // 1. Create the base writer (Stdout or File)
    let writer: Box<dyn Write> = if to_file {
        Box::new(File::create(file_name).expect("Failed to create log file"))
    } else {
        Box::new(io::stdout())
    };

    // 2. Wrap it in a BufWriter for efficiency
    let mut buffered_writer = BufWriter::new(writer);

    let time = Instant::now();
    let mut moves_count = 0;
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
                if game.player == bot_side {
                    writeln!(buffered_writer, "Bot is thinking...").expect("could not write to output");
                    buffered_writer.flush().expect("Flush failed");

                    engine.computer_move(&mut game, &mut buffered_writer);
                } else {
                    game.human_move(&engine.z_table, &mut buffered_writer);
                }
            }

            GameMode::BotVsRandom => {
                if game.player == bot_side {
                    writeln!(buffered_writer, "Bot is thinking...").expect("could not write to output");
                    buffered_writer.flush().expect("Flush failed");

                    engine.computer_move(&mut game, &mut buffered_writer);
                } else {
                    writeln!(buffered_writer, "Playing random move").expect("could not write to output");
                    let mut rng = rand::rng();
                    let mut moves = Vec::with_capacity(mcts::MAX_MOVES);
                    game.get_legal_moves(&mut moves);
                    let random_move = moves.choose(&mut rng).unwrap();
                    game.move_piece(random_move, &engine.z_table, false, &mut buffered_writer);
                }
            }
        }
        moves_count += 1;
    }
    let elapsed_time = Instant::now() - time;
    writeln!(buffered_writer, "Total moves played: {}", moves_count).expect("could not write to output");
    writeln!(buffered_writer, "Total time for game: {}", elapsed_time.as_secs_f64()).expect("could not write to output");
    buffered_writer.flush().expect("Flush failed");
}

fn play_games(mode: GameMode, bot_side: char, game_count: usize, folder_name: &str) {
    fs::create_dir(folder_name).expect("could not create folder");
    let time = Instant::now();
    for i in 0..game_count {
        let mut engine = MCTS::new(0xCAFEBABE, 200_000);
        let file_name = format!("{}/{}.txt", folder_name, i);
        play_game(&mut engine, mode, bot_side,true, &file_name);
    }
    let elapsed_time = Instant::now() - time;
    println!("Total time for {} games: {}", game_count, elapsed_time.as_secs_f64());
}

fn play_bot_vs_bot(white_engine: &mut MCTS, black_engine: &mut MCTS, to_file: bool, file_name: &str) -> char {
    let mut game = GameState::new(&white_engine.z_table);

    let writer: Box<dyn Write> = if to_file {
        Box::new(File::create(file_name).expect("Failed to create log file"))
    } else {
        Box::new(io::stdout())
    };

    let mut buffered_writer = BufWriter::new(writer);
    let time = Instant::now();
    let mut moves_count = 0;
    let mut winner = ' ';
    loop {
        game.display(&mut buffered_writer).expect("Output failed");
        buffered_writer.flush().expect("Flush failed");

        if let Some(result) = game.check_game_over_log(&mut buffered_writer) {
            announce_result(result, &mut buffered_writer).expect("Ending message failed");
            buffered_writer.flush().expect("Flush failed");
            winner = result;
            break;
        }

        writeln!(buffered_writer, "Player {} is thinking...", game.player).expect("Write failed");
        buffered_writer.flush().expect("Flush failed");

        // Alternate engines based on the current player
        if game.player == 'W' {
            white_engine.computer_move(&mut game, &mut buffered_writer);
        } else {
            black_engine.computer_move(&mut game, &mut buffered_writer);
        }

        moves_count += 1;
    }

    let elapsed_time = Instant::now() - time;
    writeln!(buffered_writer, "Total moves: {}", moves_count).ok();
    writeln!(buffered_writer, "Total time: {:.2}s", elapsed_time.as_secs_f64()).ok();
    buffered_writer.flush().ok();
    winner
}

fn play_bot_games(game_count: usize, folder_name: &str) {
    fs::create_dir(folder_name).expect("could not create folder");

    let mut white_wins = 0;
    let mut black_wins = 0;
    println!("{} games will be played with both sides having 200_000 iterations per move", game_count);
    let total_time = Instant::now();
    for i in 0..game_count {
        let mut engine_white = MCTS::new(0xCAFEBABE, 200_000);
        let mut engine_black = MCTS::new(0xDEADBEEF, 200_000);

        let file_name = format!("{}/{}.txt", folder_name, i);

        let result = play_bot_vs_bot(&mut engine_white, &mut engine_black, true, &file_name);
        if result == 'B' { black_wins += 1; } else { white_wins += 1; }
    }
    println!("White won {} games", white_wins);
    println!("Black won {} games", black_wins);
    println!("Finished {} games in {:.2}s", game_count, total_time.elapsed().as_secs_f64());

    let white_iterations = if black_wins > white_wins { 400_000 } else if black_wins == white_wins { 100_000 } else { 200_000 };
    let black_iterations = if black_wins < white_wins { 400_000 } else if black_wins == white_wins { 100_000 } else { 200_000 };
    black_wins = 0;
    white_wins = 0;

    let new_folder_name = format!("white_{}_iter_black_{}_iter", white_iterations, black_iterations);
    fs::create_dir(&new_folder_name).expect("could not create folder");
    
    println!("Another {} games will be played", game_count);
    println!("{} iterations per move for white", white_iterations);
    println!("{} iterations per move for black", black_iterations);

    let total_time = Instant::now();
    for i in 0..game_count {
        let mut engine_white = MCTS::new(0xCAFEBABE, white_iterations);
        let mut engine_black = MCTS::new(0xDEADBEEF, black_iterations);

        let file_name = format!("{}/{}.txt", &new_folder_name, i);

        let result = play_bot_vs_bot(&mut engine_white, &mut engine_black, true, &file_name);
        if result == 'B' { black_wins += 1; } else { white_wins += 1; }
    }
    println!("White won {} games", white_wins);
    println!("Black won {} games", black_wins);
    println!("Finished {} games in {:.2}s", game_count, total_time.elapsed().as_secs_f64());
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
        println!("Starting {} games of random vs engine on white", game_count);
        play_games(mode, 'W', game_count, "random_vs_engine_on_white");
        println!("Starting {} games of random vs engine on black", game_count);
        play_games(mode, 'B', game_count, "random_vs_engine_on_black");
    } else if input.trim() == "4" {
        println!("How many games should be played?");
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();

        let game_count : usize = input.trim().parse().expect("amount of games has to be given as a number");
        println!("Starting {} games of engine vs engine", game_count);
        play_bot_games(game_count, "equal_bots");
    } else {
        let mut engine = MCTS::new(0xCAFEBABE, 200_000);
        play_game(&mut engine, mode, 'W', false, "");
    }
}

