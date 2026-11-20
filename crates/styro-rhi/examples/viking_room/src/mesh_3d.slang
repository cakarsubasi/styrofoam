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
    float3 normal;
    float2 texture_uv;
};


[shader("vertex")]
VertexOutput vertMain(
    uint vid: SV_VertexID,
#ifdef DESCRIPTOR_HEAP
    uniform PushData push
#endif
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
#ifdef DESCRIPTOR_HEAP
    uniform PushData push
#endif
    ) : SV_TARGET
{
    return get_texture(push.tex_id).Sample(vert.texture_uv);
}
