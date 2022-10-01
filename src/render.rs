use bytemuck::{Pod, Zeroable};
use cubehead::Head;
use glow::HasContext;
use nalgebra::Matrix4;

/// Vertex representation used by the rendering engine
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub color: [f32; 3],
}

// Allow Vertex to be cast to bytes using bytemuck
unsafe impl Zeroable for Vertex {}
unsafe impl Pod for Vertex {}

/// Mesh representation used by the rendering engine
pub struct Mesh {
    /// Triangle indices, counter-clockwise winding order is front-facing
    pub indices: Vec<u32>,
    pub vertices: Vec<Vertex>,
}

/// Rendering engine state
pub struct Engine {
    map_vao: glow::VertexArray,
    map_vbo: glow::NativeBuffer,
    map_ebo: glow::NativeBuffer,
}

impl Engine {
    pub fn new(gl: &gl::Context, map_mesh: &Mesh, head_mesh: &Mesh) -> Result<Self, String> {
        unsafe {
            let map_vao = gl.create_vertex_array()?;
            let map_vbo = gl.create_buffer()?;
            let map_ebo = gl.create_buffer()?;

            gl.bind_vertex_array(Some(map_vao));

            // Write vertices
            gl.bind_buffer(gl::ARRAY_BUFFER, Some(map_vbo));
            gl.buffer_data_u8_slice(
                gl::ARRAY_BUFFER,
                bytemuck::cast_slice(&map_mesh.vertices),
                gl::STATIC_DRAW,
            );

            // Write vertices
            gl.bind_buffer(gl::ELEMENT_ARRAY_BUFFER, Some(map_ebo));
            gl.buffer_data_u8_slice(
                gl::ELEMENT_ARRAY_BUFFER,
                bytemuck::cast_slice(&map_mesh.indices),
                gl::STATIC_DRAW,
            );

            // Set vertex attributes
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(
                0,
                3,
                glow::FLOAT,
                false,
                std::mem::size_of::<Vertex>() as i32,
                0,
            );

            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(
                1,
                3,
                glow::FLOAT,
                false,
                std::mem::size_of::<Vertex>() as i32,
                3 * std::mem::size_of::<f32>() as i32,
            );

            gl.bind_vertex_array(None);

            Ok(Self {
                map_vao,
                map_vbo,
                map_ebo,
            })
        }
    }

    /// The given heads will be rendered using the provided projection matrix and view Head
    /// position
    pub fn frame(
        &mut self,
        gl: &gl::Context,
        heads: &[Head],
        proj: Matrix4<f32>,
        view: Matrix4<f32>,
        //view: Head,
    ) -> Result<(), String> {

    }
}

impl Vertex {
    pub fn new(pos: [f32; 3], color: [f32; 3]) -> Self {
        Self { pos, color }
    }
}


/// Creates a view matrix for the given head position
pub fn view_from_head(head: &Head) -> Matrix4<f32> {
    // Invert this quaternion, orienting the world into NDC space
    // Represent the rotation in homogeneous coordinates
    let rotation = head.orient.inverse().to_homogeneous();

    // Invert this translation, translating the world into NDC space
    let translation = Matrix4::new_translation(&-head.pos.coords);

    // Compose the view
    rotation * translation
}


