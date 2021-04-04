use std::sync::mpsc::Sender;

use rg3d::{
    core::{
        algebra::{UnitQuaternion, Vector3},
        pool::Handle,
    },
    engine::resource_manager::ResourceManager,
    event::{DeviceEvent, ElementState, Event, MouseButton, VirtualKeyCode, WindowEvent},
    physics::{dynamics::RigidBodyBuilder, geometry::ColliderBuilder},
    resource::texture::TextureWrapMode,
    scene::{
        base::BaseBuilder,
        camera::{CameraBuilder, SkyBox},
        node::Node,
        transform::TransformBuilder,
        ColliderHandle, RigidBodyHandle, Scene,
    },
};

use crate::{message::Message, weapon::Weapon};

#[derive(Default)]
pub struct InputController {
    move_forward: bool,
    move_backward: bool,
    move_left: bool,
    move_right: bool,
    pitch: f32,
    yaw: f32,
    shoot: bool,
}

pub struct Player {
    pub pivot: Handle<Node>,
    pub camera: Handle<Node>,
    pub weapon_pivot: Handle<Node>,
    pub weapon: Handle<Weapon>,
    pub rigid_body: RigidBodyHandle,
    pub collider: ColliderHandle,
    pub controller: InputController,
    pub sender: Sender<Message>,
}

async fn create_skybox(resource_manager: ResourceManager) -> SkyBox {
    let (front, back, left, right, top, bottom) = rg3d::futures::join!(
        resource_manager.request_texture("assets/textures/skybox/front.jpg"),
        resource_manager.request_texture("assets/textures/skybox/back.jpg"),
        resource_manager.request_texture("assets/textures/skybox/left.jpg"),
        resource_manager.request_texture("assets/textures/skybox/right.jpg"),
        resource_manager.request_texture("assets/textures/skybox/up.jpg"),
        resource_manager.request_texture("assets/textures/skybox/down.jpg")
    );
    let skybox = SkyBox {
        front: Some(front.unwrap()),
        back: Some(back.unwrap()),
        left: Some(left.unwrap()),
        right: Some(right.unwrap()),
        top: Some(top.unwrap()),
        bottom: Some(bottom.unwrap()),
    };
    for skybox_texture in skybox.textures().iter().filter_map(|t| t.clone()) {
        let mut data = skybox_texture.data_ref();
        data.set_s_wrap_mode(TextureWrapMode::ClampToEdge);
        data.set_t_wrap_mode(TextureWrapMode::ClampToEdge);
    }
    skybox
}

impl Player {
    pub async fn new(
        scene: &mut Scene,
        resource_manager: ResourceManager,
        sender: Sender<Message>,
    ) -> Self {
        let weapon_pivot = BaseBuilder::new()
            .with_local_transform(
                TransformBuilder::new()
                    .with_local_position(Vector3::new(-0.1, -0.05, 0.015))
                    .build(),
            )
            .build(&mut scene.graph);
        let camera = CameraBuilder::new(
            BaseBuilder::new()
                .with_local_transform(
                    TransformBuilder::new()
                        .with_local_position(Vector3::new(0.0, 0.25, 0.0))
                        .build(),
                )
                .with_children(&[weapon_pivot]),
        )
        .with_skybox(create_skybox(resource_manager).await)
        .build(&mut scene.graph);
        let pivot = BaseBuilder::new()
            .with_children(&[camera])
            .build(&mut scene.graph);
        let rigid_body_handle = scene.physics.add_body(
            RigidBodyBuilder::new_dynamic()
                .lock_rotations()
                .translation(0.0, 1.0, -1.0)
                .build(),
        );
        let collider = scene.physics.add_collider(
            ColliderBuilder::capsule_y(0.25, 0.2).build(),
            rigid_body_handle,
        );
        scene.physics_binder.bind(pivot, rigid_body_handle);
        Self {
            pivot,
            camera,
            weapon_pivot,
            weapon: Default::default(),
            rigid_body: rigid_body_handle,
            collider,
            controller: Default::default(),
            sender,
        }
    }
    pub fn update(&mut self, scene: &mut Scene) {
        scene.graph[self.camera].local_transform_mut().set_rotation(
            UnitQuaternion::from_axis_angle(&Vector3::x_axis(), self.controller.pitch.to_radians()),
        );
        let pivot = &mut scene.graph[self.pivot];
        let body = scene
            .physics
            .bodies
            .get_mut(self.rigid_body.into())
            .unwrap();
        let mut velocity = Vector3::new(0.0, body.linvel().y, 0.0);
        if self.controller.move_forward {
            velocity += pivot.look_vector();
        }
        if self.controller.move_backward {
            velocity -= pivot.look_vector();
        }
        if self.controller.move_left {
            velocity += pivot.side_vector();
        }
        if self.controller.move_right {
            velocity -= pivot.side_vector();
        }
        body.set_linvel(velocity, true);
        let mut position = *body.position();
        position.rotation =
            UnitQuaternion::from_axis_angle(&Vector3::y_axis(), self.controller.yaw.to_radians());
        body.set_position(position, true);
        if self.controller.shoot {
            self.sender
                .send(Message::ShootWeapon {
                    weapon: self.weapon,
                })
                .unwrap();
        }
    }
    pub fn process_input_event(&mut self, event: &Event<()>) {
        match event {
            Event::WindowEvent {
                event: WindowEvent::KeyboardInput { input, .. },
                ..
            } => {
                if let Some(key_code) = input.virtual_keycode {
                    match key_code {
                        VirtualKeyCode::W => {
                            self.controller.move_forward = input.state == ElementState::Pressed;
                        }
                        VirtualKeyCode::S => {
                            self.controller.move_backward = input.state == ElementState::Pressed;
                        }
                        VirtualKeyCode::A => {
                            self.controller.move_left = input.state == ElementState::Pressed;
                        }
                        VirtualKeyCode::D => {
                            self.controller.move_right = input.state == ElementState::Pressed;
                        }
                        _ => (),
                    }
                }
            }
            Event::WindowEvent {
                event:
                    WindowEvent::MouseInput {
                        button: MouseButton::Left,
                        state,
                        ..
                    },
                ..
            } => self.controller.shoot = *state == ElementState::Pressed,
            Event::DeviceEvent {
                event: DeviceEvent::MouseMotion { delta },
                ..
            } => {
                self.controller.yaw -= delta.0 as f32;
                self.controller.pitch = (self.controller.pitch + delta.1 as f32).clamp(-90.0, 90.0);
            }
            _ => (),
        }
    }
}
