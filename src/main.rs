mod message;
mod player;
mod weapon;

use rg3d::{
    core::{
        algebra::{UnitQuaternion, Vector3},
        color::Color,
        color_gradient::{ColorGradient, GradientPoint},
        math::ray::Ray,
        numeric_range::NumericRange,
        pool::{Handle, Pool},
    },
    engine::{resource_manager::ResourceManager, Engine},
    event::{Event, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    gui::node::StubNode,
    renderer::surface::{SurfaceBuilder, SurfaceSharedData},
    scene::{
        base::BaseBuilder,
        graph::Graph,
        mesh::{MeshBuilder, RenderPath},
        node::Node,
        particle_system::{BaseEmitterBuilder, ParticleSystemBuilder, SphereEmitterBuilder},
        physics::RayCastOptions,
        transform::TransformBuilder,
        Scene,
    },
    window::WindowBuilder,
};
use std::{
    path::Path,
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc, RwLock,
    },
    time,
};

use message::Message;
use player::Player;
use weapon::Weapon;

// Create our own engine type aliases. These specializations are needed, because the engine
// provides a way to extend UI with custom nodes and messages.
type GameEngine = Engine<(), StubNode>;

// Our game logic will be updated at 60 Hz rate.
const TIMESTEP: f32 = 1.0 / 60.0;

struct Game {
    scene: Handle<Scene>,
    player: Player,
    weapons: Pool<Weapon>,
    receiver: Receiver<Message>,
    sender: Sender<Message>,
}

impl Game {
    pub async fn new(engine: &mut GameEngine) -> Self {
        let mut scene = Scene::new();
        engine
            .resource_manager
            .state()
            .set_textures_path("assets/textures");
        engine
            .resource_manager
            .request_model("assets/models/scene.rgs")
            .await
            .unwrap()
            .instantiate_geometry(&mut scene);
        let (sender, receiver) = mpsc::channel();
        let mut player =
            Player::new(&mut scene, engine.resource_manager.clone(), sender.clone()).await;
        let weapon = Weapon::new(&mut scene, engine.resource_manager.clone()).await;
        scene.graph.link_nodes(weapon.model(), player.weapon_pivot);
        let mut weapons = Pool::new();
        player.weapon = weapons.spawn(weapon);
        Self {
            player,
            scene: engine.scenes.add(scene),
            weapons,
            receiver,
            sender,
        }
    }

    pub fn update(&mut self, engine: &mut GameEngine, dt: f32) {
        self.player.update(&mut engine.scenes[self.scene]);
        for weapon in self.weapons.iter_mut() {
            weapon.update(dt, &mut engine.scenes[self.scene].graph)
        }
        while let Ok(message) = self.receiver.try_recv() {
            match message {
                Message::ShootWeapon { weapon } => {
                    self.shoot_weapon(weapon, engine);
                }
            }
        }
    }

    fn shoot_weapon(&mut self, weapon: Handle<Weapon>, engine: &mut GameEngine) {
        let weapon = &mut self.weapons[weapon];

        if weapon.can_shoot() {
            weapon.shoot();

            let scene = &mut engine.scenes[self.scene];

            let weapon_model = &scene.graph[weapon.model()];

            // Make a ray that starts at the weapon's position in the world and look toward
            // "look" vector of the weapon.
            let ray = Ray::new(
                scene.graph[weapon.shot_point()].global_position(),
                weapon_model.look_vector().scale(1000.0),
            );

            let mut intersections = Vec::new();

            scene.physics.cast_ray(
                RayCastOptions {
                    ray,
                    max_len: ray.dir.norm(),
                    groups: Default::default(),
                    sort_results: true, // We need intersections to be sorted from closest to furthest.
                },
                &mut intersections,
            );

            // Ignore intersections with player's capsule.
            let trail_length = if let Some(intersection) = intersections
                .iter()
                .find(|i| i.collider != self.player.collider)
            {
                //
                // TODO: Add code to handle intersections with bots.
                //

                // For now just apply some force at the point of impact.
                let collider = scene
                    .physics
                    .colliders
                    .get(intersection.collider.into())
                    .unwrap();
                scene
                    .physics
                    .bodies
                    .get_mut(collider.parent())
                    .unwrap()
                    .apply_force_at_point(
                        ray.dir.normalize().scale(10.0),
                        intersection.position,
                        true,
                    );

                let effect_orientation = if intersection.normal.normalize() == Vector3::y() {
                    UnitQuaternion::from_axis_angle(&Vector3::y_axis(), 0.0)
                } else {
                    UnitQuaternion::face_towards(&intersection.normal, &Vector3::y())
                };
                Self::create_bullet_impact(
                    &mut scene.graph,
                    engine.resource_manager.clone(),
                    intersection.position.coords,
                    effect_orientation,
                );

                // Trail length will be the length of line between intersection point and ray origin.
                (intersection.position.coords - ray.origin).norm()
            } else {
                // Otherwise trail length will be just the ray length.
                ray.dir.norm()
            };

            Self::create_shot_trail(&mut scene.graph, ray.origin, ray.dir, trail_length);
        }
    }

