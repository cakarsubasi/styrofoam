struct PushData {
    Ptr<float3> position;
    Ptr<VertexData> vertex_data;
    Ptr<Ubo> ubo;
    uint2 tex_id;
};
struct VertexData {
    float3 normal;
    float2 uv;
};
struct Ubo {
    float4x4 model;
    float4x4 view;
    float4x4 proj;
};

#ifdef DESCRIPTOR_HEAP
float4 sample_texture(uint2 id, float2 uv) {
    return Sampler2D.Handle(id)->Sample(uv);
}
#else
[[vk::binding(0, 0)]] Texture2D textures[];
[[vk::binding(0, 2)]] SamplerState samplers[];
float4 sample_texture(uint2 id, float2 uv) {
    return textures[NonUniformResourceIndex(id.x)].Sample(samplers[NonUniformResourceIndex(id.y)], uv);
}
#endif

struct VertexOutput {
    float4 sv_position : SV_Position;
    float3 normal;
    float2 texture_uv;
};


[shader("vertex")]
VertexOutput vertMain(
    uint vid: SV_VertexID,
    uniform PushData push
) {
    let position = float4(*(push.position + vid), 1.0);

    let m = mul(push.ubo->model, position);
    let mv = mul(push.ubo->view, m);
    let mvp = mul(push.ubo->proj, mv);
    let data = *(push.vertex_data + vid);
    VertexOutput output;
    output.sv_position = mvp;
    output.texture_uv = data.uv;
    output.normal = data.normal;
    return output;
}

[shader("fragment")]
float4 fragMain(
    VertexOutput vert,
    uniform PushData push
    ) : SV_TARGET
{
    return sample_texture(push.tex_id, vert.texture_uv);
}
