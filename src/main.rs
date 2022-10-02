extern crate glow as gl;
extern crate openxr as xr;

use std::net::{SocketAddr, TcpStream};

use cubehead::{AsyncBufferedReceiver, Head, ReadState};
use glutin::{window::Window, ContextWrapper, PossiblyCurrent};
use render::Mesh;
use winit_input_helper::WinitInputHelper;
use xr::opengl::SessionCreateInfo;

use anyhow::{bail, format_err, Result};
use gl::HasContext;
use glutin::dpi::PhysicalSize;
use nalgebra::{Matrix4, Point3, Quaternion, Unit, UnitQuaternion, Vector3};

mod camera;
mod render;
mod shapes;

use camera::{FlyCam, Perspective};
use shapes::{big_quad_map, rgb_cube};

use clap::Parser;

/// Simple program to greet a person
#[derive(Parser, Debug)]
struct Args {
    /// Use OpenXR instead of windowed mode
    #[arg(long)]
    vr: bool,

    /// Spawn this many desktop clients
    #[arg(short, long)]
    clients: Option<usize>,

    /// Connection address
    #[arg()]
    addr: SocketAddr,
}

fn main() -> Result<()> {
    let args = Args::parse();

    if let Some(count) = args.clients {
        // Launch many desktop clients for testing
        let program_name = std::env::args().next().unwrap();
        for _ in 0..count {
            std::process::Command::new(&program_name)
                .arg(args.addr.to_string())
                .spawn()?;
        }
    } else {
        // Launch a single client
        unsafe {
            if args.vr {
                vr_main(args.addr)?;
            } else {
                desktop_main(args.addr)?;
            }
        }
    }

    Ok(())
}

unsafe fn desktop_main(addr: SocketAddr) -> Result<()> {
    let event_loop = glutin::event_loop::EventLoop::new();
    let window_builder = glutin::window::WindowBuilder::new()
        .with_title("Hello triangle!")
        .with_inner_size(glutin::dpi::LogicalSize::new(1024.0, 768.0));

    let glutin_ctx = glutin::ContextBuilder::new()
        .with_vsync(true)
        .build_windowed(window_builder, &event_loop)?
        .make_current()
        .unwrap();

    let gl = gl::Context::from_loader_function(|s| glutin_ctx.get_proc_address(s) as *const _);

    // We handle events differently between targets
    use glutin::event::{Event, WindowEvent};
    use glutin::event_loop::ControlFlow;

    let mut wih = WinitInputHelper::new();
    let mut camera = FlyCam::new(Point3::new(0., 4., 0.));
    let perspective_cfg = Perspective::default();

    let (map_mesh, head_mesh) = models();
    let mut engine = render::Engine::new(&gl, &map_mesh, &head_mesh)
        .map_err(|e| format_err!("Render engine failed to start; {}", e))?;

    let mut client = Client::new(addr)?;

    let mut proj = perspective_cfg.matrix(0., 0.);

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        if wih.update(&event) {
            camera.update(&wih, 0.05, 2e-3);
            // Send head position to server
            client.set_head_pos(&camera.head()).unwrap();
        }

        if let Some(ph) = wih.window_resized() {
            glutin_ctx.resize(ph);
            gl.scissor(0, 0, ph.width as i32, ph.height as i32);
            gl.viewport(0, 0, ph.width as i32, ph.height as i32);
            proj = perspective_cfg.matrix(ph.width as f32, ph.height as f32);
        }

        let heads = client.update_heads().unwrap();
        let head_mats = head_matrices(&heads);
        engine.update_heads(&gl, &head_mats);

        match event {
            Event::LoopDestroyed => {
                return;
            }
            Event::MainEventsCleared => {
                glutin_ctx.window().request_redraw();
            }
            Event::RedrawRequested(_) => {
                engine
                    .frame(&gl, proj, view_from_head(&camera.head()))
                    .expect("Engine error");

                glutin_ctx.swap_buffers().unwrap();
            }
            Event::WindowEvent { ref event, .. } => match event {
                WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                _ => (),
            },
            _ => (),
        }
    });
}

