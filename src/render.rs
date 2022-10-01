use bytemuck::{Pod, Zeroable};
use cubehead::Head;
use glow::HasContext;
use nalgebra::{Matrix4, Point3, Vector3};

/// Vertex representation used by the rendering engine
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Vertex {
    pub pos: Point3<f32>,
    pub color: Vector3<f32>,
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

const MAX_HEADS: usize = 500;

/// Rendering engine state
pub struct Engine {
    map: GpuMesh,
    head: GpuMesh,

    //heads_vao: gl::NativeBuffer,
    //heads_vbo: gl::NativeBuffer,
    //heads_matrices: Vec<Matrix4<f32>>,
    shader: gl::Program,
}

struct GpuMesh {
    vao: gl::VertexArray,
    vbo: gl::NativeBuffer,
    ebo: gl::NativeBuffer,
    index_count: i32,
}

impl Engine {
    pub fn new(gl: &gl::Context, map_mesh: &Mesh, head_mesh: &Mesh) -> Result<Self, String> {
        unsafe {
            // Enable backface culling
            gl.enable(gl::CULL_FACE);

            // Enable depth buffering
            gl.enable(gl::DEPTH_TEST);
            gl.depth_func(gl::LESS);

            // Compile shaders
            let shader = compile_glsl_program(
                &gl,
                &[
                    (gl::VERTEX_SHADER, VERTEX_SHADER_SOURCE),
                    (gl::FRAGMENT_SHADER, FRAGMENT_SHADER_SOURCE),
                ],
            )?;

            let head = upload_mesh(gl, gl::STATIC_DRAW, head_mesh)?;
            let map = upload_mesh(gl, gl::STATIC_DRAW, map_mesh)?;

            Ok(Self { head, map, shader })
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
        unsafe {
            // Clear depth and color buffers
            gl.clear_color(0.1, 0.2, 0.3, 1.0);
            gl.clear_depth_f32(1.);
            gl.clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);

            // Use shader
            gl.use_program(Some(self.shader));

            // Set camera matrix
            gl.uniform_matrix_4_f32_slice(
                gl.get_uniform_location(self.shader, "view").as_ref(),
                false,
                view.as_slice(),
            );

            gl.uniform_matrix_4_f32_slice(
                gl.get_uniform_location(self.shader, "proj").as_ref(),
                false,
                proj.as_slice(),
            );

            // Draw map
            gl.bind_vertex_array(Some(self.map.vao));
            gl.draw_elements(gl::TRIANGLES, self.map.index_count, gl::UNSIGNED_INT, 0);
            gl.bind_vertex_array(None);

            // Draw head
            gl.bind_vertex_array(Some(self.head.vao));
            gl.draw_elements(gl::TRIANGLES, self.head.index_count, gl::UNSIGNED_INT, 0);
            gl.bind_vertex_array(None);

            Ok(())
        }
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

const VERTEX_SHADER_SOURCE: &str = r#"
    #version 450

    uniform mat4 view;
    uniform mat4 proj;

    in vec3 v_pos;
    in vec3 v_color;

    out vec4 f_color;

    void main() {
        gl_Position = proj * view * vec4(v_pos, 1.0);
        f_color = vec4(v_color, 1.);
    }
"#;

const FRAGMENT_SHADER_SOURCE: &str = r#"
    #version 450
    precision mediump float;

    in vec4 f_color;

    out vec4 out_color;

    void main() {
        out_color = f_color;
    }
"#;

/// Compiles (*_SHADER, <source>) into a shader program for OpenGL
fn compile_glsl_program(gl: &gl::Context, sources: &[(u32, &str)]) -> Result<gl::Program, String> {
    // Compile default shaders
    unsafe {
        let program = gl.create_program().expect("Cannot create program");

        let mut shaders = vec![];

        for (stage, shader_source) in sources {
            let shader = gl.create_shader(*stage).expect("Cannot create shader");

            gl.shader_source(shader, shader_source);

            gl.compile_shader(shader);

            if !gl.get_shader_compile_status(shader) {
                return Err(gl.get_shader_info_log(shader));
            }

            gl.attach_shader(program, shader);

            shaders.push(shader);
        }

        gl.link_program(program);

        if !gl.get_program_link_status(program) {
            return Err(gl.get_program_info_log(program));
        }

        for shader in shaders {
            gl.detach_shader(program, shader);
            gl.delete_shader(shader);
        }

        Ok(program)
    }
}

impl Vertex {
    pub fn new(pos: [f32; 3], color: [f32; 3]) -> Self {
        Self {
            pos: pos.into(),
            color: color.into(),
        }
    }
}

fn set_vertex_attrib(gl: &gl::Context) {
    unsafe {
        // Set vertex attributes
        gl.enable_vertex_attrib_array(0);
        gl.vertex_attrib_pointer_f32(
            0,
            3,
            gl::FLOAT,
            false,
            std::mem::size_of::<Vertex>() as i32,
            0,
        );

        gl.enable_vertex_attrib_array(1);
        gl.vertex_attrib_pointer_f32(
            1,
            3,
            gl::FLOAT,
            false,
            std::mem::size_of::<Vertex>() as i32,
            3 * std::mem::size_of::<f32>() as i32,
        );
    }
}

fn upload_mesh(gl: &gl::Context, usage: u32, mesh: &Mesh) -> Result<GpuMesh, String> {
    unsafe {
        // Map buffer
        let vao = gl.create_vertex_array()?;
        let vbo = gl.create_buffer()?;
        let ebo = gl.create_buffer()?;

        gl.bind_vertex_array(Some(vao));

        // Write vertices
        gl.bind_buffer(gl::ARRAY_BUFFER, Some(vbo));
        gl.buffer_data_u8_slice(
            gl::ARRAY_BUFFER,
            bytemuck::cast_slice(&mesh.vertices),
            usage,
        );

        // Write vertices
        gl.bind_buffer(gl::ELEMENT_ARRAY_BUFFER, Some(ebo));
        gl.buffer_data_u8_slice(
            gl::ELEMENT_ARRAY_BUFFER,
            bytemuck::cast_slice(&mesh.indices),
            usage,
        );

        // Set vertex attributes
        set_vertex_attrib(gl);

        // Unbind vertex array
        gl.bind_vertex_array(None);

        Ok(GpuMesh {
            vao,
            vbo,
            ebo,
            index_count: mesh.indices.len() as i32,
        })
    }
}
