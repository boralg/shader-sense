#version 450

layout(points) in;
layout(line_strip, max_vertices = 2) out;

void main() {
    // Emit the input vertex
    gl_Position = gl_in[0].gl_Position;
    EmitVertex();

    // Emit a second vertex offset in Y
    gl_Position = gl_in[0].gl_Position + vec4(0.0, 0.5, 0.0, 0.0);
    EmitVertex();

    EndPrimitive();
}