use rg3d::{
    core::{algebra::Vector3, math::Vector3Ext, pool::Handle},
    engine::resource_manager::ResourceManager,
    scene::{graph::Graph, node::Node, Scene},
};

pub struct Weapon {
    model: Handle<Node>,
    shot_point: Handle<Node>,
    shot_timer: f32,
    recoil_offset: Vector3<f32>,
    recoil_target_offset: Vector3<f32>,
}

impl Weapon {
    pub async fn new(scene: &mut Scene, resource_manager: ResourceManager) -> Self {
        let model = resource_manager
            .request_model("assets/models/m4/m4.FBX")
            .await
            .unwrap()
            .instantiate_geometry(scene);
        let shot_point = scene.graph.find_by_name(model, "Weapon:ShotPoint");
        Self {
            model,
            shot_point,
            shot_timer: 0.0,
            recoil_offset: Default::default(),
            recoil_target_offset: Default::default(),
        }
    }

    pub fn model(&self) -> Handle<Node> {
        self.model
    }

    pub fn shot_point(&self) -> Handle<Node> {
        self.shot_point
    }

    pub fn update(&mut self, dt: f32, graph: &mut Graph) {
        self.shot_timer = (self.shot_timer - dt).min(0.0);
        self.recoil_offset.follow(&self.recoil_target_offset, 0.5);
        graph[self.model]
            .local_transform_mut()
            .set_position(self.recoil_offset);
        if self
            .recoil_offset
            .metric_distance(&self.recoil_target_offset)
            < 0.001
        {
            self.recoil_target_offset = Default::default();
        }
    }

    pub fn can_shoot(&self) -> bool {
        self.shot_timer <= 0.0
    }

    pub fn shoot(&mut self) {
        self.shot_timer = 0.1;
        self.recoil_target_offset = Vector3::new(0.0, 0.00625, -0.025);
    }
}
