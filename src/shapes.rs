use crate::render::{Mesh, Vertex};

pub fn big_quad_map(size: f32) -> Mesh {
    Mesh {
        indices: vec![0, 1, 3, 0, 3, 2],
        vertices: vec![
            Vertex::new([-size, 0., -size], [1., 0., 0.]),
            Vertex::new([-size, 0., size], [0., 1., 0.]),
            Vertex::new([size, 0., size], [0., 0., 1.]),
            Vertex::new([size, 0., -size], [1., 1., 1.]),
        ],
    }
}

pub fn rgb_cube(size: f32) -> Mesh {
    // We do a little golfing

    let mut indices = vec![];
    let mut vertices = vec![];

    for i in 0..3 {
        let mut color = [0.; 3];
        color[i] = 1.;

        for j in 0..2 {
            let sgn = if j == 0 { -size } else { size };

            let square = [
                [sgn, -size, -size],
                [sgn, -size, size],
                [sgn, size, size],
                [sgn, size, -size],
            ];

            let base = indices.len() as u32;

            for mut pos in square {
                pos.rotate_right(i);
                vertices.push(Vertex::new(pos, color));
            }

            let offsets = if j == 0 {
                [0, 1, 3, 0, 3, 2]
            } else {
                [0, 3, 1, 0, 2, 3]
            };

            indices.extend(&offsets.map(|i| i + base));
        }
    }

    Mesh { indices, vertices }
}
