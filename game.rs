use bevy::{prelude::*, render::camera::ScalingMode, utils::HashMap};
use bevy_matchbox::prelude::*;
use bevy_ggrs::*;
use bevy_ggrs::prelude::SessionBuilder;
use ggrs::GgrsEvent;
use crate::GameState;
use avian2d::prelude::*;

pub struct GamePlugin;

#[derive(Component)]
struct WaitingText;

#[derive(Component)]
struct Floor;

#[derive(Component)]
struct Player {
    handle: usize
}

type Config = bevy_ggrs::GgrsConfig<u8, PeerId>;

const INPUT_JUMP: u8 = 1 << 0;
const INPUT_LEFT: u8 = 1 << 1;
const INPUT_RIGHT: u8 = 1 << 2;
const INPUT_STRIKE: u8 = 1 << 3;

const GRAVITY: f32 = -9.81 * 5.0;
const PLAYER_MASS: f32 = 1.0;
const PLAYER_RESTITUTION: f32 = 0.3;
const FLOOR_SIZE: Vec2 = Vec2::new(20.0, 0.5);
const FLOOR_POSITION: Vec3 = Vec3::new(0.0, -4.0, 0.0);
const PLAYER_MOVE_FORCE: f32 = 15.0;
const PLAYER_JUMP_FORCE: f32 = 15.0;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(GgrsPlugin::<Config>::default())
            .add_plugins(PhysicsPlugins::default())
            .insert_resource(Gravity(Vec2::new(0.0, GRAVITY)))
            .rollback_component_with_clone::<Transform>()
            .rollback_component_with_clone::<RigidBody>()
            .rollback_component_with_clone::<Collider>()
            .rollback_component_with_clone::<LinearVelocity>()
            .rollback_component_with_clone::<AngularVelocity>()
            .rollback_component_with_clone::<GravityScale>()
            .rollback_component_with_clone::<ColliderDensity>()
            .rollback_component_with_clone::<Friction>()
            .rollback_component_with_clone::<Restitution>()
            .rollback_component_with_clone::<LockedAxes>()
            .rollback_component_with_clone::<TransformInterpolation>()
            .add_systems(OnEnter(GameState::InGame), (setup, spawn_players, spawn_floor, start_matchbox_socket))
            .add_systems(Update, wait_for_players.run_if(in_state(GameState::InGame)))
            .add_systems(Update, handle_ggrs_events.run_if(in_state(GameState::InGame)))
            .add_systems(ReadInputs, read_local_inputs.run_if(in_state(GameState::InGame)))
            .add_systems(GgrsSchedule, move_players.run_if(in_state(GameState::InGame)));
    }
}

fn handle_ggrs_events(mut session: Option<ResMut<Session<Config>>>) {
    if let Some(mut session) = session {
        if let Session::P2P(p2p_session) = session.as_mut() {
            for event in p2p_session.events() {
                match event {
                    GgrsEvent::DesyncDetected { .. } => {
                        error!("DESYNC DETECTED: Game states don't match between players!");
                    }
                    GgrsEvent::WaitRecommendation { .. } => {
                        info!("Wait recommendation received from GGRS");
                    }
                    GgrsEvent::NetworkInterrupted { .. } => {
                        warn!("Network interrupted");
                    }
                    GgrsEvent::NetworkResumed { .. } => {
                        info!("Network resumed");
                    }
                    GgrsEvent::Disconnected { .. } => {
                        warn!("Player disconnected");
                    }
                    _ => {}
                }
            }
        }
    }
}

fn read_local_inputs(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    local_players: Res<LocalPlayers>,
) {
    let mut local_inputs = HashMap::new();

    for handle in &local_players.0 {
        let mut input = 0u8;

        if keys.any_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) {
            input |= INPUT_JUMP;
        }
        if keys.any_pressed([KeyCode::ArrowLeft, KeyCode::KeyA]) {
            input |= INPUT_LEFT
        }
        if keys.any_pressed([KeyCode::ArrowRight, KeyCode::KeyD]) {
            input |= INPUT_RIGHT;
        }
        if keys.any_pressed([KeyCode::Space, KeyCode::Enter]) {
            input |= INPUT_STRIKE;
        }

        local_inputs.insert(*handle, input);
    }

    commands.insert_resource(LocalInputs::<Config>(local_inputs));
}

