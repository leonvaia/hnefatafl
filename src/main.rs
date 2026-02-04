
pub mod hnefatafl;
pub mod zobrist;
pub mod transposition;
pub mod mcts;

use std::fs::File;
use std::{fs, io};
use std::io::{BufWriter, Write};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use rand::prelude::IndexedRandom;
use hnefatafl::GameState;
use mcts::MCTS;
use crate::mcts::SimulationType;

#[derive(Copy, Clone)]
enum GameMode {
    HumanVsHuman,
    HumanVsBot,
    BotVsRandom,
}

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
                    game.get_legal_moves(&mut moves, false);
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
    fs::create_dir_all(folder_name).expect("could not create folder");

    let time = Instant::now();
    
    for i in 0..game_count {
        let mut engine = MCTS::new(0xCAFEBABE, 200_000, SimulationType::Light);
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
    let winner;
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
        let mut engine_white = MCTS::new(0xCAFEBABE, 200_000, SimulationType::Light);
        let mut engine_black = MCTS::new(0xDEADBEEF, 200_000, SimulationType::Light);

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
        let mut engine_white = MCTS::new(0xCAFEBABE, white_iterations, SimulationType::Light);
        let mut engine_black = MCTS::new(0xDEADBEEF, black_iterations, SimulationType::Light);

        let file_name = format!("{}/{}.txt", &new_folder_name, i);

        let result = play_bot_vs_bot(&mut engine_white, &mut engine_black, true, &file_name);
        if result == 'B' { black_wins += 1; } else { white_wins += 1; }
    }
    println!("White won {} games", white_wins);
    println!("Black won {} games", black_wins);
    println!("Finished {} games in {:.2}s", game_count, total_time.elapsed().as_secs_f64());
}

use rayon::prelude::*; // Ensure this is at the top of your file

fn play_bot_games_parallel(thread_count: usize, game_count: usize) {
    println!("Starting {} parallel threads.", thread_count);
    let folder_name = "equal_bots_parallel";
    fs::create_dir_all(folder_name).expect("could not create folder");

    let total_time = Instant::now();

    let black_wins = Arc::new(Mutex::new(0));
    let white_wins = Arc::new(Mutex::new(0));

    // Use rayon to parallelize the trials
    (0..thread_count).into_par_iter().for_each(|thread_id| {
        let white_iterations = 200_000;
        let black_iterations = 200_000;

        for _ in 0..game_count {
            let white_seed = 0xCAFEBABE + (thread_id as u64 * 100);
            let black_seed = 0xDEADBEEF + (thread_id as u64 * 100);
            let mut engine_white = MCTS::new(white_seed, white_iterations, SimulationType::Light);
            let mut engine_black = MCTS::new(black_seed, black_iterations, SimulationType::Light);

            // Use your existing logic to play the game
            for i in 0..game_count {
                let run_id = thread_count * thread_id + i;
                let file_name = format!("{}/{}", folder_name, run_id);
                let result = play_bot_vs_bot(&mut engine_white, &mut engine_black, true, &file_name);
                if result == 'B' {
                    let mut help = black_wins.lock().unwrap();
                    *help += 1;
                } else {
                    let mut help = white_wins.lock().unwrap();
                    *help += 1;
                }
            }
        }
    });

    println!("White won {} games", white_wins.lock().unwrap());
    println!("Black won {} games", black_wins.lock().unwrap());
    println!("Finished {} games using {} threads in {:.2}s", game_count * thread_count, thread_count, total_time.elapsed().as_secs_f64());
}

