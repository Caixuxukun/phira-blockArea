#version 100
attribute vec3 position;
attribute vec4 color0;
varying highp vec2 uv;
varying lowp vec4 tint;
void main() {
    // All block passes use clip coordinates, independent of chart model stack.
    gl_Position = vec4(position.xy, 0.0, 1.0);
    uv = position.xy * 0.5 + 0.5;
    tint = color0;
}
