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

/// A 4x4 matrix as nested arrays
type RawMatrix = [[f32; 4]; 4];

/// Mesh representation used by the rendering engine
pub struct Mesh {
    /// Triangle indices, counter-clockwise winding order is front-facing
    pub indices: Vec<u32>,
    pub vertices: Vec<Vertex>,
}

const MAX_HEADS: usize = 500;

/// Rendering engine state
pub struct Engine {
    // NOTE: We do not call destructors!
    map: GpuMesh,
    head: GpuMesh,

    head_inst_vbo: gl::NativeBuffer,
    head_count: usize,

    map_shader: gl::Program,
    head_shader: gl::Program,
}

struct GpuMesh {
    vao: gl::VertexArray,
    _vbo: gl::NativeBuffer,
    _ebo: gl::NativeBuffer,
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
            let map_shader = compile_glsl_program(
                &gl,
                &[
                    (gl::VERTEX_SHADER, include_str!("shaders/map.vert")),
                    (gl::FRAGMENT_SHADER, include_str!("shaders/unlit.frag")),
                ],
            )?;

            // Compile shaders
            let head_shader = compile_glsl_program(
                &gl,
                &[
                    (gl::VERTEX_SHADER, include_str!("shaders/head.vert")),
                    (gl::FRAGMENT_SHADER, include_str!("shaders/unlit.frag")),
                ],
            )?;

            // Upload head mesh
            let head = upload_mesh(gl, gl::STATIC_DRAW, head_mesh)?;

            // Upload map mesh
            let map = upload_mesh(gl, gl::DYNAMIC_DRAW, map_mesh)?;

            // Create head instance buffer
            gl.bind_vertex_array(Some(head.vao));
            let head_inst_vbo = gl.create_buffer()?;
            gl.bind_buffer(gl::ARRAY_BUFFER, Some(head_inst_vbo));
            gl.buffer_data_size(
                gl::ARRAY_BUFFER,
                (std::mem::size_of::<RawMatrix>() * MAX_HEADS) as i32,
                gl::DYNAMIC_DRAW,
            );
            gl.bind_buffer(gl::ARRAY_BUFFER, None);

            // Set up instance buffer
            gl.bind_buffer(gl::ARRAY_BUFFER, Some(head_inst_vbo));
            for i in 0..4 {
                let attrib_idx = 2 + i;
                gl.enable_vertex_attrib_array(attrib_idx);
                gl.vertex_attrib_pointer_f32(
                    attrib_idx,
                    4,
                    gl::FLOAT,
                    false,
                    std::mem::size_of::<RawMatrix>() as i32,
                    i as i32 * std::mem::size_of::<[f32; 4]>() as i32,
                );
                gl.vertex_attrib_divisor(attrib_idx, 1);
            }
            gl.bind_buffer(gl::ARRAY_BUFFER, None);
            gl.bind_vertex_array(None);

            Ok(Self {
                head_inst_vbo,
                head_count: 0,
                head,
                map,
                map_shader,
                head_shader,
            })
        }
    }

    /// Update head positions  
    pub fn update_heads(&mut self, gl: &gl::Context, heads: &[RawMatrix]) {
        assert!(heads.len() <= MAX_HEADS);
        unsafe {
            gl.bind_buffer(gl::ARRAY_BUFFER, Some(self.head_inst_vbo));
            gl.buffer_sub_data_u8_slice(gl::ARRAY_BUFFER, 0, bytemuck::cast_slice(heads));
            gl.bind_buffer(gl::ARRAY_BUFFER, None);
            self.head_count = heads.len();
        }
    }

    /// The given heads will be rendered using the provided projection matrix and view Head
    /// position
    pub fn frame(
        &mut self,
        gl: &gl::Context,
        proj: Matrix4<f32>,
        view: Matrix4<f32>,
        //view: Head,
    ) -> Result<(), String> {
        unsafe {
            // Clear depth and color buffers
            gl.clear_color(0.1, 0.2, 0.3, 1.0);
            gl.clear_depth_f32(1.);
            gl.clear(gl::COLOR_BUFFER_BIT | gl::STENCIL_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);

            let set_camera_uniforms = |shader| {
                // Set camera matrix
                gl.uniform_matrix_4_f32_slice(
                    gl.get_uniform_location(shader, "view").as_ref(),
                    false,
                    view.as_slice(),
                );

                gl.uniform_matrix_4_f32_slice(
                    gl.get_uniform_location(shader, "proj").as_ref(),
                    false,
                    proj.as_slice(),
                );
            };

            // Draw map
            gl.use_program(Some(self.map_shader));
            set_camera_uniforms(self.map_shader);

            gl.bind_vertex_array(Some(self.map.vao));
            gl.draw_elements(gl::TRIANGLES, self.map.index_count, gl::UNSIGNED_INT, 0);
            gl.bind_vertex_array(None);

            // Draw heads
            gl.use_program(Some(self.head_shader));
            set_camera_uniforms(self.head_shader);

            gl.bind_vertex_array(Some(self.head.vao));
            gl.draw_elements_instanced(
                gl::TRIANGLES,
                self.head.index_count,
                gl::UNSIGNED_INT,
                0,
                self.head_count as i32,
            );
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

/// Uploads a mesh; does not unbind vertex array
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
            _vbo: vbo,
            _ebo: ebo,
            index_count: mesh.indices.len() as i32,
        })
    }
}
