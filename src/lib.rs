use nalgebra::{Point3, UnitQuaternion};
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
