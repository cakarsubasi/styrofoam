
struct VertexInput  {
    float2 position : SV_POSITION;
    float2 uv : TEX_COORD0;
    uint color : COLOR;
};

struct VertexOutput {
    float2 sv_position : SV_Position;
    float2 uv;
    uint color;
};


[shader("vertex")]
VertexOutput vertMain(
    uint vid: SV_VertexID,
    uniform VertexInput* vertex_buffer,
    uniform uint2 tex_id,
) {
    VertexInput input = vertex_buffer[vid];
    VertexOutput output;
    output.sv_position = input.position;
    output.color = input.color;
    output.uv = input.uv;
    return output;
}

[shader("fragment")]
float4 fragMain(VertexOutput vert,
    uniform VertexInput* vertex_buffer,
    uniform uint2 tex_id,
) : SV_TARGET {
    Sampler2D tex = *Sampler2D.Handle(tex_id);
    int height;
    int width;
    int levels;

    let color = tex.Sample(vert.uv);

    return vert.color * color;
}
