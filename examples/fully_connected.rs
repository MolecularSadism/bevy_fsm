//! Fully connected FSM example with zero boilerplate.
//!
//! This demonstrates:
//! - Zero boilerplate FSM with default "allow all transitions" behavior
//! - Just three derives: #[derive(EnumEvent, `FSMTransition`, `FSMState`)]
//! - No manual `FSMTransition` implementation needed
//! - Using `fsm_observer`! macro to register observers in the FSM hierarchy
//! - ALL transitions are allowed (fully connected state graph)
//!
//! Run with: `cargo run --example fully_connected`

use bevy::prelude::*;
use bevy_fsm::{
    fsm_observer, Enter, EnumEvent, Exit, FSMPlugin, FSMState, FSMTransition, StateChangeRequest,
};

fn main() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(FSMPlugin::<GameState>::default());

    // Use fsm_observer! macro to register observers in the FSM hierarchy
    fsm_observer!(app, GameState, on_enter_playing);
    fsm_observer!(app, GameState, on_exit_playing);

    app.add_systems(Startup, setup)
        .add_systems(Update, cycle_states)
        .run();
}

/// Define a simple game state FSM.
///
/// The `FSMTransition` derive gives us "allow all transitions" by default.
#[derive(
    Component, EnumEvent, FSMTransition, FSMState, Reflect, Clone, Copy, Debug, PartialEq, Eq, Hash,
)]
#[reflect(Component)]
enum GameState {
    MainMenu,
    Playing,
    Paused,
    GameOver,
}

/// Setup the game
fn setup(mut commands: Commands) {
    println!("=== Fully Connected FSM Example ===");
    println!("This FSM allows ALL transitions (fully connected graph)");
    println!("Observers registered using fsm_observer! macro\n");

    commands.spawn((GameState::MainMenu, Name::new("Game")));
}

/// Cycle through states to demonstrate the fully connected FSM
#[allow(clippy::match_same_arms)]
fn cycle_states(
    mut commands: Commands,
    query: Query<(Entity, &GameState, &Name)>,
    mut frame: Local<u32>,
) {
    *frame += 1;

    for (entity, &state, name) in query.iter() {
        let next_state = match *frame {
            1 => Some(GameState::Playing),
            2 => Some(GameState::Paused),
            3 => Some(GameState::Playing),
            4 => Some(GameState::GameOver),
            5 => {
                // Demonstrate fully connected: can go directly from GameOver to any state!
                println!("\nDemonstrating fully connected graph:");
                println!("  GameOver -> MainMenu ✓ (would be blocked in a typical FSM)");
                Some(GameState::MainMenu)
            }
            6 => {
                println!("  MainMenu -> GameOver ✓ (skipping intermediate states)");
                Some(GameState::GameOver)
            }
            7 => {
                println!("  GameOver -> Playing ✓ (any transition is valid!)\n");
                Some(GameState::Playing)
            }
            8 => {
                println!("=== Example complete! ===");
                std::process::exit(0);
            }
            _ => None,
        };

        if let Some(next) = next_state {
            if *frame < 50 {
                println!("{name} transitioning: {state:?} -> {next:?}");
            }
            commands.trigger(StateChangeRequest { entity, next });
        }
    }
}

/// Observer: fires when entering Playing state
fn on_enter_playing(_trigger: On<Enter<game_state::Playing>>) {
    println!("  [ENTER Playing] Game started!");
}

/// Observer: fires when exiting Playing state
fn on_exit_playing(_trigger: On<Exit<game_state::Playing>>) {
    println!("  [EXIT Playing] Game stopped!");
}