unsafe fn vr_main(addr: SocketAddr) -> Result<()> {
    // Load OpenXR from platform-specific location
    #[cfg(target_os = "linux")]
    let entry = xr::Entry::load()?;

    #[cfg(target_os = "windows")]
    let entry = xr::Entry::linked();

    // Application info
    let app_info = xr::ApplicationInfo {
        application_name: "Ugly OpenGL",
        application_version: 0,
        engine_name: "Ugly Engine",
        engine_version: 0,
    };

    // Ensure we have the OpenGL extension
    let available_extensions = entry.enumerate_extensions()?;
    assert!(available_extensions.khr_opengl_enable);

    // Enable the OpenGL extension
    let mut extensions = xr::ExtensionSet::default();
    extensions.khr_opengl_enable = true;

    // Create instance
    let xr_instance = entry.create_instance(&app_info, &extensions, &[])?;
    let instance_props = xr_instance.properties().unwrap();
    println!(
        "loaded OpenXR runtime: {} {}",
        instance_props.runtime_name, instance_props.runtime_version
    );

    // Get headset system
    let xr_system = xr_instance.system(xr::FormFactor::HEAD_MOUNTED_DISPLAY)?;

    let xr_view_configs = xr_instance.enumerate_view_configurations(xr_system)?;
    assert_eq!(xr_view_configs.len(), 1);
    let xr_view_type = xr_view_configs[0];

    let xr_views = xr_instance.enumerate_view_configuration_views(xr_system, xr_view_type)?;

    // Check what blend mode is valid for this device (opaque vs transparent displays). We'll just
    // take the first one available!
    let xr_environment_blend_mode =
        xr_instance.enumerate_environment_blend_modes(xr_system, xr_view_type)?[0];

    // TODO: Check this???
    let _xr_opengl_requirements = xr_instance.graphics_requirements::<xr::OpenGL>(xr_system)?;

    // Create window
    let event_loop = glutin::event_loop::EventLoop::new();
    let window_builder = glutin::window::WindowBuilder::new()
        .with_title("Hello world!")
        .with_inner_size(glutin::dpi::LogicalSize::new(1024.0f32, 768.0));

    let windowed_context = glutin::ContextBuilder::new()
        .build_windowed(window_builder, &event_loop)
        .unwrap();

    let (ctx, window) = windowed_context.split();
    let ctx = ctx.make_current().unwrap();

    // Load OpenGL
    let gl = gl::Context::from_loader_function(|s| ctx.get_proc_address(s) as *const _);

    let session_create_info = glutin_openxr_opengl_helper::session_create_info(&ctx, &window)?;

    // Create vertex array
    let vertex_array = gl
        .create_vertex_array()
        .expect("Cannot create vertex array");
    gl.bind_vertex_array(Some(vertex_array));

    // Create session
    let (xr_session, mut xr_frame_waiter, mut xr_frame_stream) =
        xr_instance.create_session::<xr::OpenGL>(xr_system, &session_create_info)?;

    // Determine swapchain formats
    let xr_swapchain_formats = xr_session.enumerate_swapchain_formats()?;

    let color_swapchain_format = xr_swapchain_formats
        .iter()
        .copied()
        .find(|&f| f == gl::SRGB8_ALPHA8)
        .unwrap_or(xr_swapchain_formats[0]);

    /*
    let depth_swapchain_format = xr_swapchain_formats
    .iter()
    .copied()
    .find(|&f| f == glow::DEPTH_COMPONENT16)
    .expect("No suitable depth format found");
    */

    // Create color swapchain
    let mut swapchain_images = vec![];
    let mut xr_swapchains = vec![];

    // Set up swapchains and get images
    for &xr_view in &xr_views {
        let xr_swapchain_create_info = xr::SwapchainCreateInfo::<xr::OpenGL> {
            create_flags: xr::SwapchainCreateFlags::EMPTY,
            usage_flags: xr::SwapchainUsageFlags::SAMPLED
                | xr::SwapchainUsageFlags::COLOR_ATTACHMENT,
            format: color_swapchain_format,
            sample_count: xr_view.recommended_swapchain_sample_count,
            width: xr_view.recommended_image_rect_width,
            height: xr_view.recommended_image_rect_height,
            face_count: 1,
            array_size: 1,
            mip_count: 1,
        };

        let xr_swapchain = xr_session.create_swapchain(&xr_swapchain_create_info)?;

        let images = xr_swapchain.enumerate_images()?;

        swapchain_images.push(images);
        xr_swapchains.push(xr_swapchain);
    }

    // Create OpenGL framebuffers
    let mut gl_framebuffers = vec![];
    for _ in &xr_views {
        gl_framebuffers.push(
            gl.create_framebuffer()
                .map_err(|s| format_err!("Failed to create framebuffer; {}", s))?,
        );
    }

    // Compile shaders
    let xr_play_space =
        xr_session.create_reference_space(xr::ReferenceSpaceType::LOCAL, xr::Posef::IDENTITY)?;

    let mut xr_event_buf = xr::EventDataBuffer::default();

    let (map_mesh, head_mesh) = models();
    let mut engine = render::Engine::new(&gl, &map_mesh, &head_mesh)
        .map_err(|e| format_err!("Render engine failed to start; {}", e))?;

    let mut client = Client::new(addr)?;

    'main: loop {
        // Handle OpenXR Events
        while let Some(event) = xr_instance.poll_event(&mut xr_event_buf)? {
            match event {
                xr::Event::InstanceLossPending(_) => break 'main,
                xr::Event::SessionStateChanged(delta) => {
                    match delta.state() {
                        xr::SessionState::IDLE | xr::SessionState::UNKNOWN => {
                            continue 'main;
                        }
                        //xr::SessionState::FOCUSED | xr::SessionState::SYNCHRONIZED | xr::SessionState::VISIBLE => (),
                        xr::SessionState::STOPPING => {
                            xr_session.end()?;
                            break 'main;
                        }
                        xr::SessionState::LOSS_PENDING | xr::SessionState::EXITING => {
                            // ???
                        }
                        xr::SessionState::READY => {
                            dbg!(delta.state());
                            xr_session.begin(xr_view_type)?;
                        }
                        _ => continue 'main,
                    }
                }
                _ => (),
            }
        }

        // --- Wait for our turn to do head-pose dependent computation and render a frame
        let xr_frame_state = xr_frame_waiter.wait()?;

        // Signal to OpenXR that we are beginning graphics work
        xr_frame_stream.begin()?;

        // Early exit
        if !xr_frame_state.should_render {
            xr_frame_stream.end(
                xr_frame_state.predicted_display_time,
                xr_environment_blend_mode,
                &[],
            )?;
            continue;
        }

        // Get head positions from server
        let heads = client.update_heads()?;
        let head_mats = head_matrices(&heads);
        engine.update_heads(&gl, &head_mats);

        // Get OpenXR Views
        // TODO: Do this as close to render-time as possible!!
        let (_xr_view_state_flags, xr_view_poses) = xr_session.locate_views(
            xr_view_type,
            xr_frame_state.predicted_display_time,
            &xr_play_space,
        )?;

        for view_idx in 0..xr_views.len() {
            // Acquire image
            let xr_swapchain_img_idx = xr_swapchains[view_idx].acquire_image()?;
            xr_swapchains[view_idx].wait_image(xr::Duration::from_nanos(1_000_000_000_000))?;

            // Bind framebuffer
            gl.bind_framebuffer(gl::FRAMEBUFFER, Some(gl_framebuffers[view_idx]));

            // Set scissor and viewport
            let view = xr_views[view_idx];
            let w = view.recommended_image_rect_width as i32;
            let h = view.recommended_image_rect_height as i32;
            gl.viewport(0, 0, w, h);
            gl.scissor(0, 0, w, h);

            // Set the texture as the render target
            let texture = swapchain_images[view_idx][xr_swapchain_img_idx as usize];
            let texture = std::num::NonZeroU32::new(texture).unwrap();

            /// Workaround for glow having not released https://github.com/grovesNL/glow/issues/210
            pub struct NativeTextureFuckery(pub std::num::NonZeroU32);

            let texture: glow::NativeTexture = std::mem::transmute(NativeTextureFuckery(texture));

            gl.framebuffer_texture_2d(
                gl::FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D,
                Some(texture),
                0,
            );

            // Set view and projection matrices
            let headset_view = xr_view_poses[view_idx];

            let view = view_from_pose(&headset_view.pose);
            let proj = projection_from_fov(&headset_view.fov, 0., 1000.);

            engine.frame(&gl, proj, view).expect("Engine error");

            // Unbind framebuffer
            gl.bind_framebuffer(gl::FRAMEBUFFER, None);

            // Release image
            xr_swapchains[view_idx].release_image()?;
        }

        // Set up projection views
        let mut xr_projection_views = vec![];
        for view_idx in 0..xr_views.len() {
            // Set up projection view
            let xr_sub_image = xr::SwapchainSubImage::<xr::OpenGL>::new()
                .swapchain(&xr_swapchains[view_idx])
                .image_array_index(0)
                .image_rect(xr::Rect2Di {
                    offset: xr::Offset2Di { x: 0, y: 0 },
                    extent: xr::Extent2Di {
                        width: xr_views[view_idx].recommended_image_rect_width as i32,
                        height: xr_views[view_idx].recommended_image_rect_height as i32,
                    },
                });

            let xr_proj_view = xr::CompositionLayerProjectionView::<xr::OpenGL>::new()
                .pose(xr_view_poses[view_idx].pose)
                .fov(xr_view_poses[view_idx].fov)
                .sub_image(xr_sub_image);

            xr_projection_views.push(xr_proj_view);
        }

        let layers = xr::CompositionLayerProjection::new()
            .space(&xr_play_space)
            .views(&xr_projection_views);

        xr_frame_stream.end(
            xr_frame_state.predicted_display_time,
            xr_environment_blend_mode,
            &[&layers],
        )?;

        // Update head position in server. This is done after all the display work, so that we
        // don't introduce latency
        client.set_head_pos(&head_from_xr_pose(&xr_view_poses[0].pose))?;
    }

    Ok(())
}

