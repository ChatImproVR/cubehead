use nalgebra::{Matrix4, Point3, UnitQuaternion};
use serde::{Deserialize, Serialize};

/// The position and orientation of a user's head
/// User's head points in the negative Z direction (following OpenGL NDC)
#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct Head {
    /// Position
    pub pos: Point3<f32>,
    /// Orientation
    pub orient: UnitQuaternion<f32>,
}

impl Head {
    pub fn matrix(&self) -> Matrix4<f32> {
        // TODO: Make this cheaper?
        Matrix4::new_translation(&self.pos.coords) * self.orient.to_homogeneous()
    }
}
