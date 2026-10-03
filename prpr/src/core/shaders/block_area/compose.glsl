#version 100
precision highp float;
varying highp vec2 uv;
uniform sampler2D normalMask;
uniform sampler2D subtractMask;
uniform sampler2D displaceMap;
uniform float time;
uniform float isActive;
void main() {
    vec2 p = uv;
    if (isActive > 0.5) {
        // Unlit/BlockCompose, pass 0, using the APK material values.
        vec2 direction = vec2(0.70710678);
        vec2 perpendicular = vec2(-direction.y, direction.x);
        float t = time / 20.0 * 2.59;
        vec2 noiseUV = uv * vec2(2.13, 1.02);
        float a = texture2D(displaceMap, noiseUV + direction * t).r - 0.5;
        float b = texture2D(displaceMap, noiseUV + perpendicular * t).r - 0.5;
        p += (direction * a + perpendicular * b) * 0.1;
    }
    float a = texture2D(normalMask, p).r;
    float b = texture2D(subtractMask, p).r;
    float mask = abs(a - b);
    gl_FragColor = vec4(mask, b, 0.0, 1.0);
}
