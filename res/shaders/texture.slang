struct VertexOutput {
    float4 sv_position : SV_Position;
    float2 texture_uv;
};
static float2 positions[4] = float2[](
    float2(-0.5, -0.5),
    float2(0.5, -0.5),
    float2(0.5, 0.5),
    float2(-0.5, 0.5),
);

[shader("vertex")]
VertexOutput vertMain(
    uint vid: SV_VertexID,
    uniform Sampler2D tex,
) {
    let position = float4(positions[vid], 0.0, 1.0);
    VertexOutput output;
    output.sv_position = position;
    output.texture_uv = (position.xy + 0.5);
    return output;
}

[shader("fragment")]
float4 fragMain(VertexOutput vert,
    uniform Sampler2D tex,
) : SV_TARGET {
    return tex.Sample(vert.texture_uv);
}