    fn create_shot_trail(
        graph: &mut Graph,
        origin: Vector3<f32>,
        direction: Vector3<f32>,
        trail_length: f32,
    ) {
        let transform = TransformBuilder::new()
            .with_local_position(origin)
            .with_local_scale(Vector3::new(0.0025, 0.0025, trail_length))
            .with_local_rotation(UnitQuaternion::face_towards(&direction, &Vector3::y()))
            .build();

        // Create unit cylinder with caps that faces toward Z axis.
        let shape = Arc::new(RwLock::new(SurfaceSharedData::make_cylinder(
            6,     // Count of sides
            1.0,   // Radius
            1.0,   // Height
            false, // No caps are needed.
            // Rotate vertical cylinder around X axis to make it face towards Z axis
            UnitQuaternion::from_axis_angle(&Vector3::x_axis(), 90.0f32.to_radians())
                .to_homogeneous(),
        )));

        MeshBuilder::new(
            BaseBuilder::new()
                .with_local_transform(transform)
                // Shot trail should live ~0.25 seconds, after that it will be automatically
                // destroyed.
                .with_lifetime(0.25),
        )
        .with_surfaces(vec![SurfaceBuilder::new(shape)
            // Set yellow-ish color.
            .with_color(Color::from_rgba(255, 255, 0, 120))
            .build()])
        // Do not cast shadows.
        .with_cast_shadows(false)
        // Make sure to set Forward render path, otherwise the object won't be
        // transparent.
        .with_render_path(RenderPath::Forward)
        .build(graph);
    }

    fn create_bullet_impact(
        graph: &mut Graph,
        resource_manager: ResourceManager,
        pos: Vector3<f32>,
        orientation: UnitQuaternion<f32>,
    ) -> Handle<Node> {
        // Create sphere emitter first.
        let emitter = SphereEmitterBuilder::new(
            BaseEmitterBuilder::new()
                .with_max_particles(200)
                .with_spawn_rate(1000)
                .with_size_modifier_range(NumericRange::new(-0.01, -0.0125))
                .with_size_range(NumericRange::new(0.0010, 0.025))
                .with_x_velocity_range(NumericRange::new(-0.01, 0.01))
                .with_y_velocity_range(NumericRange::new(0.030, 0.10))
                .with_z_velocity_range(NumericRange::new(-0.01, 0.01))
                .resurrect_particles(false),
        )
        .with_radius(0.01)
        .build();

        // Color gradient will be used to modify color of each particle over its lifetime.
        let color_gradient = {
            let mut gradient = ColorGradient::new();
            gradient.add_point(GradientPoint::new(0.00, Color::from_rgba(255, 255, 0, 0)));
            gradient.add_point(GradientPoint::new(0.05, Color::from_rgba(255, 160, 0, 255)));
            gradient.add_point(GradientPoint::new(0.95, Color::from_rgba(255, 120, 0, 255)));
            gradient.add_point(GradientPoint::new(1.00, Color::from_rgba(255, 60, 0, 0)));
            gradient
        };

        // Create new transform to orient and position particle system.
        let transform = TransformBuilder::new()
            .with_local_position(pos)
            .with_local_rotation(orientation)
            .build();

        // Finally create particle system with limited lifetime.
        ParticleSystemBuilder::new(
            BaseBuilder::new()
                .with_lifetime(1.0)
                .with_local_transform(transform),
        )
        .with_acceleration(Vector3::new(0.0, -10.0, 0.0))
        .with_color_over_lifetime_gradient(color_gradient)
        .with_emitters(vec![emitter])
        // We'll use simple spark texture for each particle.
        .with_texture(resource_manager.request_texture(Path::new("assets/textures/spark.png")))
        .build(graph)
    }
}

fn main() {
    // Configure main window first.
    let window_builder = WindowBuilder::new()
        .with_maximized(true)
        .with_title("3D Shooter Tutorial");
    // Create event loop that will be used to "listen" events from the OS.
    let event_loop = EventLoop::new();

    // Finally create an instance of the engine.
    let mut engine = GameEngine::new(window_builder, &event_loop, true).unwrap();

    // Initialize game instance. It is empty for now.
    let mut game = rg3d::futures::executor::block_on(Game::new(&mut engine));

    // Run the event loop of the main window. which will respond to OS and window events and update
    // engine's state accordingly. Engine lets you to decide which event should be handled,
    // this is minimal working example if how it should be.
    let clock = time::Instant::now();

    let mut elapsed_time = 0.0;
    event_loop.run(move |event, _, control_flow| {
        game.player.process_input_event(&event);
        match event {
            Event::MainEventsCleared => {
                // This main game loop - it has fixed time step which means that game
                // code will run at fixed speed even if renderer can't give you desired
                // 60 fps.
                let mut dt = clock.elapsed().as_secs_f32() - elapsed_time;
                while dt >= TIMESTEP {
                    dt -= TIMESTEP;
                    elapsed_time += TIMESTEP;

                    // Run our game's logic.
                    game.update(&mut engine, dt);

                    // Update engine each frame.
                    engine.update(TIMESTEP);
                }

                // Rendering must be explicitly requested and handled after RedrawRequested event is received.
                engine.get_window().request_redraw();
            }
            Event::RedrawRequested(_) => {
                // Render at max speed - it is not tied to the game code.
                engine.render(TIMESTEP).unwrap();
            }
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                WindowEvent::KeyboardInput { input, .. } => {
                    // Exit game by hitting Escape.
                    if let Some(VirtualKeyCode::Escape) = input.virtual_keycode {
                        *control_flow = ControlFlow::Exit
                    }
                }
                WindowEvent::Resized(size) => {
                    // It is very important to handle Resized event from window, because
                    // renderer knows nothing about window size - it must be notified
                    // directly when window size has changed.
                    engine.renderer.set_frame_size(size.into());
                }
                _ => (),
            },
            _ => *control_flow = ControlFlow::Poll,
        }
    });
}
