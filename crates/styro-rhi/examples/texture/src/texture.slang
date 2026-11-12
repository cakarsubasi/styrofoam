struct PushData {
    uint2 tex_id;
};

#ifdef DESCRIPTOR_HEAP
Sampler2D get_texture(uint2 id) {
    return *Sampler2D.Handle(id);
}
#else
[[vk::binding(0, 0)]] Sampler2D textures[];
[[vk::push_constant]] ConstantBuffer<PushData> push;
Sampler2D get_texture(uint2 id) {
    return textures[NonUniformResourceIndex(id.x)];
}
#endif

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
VertexOutput vertMain(uint vid: SV_VertexID) {
    let position = float4(positions[vid], 0.0, 1.0);
    VertexOutput output;
    output.sv_position = position;
    output.texture_uv = position.xy + 0.5;
    return output;
}

[shader("fragment")]
float4 fragMain(
    VertexOutput vert,
#ifdef DESCRIPTOR_HEAP
    uniform PushData push
#endif
    ) : SV_TARGET
{
    return get_texture(push.tex_id).Sample(vert.texture_uv);
}