fn play_increasing_bot_games(thread_count: usize, folder_name: &str) {
    // Create the base directory
    fs::create_dir_all(folder_name).expect("could not create folder");

    println!("Starting {} parallel threads.", thread_count);
    println!("White's iterations will increase by 1_000_000 every time it loses.");

    let total_time = Instant::now();

    // Use rayon to parallelize the trials
    (0..thread_count).into_par_iter().for_each(|thread_id| {
        let mut white_iterations = 1_000_000;
        let black_iterations = 200_000;
        let mut attempt = 0;

        loop {
            attempt += 1;
            // Distinct seeds for each trial/attempt to ensure variety
            let white_seed = 0xCAFEBABE + (thread_id as u64 * 100) + attempt;
            let black_seed = 0xDEADBEEF + (thread_id as u64 * 100) + attempt;

            let mut engine_white = MCTS::new(white_seed, white_iterations, SimulationType::Light);
            let mut engine_black = MCTS::new(black_seed, black_iterations, SimulationType::Light);

            // Create a unique filename for this specific attempt
            let file_name = format!("{}/trial_{}_iters_{}.txt", folder_name, thread_id, white_iterations);

            // Use your existing logic to play the game
            let result = play_bot_vs_bot(&mut engine_white, &mut engine_black, true, &file_name);

            if result == 'W' {
                // We use a standard println! here; Rayon handles thread-safe stdout locking
                println!("Trial {}: White WON with {} iterations (Attempt {})", thread_id, white_iterations, attempt);
                break;
            } else {
                // Increase difficulty for the next attempt in this thread
                if attempt >= 4 {
                    white_iterations += 2_000_000
                }
                white_iterations += 1_000_000;
            }
        }
    });

    println!("Finished all trials in {:.2}s", total_time.elapsed().as_secs_f64());
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

fn run_simulation_test(
    test_name: &str,
    games_per_side: usize,
    config_a: (SimulationType, u32), // (Type, Iterations)
    config_b: (SimulationType, u32), 
    folder_root: &str
) {
    let mut wins_a = 0;
    let mut wins_b = 0;
    let mut draws = 0;

    println!("============================================================");
    println!("Starting Test: {}", test_name);
    println!("Config A: {:?} @ {} iters", config_a.0, config_a.1);
    println!("Config B: {:?} @ {} iters", config_b.0, config_b.1);
    println!("============================================================");

    let setup_dir = format!("{}/{}", folder_root, test_name.replace(" ", "_"));
    fs::create_dir_all(&setup_dir).expect("Could not create directory");

    // === PHASE 1: A is White, B is Black ===
    println!("Phase 1: A (White) vs B (Black)");
    for i in 0..games_per_side {
        let file_name = format!("{}/game_ph1_{}.txt", setup_dir, i);
        
        // Ensure distinct seeds
        let mut engine_a = MCTS::new(0xCAFE + i as u64, config_a.1, config_a.0);
        let mut engine_b = MCTS::new(0xBEEF + i as u64, config_b.1, config_b.0);

        let result = play_bot_vs_bot(&mut engine_a, &mut engine_b, true, &file_name);
        
        match result {
            'W' => { wins_a += 1; print!("A"); },
            'B' => { wins_b += 1; print!("B"); },
             _  => { draws += 1; print!("D"); },
        }
        io::stdout().flush().unwrap();
    }
    println!("\nPhase 1 Complete.");

    // === PHASE 2: B is White, A is Black ===
    println!("Phase 2: B (White) vs A (Black)");
    for i in 0..games_per_side {
        let file_name = format!("{}/game_ph2_{}.txt", setup_dir, i);
        
        // Ensure distinct seeds
        let mut engine_b = MCTS::new(0xCAFE + 1000 + i as u64, config_b.1, config_b.0);
        let mut engine_a = MCTS::new(0xBEEF + 1000 + i as u64, config_a.1, config_a.0);

        let result = play_bot_vs_bot(&mut engine_b, &mut engine_a, true, &file_name);
        
        match result {
            'W' => { wins_b += 1; print!("B"); },
            'B' => { wins_a += 1; print!("A"); },
             _  => { draws += 1; print!("D"); },
        }
        io::stdout().flush().unwrap();
    }
    println!("\nPhase 2 Complete.");

    println!("------------------------------------------------------------");
    println!("FINAL RESULTS for {}", test_name);
    println!("Config A Wins: {}", wins_a);
    println!("Config B Wins: {}", wins_b);
    println!("Draws:         {}", draws);
    println!("------------------------------------------------------------\n");
}

fn main() {

    println!("Welcome to Hnefatafl!\n");
    println!("Enter positions in the following format:");
    println!("start_row start_col end_row end_col\n");

    println!("type:");
    println!("2 -> human vs human");
    println!("3 -> bot vs random");
    println!("4 -> bot vs bot");
    println!("5 -> bot vs bot (increasing iterations)");
    println!("6 -> bot vs bot (threads)");
    println!("7 -> TO DO: simulation");
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
    } else if input.trim() == "5" {
        println!("How many parallel threads should be run?");
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        let thread_count: usize = input.trim().parse().expect("Invalid number");

        play_increasing_bot_games(thread_count, "parallel_white_increasing");
    } else if input.trim() == "6" {
        println!("How many parallel threads should be run?");
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();

        println!("How many games should be played on each thread?");
        let mut input2 = String::new();
        io::stdin().read_line(&mut input2).unwrap();

        let thread_count : usize = input.trim().parse().expect("amount of threads has to be given as a number");
        let game_count : usize = input2.trim().parse().expect("amount of games has to be given as a number");
        println!("Starting {} threads playing {} games of engine vs engine each", thread_count, game_count);
        play_bot_games_parallel(thread_count, game_count);
    } else if input.trim() == "7" {
        let games_per_side = 1;
        let iteration_tiers = [100_000, 200_000, 400_000];

        println!("Starting Comparative Benchmark Suite");
        println!("Tiers: {:?}", iteration_tiers);

        for &iters in &iteration_tiers {
            println!("\n============================================");
            println!("STARTING TIER: {} Iterations", iters);
            println!("============================================");

            // Create a specific folder for this iteration tier
            // e.g., "benchmark_200000_iters"
            let folder = format!("benchmark_{}_iters", iters);
            std::fs::create_dir_all(&folder).ok();

            // Test 1: Single Light vs Single Heavy
            // (100 games each side)
            run_simulation_test(
                "1_Light_vs_Heavy", 
                games_per_side, 
                (SimulationType::Light, iters), 
                (SimulationType::Heavy, iters), 
                &folder
            );

            // Test 2: Single Light vs Parallel Light (Batch 8)
            // (100 games each side)
            run_simulation_test(
                "2_Light_vs_ParallelLight", 
                games_per_side, 
                (SimulationType::Light, iters), 
                (SimulationType::ParallelLight(8), iters), 
                &folder
            );

            // Test 3: Parallel Light (Batch 8) vs Parallel Heavy (Batch 8)
            // (100 games each side)
            run_simulation_test(
                "3_ParallelLight_vs_ParallelHeavy", 
                games_per_side, 
                (SimulationType::ParallelLight(8), iters), 
                (SimulationType::ParallelHeavy(8), iters), 
                &folder
            );
        }
        println!("\nAll benchmarks complete.");
    } else {
        let mut engine = MCTS::new(0xCAFEBABE, 200_000, SimulationType::Light);
        play_game(&mut engine, mode, 'W', false, "");
    }
}
