#version 450

layout(location = 0) out vec4 renderTarget;

void PSMain()
{
    // Output a solid magenta color
    renderTarget = vec4(1.0, 0.0, 1.0, 1.0);
}