/// Compiles (*_SHADER, <source>) into a shader program for OpenGL
fn compile_glsl_program(gl: &gl::Context, sources: &[(u32, &str)]) -> Result<gl::Program> {
    // Compile default shaders
    unsafe {
        let program = gl.create_program().expect("Cannot create program");

        let mut shaders = vec![];

        for (stage, shader_source) in sources {
            let shader = gl.create_shader(*stage).expect("Cannot create shader");

            gl.shader_source(shader, shader_source);

            gl.compile_shader(shader);

            if !gl.get_shader_compile_status(shader) {
                bail!(
                    "Failed to compile shader;\n{}",
                    gl.get_shader_info_log(shader)
                );
            }

            gl.attach_shader(program, shader);

            shaders.push(shader);
        }

        gl.link_program(program);

        if !gl.get_program_link_status(program) {
            bail!("{}", gl.get_program_info_log(program));
        }

        for shader in shaders {
            gl.detach_shader(program, shader);
            gl.delete_shader(shader);
        }

        Ok(program)
    }
}

/*
 * According to their respective specifications, the
 * OpenXR and OpenGL APIs both use a **Right Handed** coordinate system.
 */

/// Creates a view matrix for the given pose
pub fn view_from_pose(pose: &xr::Posef) -> Matrix4<f32> {
    view_from_head(&head_from_xr_pose(pose))
}

