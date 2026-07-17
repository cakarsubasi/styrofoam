static float2 positions[3] = float2[](
    float2(0.0, -0.5),
    float2(0.5, 0.5),
    float2(-0.5, 0.5)
);

static float3 colors[3] = float3[](
    float3(1.0, 0.0, 0.0),
    float3(0.0, 1.0, 0.0),
    float3(0.0, 0.0, 1.0),
);

struct VertexInput {
    float2 position;
    float3 color;
}

struct VertexOutput {
    float4 sv_position : SV_Position;
    float3 color;
};

[shader("vertex")]
VertexOutput triangle_vert(uint vid : SV_VertexID, uint mesh_id) {
    VertexInput input = getDescriptorFromHandle(StructuredBuffer<VertexInput>.Handle(uint2(mesh_id, 0)))[vid];
    VertexOutput output;
    output.sv_position = float4(input.position, 0.0, 1.0);
    output.color = input.color;
    return output;
}

[shader("fragment")]
float4 triangle_frag(VertexOutput vert) {
    return float4(vert.color, 1.0);
}
