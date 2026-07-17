struct VertexOutput {
    float2 sv_position : SV_Position;
};
static float2 positions[3] = float2[](
    float2(0.0, -0.5),
    float2(0.5, 0.5),
    float2(-0.5, 0.5)
);


[shader("vertex")]
VertexOutput vertMain(
    uint vid: SV_VertexID,
    uniform uint2 tex_id,
) {
    let position = positions[vid];
    VertexOutput output;
    output.sv_position = position;
    return output;
}

[shader("fragment")]
float4 fragMain(VertexOutput vert,
    uniform uint2 tex_id,
) : SV_TARGET {
    Sampler2D tex = *Sampler2D.Handle(tex_id);
    int height;
    int width;
    int levels;

    let color = tex.Sample(vert.sv_position);

    return color;
}
