// 步骤 1.5: 编写 WGSL 着色器

// 顶点着色器的输入，对应 Rust 中的 Vertex 结构体
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
};

// 顶点着色器的输出，会传递给片元着色器
// @builtin(position) 是必须的，它告诉 GPU 顶点最终的位置
// @location(0) 将颜色数据传递给片元着色器的 @location(0)
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
};

// 顶点着色器主函数
@vertex
fn vs_main(
    model: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(model.position, 1.0);
    out.color = model.color;
    return out;
}

// 片元着色器主函数
// 它接收来自顶点着色器的 VertexOutput (GPU 会自动进行插值)
// @location(0) 对应渲染管线中的 color_targets[0]
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // 直接输出插值后的颜色，alpha 为 1.0 (不透明)
    return vec4<f32>(in.color, 1.0);
}