fn setup(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: 10.,
            },
            ..OrthographicProjection::default_2d()
        },
        TransformInterpolation,
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

fn spawn_floor(mut commands: Commands) {
    commands.spawn((
        Floor,
        Sprite {
            color: Color::srgb(0.5, 0.5, 0.5),
            custom_size: Some(FLOOR_SIZE),
            ..default()
        },
        Transform::from_translation(FLOOR_POSITION),
        GlobalTransform::default(),
        Visibility::default(),
        InheritedVisibility::default(),
        ViewVisibility::default(),
        RigidBody::Static,
        Collider::rectangle(FLOOR_SIZE.x, FLOOR_SIZE.y),
        Friction::new(0.7),
        Restitution::new(0.2),
        TransformInterpolation,
    )).add_rollback();
}

fn spawn_players(mut commands: Commands) {
    // Player 1
    commands
        .spawn((
            Player { handle: 0 },
            Sprite {
                color: Color::srgb(0., 0.47, 1.),
                custom_size: Some(Vec2::new(PLAYER_SIZE, PLAYER_SIZE)),
                ..default()
            },
            Transform::from_translation(Vec3::new(-2., 2., 0.)),
            GlobalTransform::default(),
            Visibility::default(),
            InheritedVisibility::default(),
            ViewVisibility::default(),
            RigidBody::Dynamic,
            Collider::rectangle(PLAYER_SIZE, PLAYER_SIZE),
            ColliderDensity(PLAYER_MASS),
            Restitution::new(PLAYER_RESTITUTION),
            Friction::new(0.2),
            GravityScale(1.0),
            LockedAxes::ROTATION_LOCKED,
            TransformInterpolation,
        ))
        .add_rollback();

    // Player 2
    commands
        .spawn((
            Player { handle: 1 },
            Sprite {
                color: Color::srgb(0., 0.4, 0.),
                custom_size: Some(Vec2::new(PLAYER_SIZE, PLAYER_SIZE)),
                ..default()
            },
            Transform::from_translation(Vec3::new(2., 2., 0.)),
            GlobalTransform::default(),
            Visibility::default(),
            InheritedVisibility::default(), 
            ViewVisibility::default(),
            RigidBody::Dynamic,
            Collider::rectangle(PLAYER_SIZE, PLAYER_SIZE),
            ColliderDensity(PLAYER_MASS),
            Restitution::new(PLAYER_RESTITUTION),
            Friction::new(0.2),
            GravityScale(1.0),
            LockedAxes::ROTATION_LOCKED,
            TransformInterpolation,
        ))
        .add_rollback();
}

fn start_matchbox_socket(mut commands: Commands) {
    let room_url = "xxx";
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
        .with_input_delay(2);

    // Enable desync detection
    session_builder = session_builder.with_desync_detection_mode(ggrs::DesyncDetection::On { interval: 10 });

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

fn move_players(
    mut players: Query<(&mut LinearVelocity, &Transform, &Player)>,
    inputs: Res<PlayerInputs<Config>>,
) {
    for (mut velocity, transform, player) in &mut players {
        let (input, _) = inputs[player.handle];
        let mut movement = Vec2::ZERO;

        let ground_y_pos = FLOOR_POSITION.y + (FLOOR_SIZE.y / 2.0) + (PLAYER_SIZE / 2.0) + 0.01;
        let is_grounded = transform.translation.y <= ground_y_pos;

        if input & INPUT_LEFT != 0 {
            movement.x -= 1.0;
        }
        if input & INPUT_RIGHT != 0 {
            movement.x += 1.0;
        }
        
        velocity.x = movement.x * PLAYER_MOVE_FORCE;
        
        if is_grounded && input & INPUT_JUMP != 0 {
            velocity.y = PLAYER_JUMP_FORCE;
        }
    }
}
