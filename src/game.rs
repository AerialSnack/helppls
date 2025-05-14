use bevy::{prelude::*, render::camera::ScalingMode, utils::HashMap};
use bevy_rapier2d::{render, prelude::*};
use crate::GameState;
use bevy_matchbox::prelude::*;
use bevy_ggrs::*;
use bevy_ggrs::prelude::{SessionBuilder, GgrsEvent};
use serde::{Deserialize, Serialize};

const INPUT_LEFT: u8 = 1 << 0;
const INPUT_RIGHT: u8 = 1 << 1;
const INPUT_JUMP: u8 = 1 << 2;

type Config = bevy_ggrs::GgrsConfig<u8, PeerId>;
pub struct GamePlugin;

#[derive(Default, Reflect, Hash, Resource, Copy, Clone)]
pub struct FrameCount {
    pub frame: u32,
}

#[derive(Component)]
struct WaitingText;

#[derive(Component)]
struct Player {
    handle: usize
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Input {
    pub input: u8,
}

#[derive(Component)]
pub struct Boundary;

#[derive(Resource, Default, Copy, Clone)]
struct JumpState {
    was_pressed: bool,
}

#[derive(Resource, Default)]
struct LastConfirmedFrame(u32);

#[derive(Resource, Default)]
struct GameStateChecksum {
    last_checksum: u64,
    desync_detected: bool,
}

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
         GgrsPlugin::<Config>::default(),
         RapierPhysicsPlugin::<NoUserData>::default(),
        ))
            .rollback_component_with_clone::<Transform>()
            .rollback_component_with_copy::<Velocity>()
            .rollback_resource_with_copy::<FrameCount>()
            .rollback_resource_with_copy::<JumpState>()
            .insert_resource(Time::<Fixed>::from_hz(120.0))
            .set_rollback_schedule_fps(120)
            .insert_resource(FrameCount { frame: 0 })
            .insert_resource(JumpState::default())
            .insert_resource(LastConfirmedFrame(0))
            .insert_resource(GameStateChecksum::default())
            .add_systems(OnEnter(GameState::InGame), (setup, spawn_boundary, spawn_players, start_matchbox_socket))
            .add_systems(Update, wait_for_players.run_if(in_state(GameState::InGame)))
            .add_systems(ReadInputs, read_local_inputs.run_if(in_state(GameState::InGame)))
            .add_systems(GgrsSchedule, (
                move_players.run_if(in_state(GameState::InGame)),
                increase_frame_system.run_if(in_state(GameState::InGame)),
                sync_test.run_if(in_state(GameState::InGame))
            ).chain())
            .add_plugins(RapierDebugRenderPlugin::default());
    }
}

pub fn setup(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        OrthographicProjection {
            scaling_mode: ScalingMode::Fixed { height: 900.0, width: 1600.0 },
            near: -1000.0,
            far: 1000.0,
            viewport_origin: Vec2::ZERO,
            scale: 1.0,
            area: Rect::default(),
        },
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));

    // Spawn waiting text
    commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            WaitingText,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("Waiting for other player..."),
                TextFont {
                    font_size: 30.0,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
}

fn start_matchbox_socket(mut commands: Commands) {
    let room_url = "ws://ec2-54-67-37-240.us-west-1.compute.amazonaws.com:3536/extreme_bevy?next=2";
    info!("connecting to matchbox server: {room_url}");
    commands.insert_resource(MatchboxSocket::new_unreliable(room_url));
}

fn wait_for_players(
    mut socket: ResMut<MatchboxSocket>, 
    mut commands: Commands,
    waiting_text: Query<Entity, With<WaitingText>>,
) {
    if socket.get_channel(0).is_err() {
        return; // we've already started
    }

    // Check for new connections
    socket.update_peers();
    let players = socket.players();

    let num_players = 2;
    if players.len() < num_players {
        return; // wait for more players
    }

    info!("All peers have joined, going in-game");

    // Remove waiting text
    if let Ok(entity) = waiting_text.get_single() {
        commands.entity(entity).despawn_recursive();
    }

    // create a GGRS P2P session
    let mut session_builder = SessionBuilder::<Config>::new()
        .with_num_players(num_players)
        .with_desync_detection_mode(ggrs::DesyncDetection::On { interval: 10 })
        .with_input_delay(2);

    for (i, player) in players.into_iter().enumerate() {
        session_builder = session_builder
            .add_player(player, i)
            .expect("failed to add player");
    }

    // move the channel out of the socket (required because GGRS takes ownership of it)
    let channel = socket.take_channel(0).unwrap();

    // start the GGRS session
    let ggrs_session = session_builder
        .start_p2p_session(channel)
        .expect("failed to start session");

    commands.insert_resource(bevy_ggrs::Session::P2P(ggrs_session));
}


// ANYTHING HERE THAT GETS CHANGED IN GAME SHOULD BE INCLUDED IN THE ROLLBACK_COMPONENT_WITH_CLONE
fn spawn_players(mut commands: Commands) {
    
    // Player 1
    commands.spawn((
        Player { handle: 0 },
        Transform::from_xyz(600.0, 450.0, 0.0),
        RigidBody::Dynamic,
        Collider::cuboid(16.0, 16.0),
        Sprite::from_color(Color::srgb(1.0, 0.0, 0.0), Vec2::new(32.0, 32.0)),
        GravityScale(200.0),
        Velocity::zero(),
        LockedAxes::ROTATION_LOCKED,
    ))
        .add_rollback();

    // Player 2
    commands.spawn((
        Player { handle: 1 },
        Transform::from_xyz(1000.0, 450.0, 0.0),
        RigidBody::Dynamic,
        Collider::cuboid(16.0, 16.0),
        Sprite::from_color(Color::srgb(0.0, 0.0, 1.0), Vec2::new(32.0, 32.0)),
        GravityScale(200.0),
        Velocity::zero(),
        LockedAxes::ROTATION_LOCKED,
    ))
    .add_rollback();
}

