#version 100
precision highp float;
varying highp vec2 uv;
uniform sampler2D mask;
uniform vec2 texel;
void main() {
    float center = texture2D(mask, uv).r;
    float edge = 0.0;
    float glow = 0.0;
    // Square dilation rings. R is the outer edge, G the weighted glow.
    for (int i = 1; i <= 8; i++) {
        vec2 d = texel * float(i);
        float ring = max(max(texture2D(mask, uv + vec2(d.x, 0.0)).r,
                             texture2D(mask, uv - vec2(d.x, 0.0)).r),
                         max(texture2D(mask, uv + vec2(0.0, d.y)).r,
                             texture2D(mask, uv - vec2(0.0, d.y)).r));
        ring = max(ring, max(max(texture2D(mask, uv + d).r, texture2D(mask, uv - d).r),
                             max(texture2D(mask, uv + vec2(d.x, -d.y)).r,
                                 texture2D(mask, uv + vec2(-d.x, d.y)).r)));
        if (i <= 2) edge = max(edge, ring);
        glow += max(0.0, ring - center) * (9.0 - float(i)) / 36.0;
    }
    gl_FragColor = vec4(max(0.0, edge - center), glow, 0.0, 1.0);
}
