use cubehead::Head;
use glutin::{
    dpi::PhysicalPosition,
    event::{ElementState, MouseButton, MouseScrollDelta, VirtualKeyCode, WindowEvent},
};
use nalgebra::{Matrix4, Point3, UnitQuaternion, Vector3, Vector4};
use std::f32::consts::PI;
use winit_input_helper::WinitInputHelper;

pub struct FlyCam {
    yaw: f32,
    pitch: f32,
    pos: Point3<f32>,
}

impl FlyCam {
    pub fn new(pos: Point3<f32>) -> Self {
        Self {
            yaw: 0.,
            pitch: 0.,
            pos,
        }
    }

    pub fn update(&mut self, wih: &WinitInputHelper, speed: f32, sensitivity: f32) {
        if wih.mouse_held(0) {
            let (x_delta, y_delta) = wih.mouse_diff();
            self.yaw += x_delta * sensitivity;
            self.pitch = (self.pitch + y_delta * sensitivity).clamp(-PI, PI);
        }

        if wih.key_held(VirtualKeyCode::W) {
            self.pos += self.head().orient.transform_vector(&-Vector3::z()) * speed;
        }
    }

    pub fn head(&self) -> Head {
        Head {
            pos: self.pos,
            orient: UnitQuaternion::from_euler_angles(self.pitch, self.yaw, 0.),
        }
    }
}