pub fn spawn_boundary(mut commands: Commands, asset_server: Res<AssetServer>) {
    let arena_width = 1600.0;
    let arena_height = 900.0;
    let boundary_thickness = 50.0;

    commands.spawn((
        Transform::from_xyz(arena_width / 2.0, boundary_thickness / 2.0, 0.0),
        Collider::cuboid(arena_width / 2.0, boundary_thickness / 2.0),
        RigidBody::Fixed,
        Boundary,
        Sprite {
            custom_size: Some(Vec2::new(arena_width, boundary_thickness)),
            ..Sprite::from_image(asset_server.load("boundary.png"))
        },
    ));
}

fn read_local_inputs(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    local_players: Res<LocalPlayers>,
    mut jump_state: ResMut<JumpState>
) {
    let mut local_inputs = HashMap::new();

    for handle in &local_players.0 {
        let mut input = 0u8;

        if keys.any_pressed([KeyCode::ArrowLeft, KeyCode::KeyA]) {
            input |= INPUT_LEFT;
        }
        if keys.any_pressed([KeyCode::ArrowRight, KeyCode::KeyD]) {
            input |= INPUT_RIGHT;
        }
        
        let is_jump_pressed = keys.any_pressed([KeyCode::Space, KeyCode::KeyW]);
        if is_jump_pressed && !jump_state.was_pressed {
            input |= INPUT_JUMP;
        }
        jump_state.was_pressed = is_jump_pressed;

        local_inputs.insert(*handle, input);
    }

    commands.insert_resource(LocalInputs::<Config>(local_inputs));
}

fn move_players(
    mut players: Query<(&mut Velocity, &Player)>, 
    inputs: Res<PlayerInputs<Config>>,
) {
    for (mut velocity, player) in &mut players {
        let (input, _) = inputs[player.handle];
        
        if input & INPUT_LEFT != 0 {
            velocity.linvel.x = -600.0;
        }
        else if input & INPUT_RIGHT != 0 {
            velocity.linvel.x = 600.0;
        }
        else {
            velocity.linvel.x = 0.0;
        }
        
        if input & INPUT_JUMP != 0 {
            velocity.linvel.y = 600.0;
        }
    }
}

#[allow(dead_code)]
pub fn increase_frame_system(mut frame_count: ResMut<FrameCount>) {
    frame_count.frame += 1;
}

// this doesn't even work
fn print_events_system(mut session: ResMut<Session<Config>>) {
    match session.as_mut() {
        Session::P2P(s) => {
            for event in s.events() {
                match event {
                    GgrsEvent::Disconnected { .. } | GgrsEvent::NetworkInterrupted { .. } => {
                        warn!("GGRS event: {event:?}")
                    }
                    GgrsEvent::DesyncDetected { .. } => error!("GGRS event: {event:?}"),
                    _ => info!("GGRS event: {event:?}"),
                }
            }
        }
        _ => panic!("This example focuses on p2p."),
    }
}

fn sync_test(
    players: Query<(&Transform, &Velocity), With<Player>>,
    frame_count: Res<FrameCount>,
    mut checksum: ResMut<GameStateChecksum>,
    session: Res<Session<Config>>,
) {
    if let Session::P2P(session) = session.as_ref() {
        // Calculate checksum from player positions and velocities
        let mut current_checksum = 0u64;
        for (transform, velocity) in &players {
            // Use position and velocity to create a deterministic checksum
            current_checksum = current_checksum.wrapping_add(
                (transform.translation.x * 1000.0) as u64 ^
                (transform.translation.y * 1000.0) as u64 ^
                (velocity.linvel.x * 1000.0) as u64 ^
                (velocity.linvel.y * 1000.0) as u64
            );
        }

        // Log checksum every 60 frames
        if frame_count.frame % 60 == 0 {
            println!("Frame {} - Checksum: {}", frame_count.frame, current_checksum);
        }

        // Check for desync
        if checksum.last_checksum != 0 && checksum.last_checksum != current_checksum {
            if !checksum.desync_detected {
                error!("DESYNC DETECTED at frame {}! Local checksum: {}, Remote checksum: {}", 
                    frame_count.frame, current_checksum, checksum.last_checksum);
                checksum.desync_detected = true;
            }
        } else {
            checksum.desync_detected = false;
        }

        checksum.last_checksum = current_checksum;
    }
}

// TODO:
// FIX SLIGHT DESYNC OVER TIME
// USE SPRITES FOR PLAYERS
// MAKE PLAYER SPAWNING ABSTRACTABLE + SCALEABLE (one iterative player spawn system)
// IMPLEMENT COLLISION GROUPS - PLAYERS SHOULDN'T COLLIDE WITH EACH OTHER
// IMPLEMENT CHECKSUM FOR ROLLBACK: EXAMPLE (https://github.com/gschup/bevy_ggrs_demo/blob/main/src/checksum.rs)
// IMPLEMENT DEBUGGING USING CHECKSUM
// MAKE JUMP ONLY WORK ON GROUND
// ADD A DOUBLE JUMP
// ADD WALLS AND CEILING
// ADD WALL JUMP