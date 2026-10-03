#version 100
precision highp float;
varying highp vec2 uv;
uniform sampler2D sourceScene;
void main() {
    gl_FragColor = texture2D(sourceScene, uv);
}