/// Creates a projection matrix for the given fov
pub fn projection_from_fov(fov: &xr::Fovf, near: f32, far: f32) -> Matrix4<f32> {
    let tan_left = fov.angle_left.tan();
    let tan_right = fov.angle_right.tan();

    let tan_up = fov.angle_up.tan();
    let tan_down = fov.angle_down.tan();

    let tan_width = tan_right - tan_left;
    let tan_height = tan_up - tan_down;

    let a11 = 2.0 / tan_width;
    let a22 = 2.0 / tan_height;

    let a31 = (tan_right + tan_left) / tan_width;
    let a32 = (tan_up + tan_down) / tan_height;
    let a33 = -far / (far - near);

    let a43 = (far * near) / (far - near);

    Matrix4::new(
        a11, 0.0, a31, 0.0, //
        0.0, a22, a32, 0.0, //
        0.0, 0.0, a33, a43, //
        0.0, 0.0, -1.0, 0.0, //
    )
}

/// Creates a view matrix for the given pose
pub fn head_from_xr_pose(pose: &xr::Posef) -> Head {
    // Convert the rotation quaternion from OpenXR to nalgebra
    let orient = pose.orientation;
    let orient = Quaternion::new(orient.w, orient.x, orient.y, orient.z);
    let orient = Unit::try_new(orient, 0.0).expect("Not a unit orienternion");

    // Convert the position vector from OpenXR to nalgebra
    let pos = pose.position;
    let pos = Point3::new(pos.x, pos.y, pos.z);

    Head { pos, orient }
}

/// Creates a view matrix for the given head pose
pub fn view_from_head(head: &Head) -> Matrix4<f32> {
    // Invert this quaternion, orienting the world into NDC space
    let orient_inv = head.orient.inverse();

    // Represent the rotation in homogeneous coordinates
    let rotation = orient_inv.to_homogeneous();

    // Invert this translation, translating the world into NDC space
    let trans = Matrix4::new_translation(&-head.pos.coords);

    rotation * trans
}

struct Client {
    tcp_stream: TcpStream,
    msg_buf: AsyncBufferedReceiver,
    heads: Vec<Head>,
}

impl Client {
    /// Connect to server
    pub fn new(addr: SocketAddr) -> Result<Self> {
        let tcp_stream = TcpStream::connect(addr)?;
        tcp_stream.set_nonblocking(true)?;
        let msg_buf = AsyncBufferedReceiver::new();

        Ok(Self {
            tcp_stream,
            heads: vec![],
            msg_buf,
        })
    }

    /// Send our own head position
    pub fn set_head_pos(&mut self, head: &Head) -> Result<()> {
        Ok(cubehead::serialize_msg(head, &mut self.tcp_stream)?)
    }

    /// Get latest head positions
    pub fn update_heads(&mut self) -> Result<&[Head]> {
        self.poll()?;

        Ok(&self.heads)
    }

    /// Receive head positions of all players
    fn poll(&mut self) -> Result<()> {
        let mut latest = None;
        while let ReadState::Complete(msg) = self.msg_buf.read(&mut self.tcp_stream)? {
            latest = Some(msg);
        }

        if let Some(heads) = latest {
            self.heads = bincode::deserialize(&heads)?;
        }

        Ok(())
    }
}

fn head_matrices(heads: &[Head]) -> Vec<[[f32; 4]; 4]> {
    heads.iter().map(|head| *head.matrix().as_ref()).collect()
}

fn models() -> (Mesh, Mesh) {
    (big_quad_map(10.), rgb_cube(0.25))
}
