#version 450

uniform mat4 view;
uniform mat4 proj;

layout (location = 0) in vec3 pos;
layout (location = 1) in vec3 color;

out vec4 f_color;

void main() {
    gl_Position = proj * view * vec4(pos, 1.0);
    f_color = vec4(color, 1.);
}

