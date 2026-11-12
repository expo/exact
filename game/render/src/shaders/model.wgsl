override MODEL_DISCARD: bool = true;
override MODEL_TEXTURES: bool = true;
struct BakedMaterial {
    base: vec4<f32>, surface: vec4<f32>, emission_cutoff: vec4<f32>, flags: vec4<f32>,
    uv: array<vec4<f32>, 10>,
    // clearcoat, its roughness, the sheen's roughness; the sheen's colour
    coat: vec4<f32>, sheen: vec4<f32>,
}
@group(2) @binding(0) var<uniform> baked: BakedMaterial;
@group(2) @binding(1) var base_texture: texture_2d<f32>;
@group(2) @binding(2) var base_sampler: sampler;
@group(2) @binding(3) var normal_texture: texture_2d<f32>;
@group(2) @binding(4) var normal_sampler: sampler;
@group(2) @binding(5) var mr_texture: texture_2d<f32>;
@group(2) @binding(6) var mr_sampler: sampler;
@group(2) @binding(7) var emission_texture: texture_2d<f32>;
@group(2) @binding(8) var emission_sampler: sampler;
@group(2) @binding(9) var ao_texture: texture_2d<f32>;
@group(2) @binding(10) var ao_sampler: sampler;
@vertex fn model_vs(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>, @location(3) color:vec4<f32>, @builtin(instance_index) instance:u32, @builtin(vertex_index) vertex:u32) -> ModelVarying {
    return model_transform(position,normal,uv,instance,vertex,color);
}
fn material_uv(uv: vec2<f32>, index:u32) -> vec2<f32> {
    let t=baked.uv[index*2u]; let o=baked.uv[index*2u+1u];
    return mat2x2(t.xy,t.zw)*uv+o.xy;
}
fn model_base(input:ModelVarying) -> vec4<f32> {
    var texel=vec4(1.0);
    if MODEL_TEXTURES { texel=textureSample(base_texture,base_sampler,material_uv(input.uv,0u)+input.uv_offset); }
    return texel*select(baked.base,vec4(1.0),input.glow.w>0.5)*input.color;
}
fn mapped_normal(input:ModelVarying, front:bool) -> vec3<f32> {
    let n=normalize(input.normal)*select(-1.0,1.0,front);
    if !MODEL_TEXTURES || baked.surface.z == 0.0 { return n; }
    let uv=material_uv(input.uv,1u);
    let dp1=dpdx(input.world); let dp2=dpdy(input.world);
    let duv1=dpdx(uv); let duv2=dpdy(uv);
    // Cotangent frame from screen derivatives: also covers absent baked tangents.
    let p2=cross(dp2,n); let p1=cross(n,dp1);
    let t=p2*duv1.x+p1*duv2.x; let b=p2*duv1.y+p1*duv2.y;
    let length2=max(dot(t,t),dot(b,b));
    // XY only, Z rebuilt: BC5 and two-channel ASTC carry no Z, and RGBA8 reads alike.
    let xy=textureSample(normal_texture,normal_sampler,uv).xy*2.0-1.0;
    let sampled=vec3(xy,sqrt(max(1.0-dot(xy,xy),0.0)));
    if length2 < 1e-16 { return n; }
    return normalize((t*sampled.x+b*sampled.y)*baked.surface.z*inverseSqrt(length2)+n*sampled.z);
}
// The environment's light on the layers over the base (`layered_brdf`): the
// coat's split-sum specular with F0 0.04 at its roughness, attenuating `lobe`
// by its Fresnel (KHR_materials_clearcoat, "Layering"), and the sheen's
// directional albedo times the diffuse irradiance, scaling `lobe` by
// 1 - max3(sheenColor) E(NdotV) (KHR_materials_sheen, "Albedo-scaling technique").
fn layered_ambient(n:vec3<f32>, v:vec3<f32>, lobe:vec3<shading>) -> vec3<shading> {
    let nv=max(dot(n,v),0.0001);
    let nc=surface_layer.normal;
    let cv=max(dot(nc,v),0.0001);
    let strength=shading(frame.zenith_ambient.w);
    let albedo=sheen_albedo(nv,surface_layer.sheen_roughness);
    let e=select(irradiance(n),max(sh(0u),vec3<shading>(0.0)),UNIFORM_AMBIENT);
    let sheen=e*vec3<shading>(surface_layer.sheen)*shading(albedo)*strength;
    let tallest=max(surface_layer.sheen.r,max(surface_layer.sheen.g,surface_layer.sheen.b));
    let ab=environment_brdf(shading(surface_layer.coat_roughness),shading(cv));
    let direction=select(reflect(-v,nc),vec3(0.0,1.0,0.0),UNIFORM_AMBIENT);
    let radiance=vec3<shading>(textureSampleLevel(environment_radiance,environment_sampler,direction,surface_layer.coat_roughness*ENVIRONMENT_LODS).rgb);
    let coat=radiance*(shading(0.04)*ab.x+ab.y)*strength*shading(surface_layer.coat);
    let fc=surface_layer.coat*coat_fresnel(cv);
    return (lobe*shading(1.0-tallest*albedo)+sheen)*shading(1.0-fc)+coat;
}
// An unlit material (`MaterialData::unlit`): its base and emission, which no
// light, environment or rim changes; fog and fading still apply.
@diagnostic(off, derivative_uniformity)
fn model_unlit(input:ModelVarying, base:vec4<f32>) -> vec4<f32> {
    var emission=vec3(1.0);
    if MODEL_TEXTURES { emission=textureSample(emission_texture,emission_sampler,material_uv(input.uv,3u)).rgb; }
    if MODEL_DISCARD && baked.flags.x==1.0 && base.a < baked.emission_cutoff.w { discard; }
    var color=vec3<shading>(base.rgb)+vec3<shading>(emission*baked.emission_cutoff.rgb*input.part_emission)+vec3<shading>(input.glow.xyz);
    if FOG { color=shaded_fog(color,input.world); }
    var alpha=1.0;
    if MODEL_DISCARD && baked.flags.x==2.0 { alpha=base.a*(1.0-fade_of(input.slot)); }
    return vec4(vec3<f32>(color),alpha);
}
@diagnostic(off, derivative_uniformity)
fn model_shade(input:ModelVarying, front:bool, visibility:f32) -> vec4<f32> {
    if DEPTH_PREPASS && input.clip.z >= 1.0 { discard; }
    // Opaque materials fade by dither; blended ones multiply alpha below.
    if MODEL_DISCARD && baked.flags.x!=2.0 && faded(input.slot,input.clip.xy) { discard; }
    let base=model_base(input);
    if baked.flags.z > 0.0 { return model_unlit(input,base); }
    var mr=vec4(1.0);
    var emission=vec3(1.0);
    var ao=1.0;
    if MODEL_TEXTURES {
        mr=textureSample(mr_texture,mr_sampler,material_uv(input.uv,2u));
        emission=textureSample(emission_texture,emission_sampler,material_uv(input.uv,3u)+input.uv_offset).rgb;
        ao=mix(1.0,textureSample(ao_texture,ao_sampler,material_uv(input.uv,4u)).r,baked.surface.w);
    }
    let n=mapped_normal(input,front);
    if MODEL_DISCARD && baked.flags.x==1.0 && base.a < baked.emission_cutoff.w { discard; }
    let metallic=clamp(baked.surface.x*mr.b*input.part.x,0.0,1.0);
    let roughness=clamp(baked.surface.y*mr.g*input.part.y,0.045,1.0);
    let v=view_direction(input.world);
    // The colour in `shading` (frame.wgsl): f16 where the device has it.
    let albedo=vec3<shading>(base.rgb);
    var environment=ambient(n,v,albedo,metallic,roughness);
    var emitted=vec3<shading>(emission*baked.emission_cutoff.rgb*input.part_emission);
    // The instance's added emission (its material, node and override colours).
    var glow=vec3<shading>(input.glow.xyz);
    if LAYERED && !MATTE {
        // The roughness floors keep the coat's GGX peak and the sheen's 1/alpha finite.
        let coat_normal=normalize(input.normal)*select(-1.0,1.0,front);
        surface_layer=SurfaceLayer(clamp(baked.coat.x*select(1.0,mr.a,baked.coat.w>0.0),0.0,1.0),clamp(baked.coat.y,0.045,1.0),clamp(baked.coat.z,0.07,1.0),max(baked.sheen.rgb,vec3(0.0)),coat_normal);
        environment=layered_ambient(n,v,environment);
        // KHR_materials_clearcoat, "Emission": the coat darkens what the base emits.
        let under=shading(1.0-surface_layer.coat*coat_fresnel(max(dot(coat_normal,v),0.0001)));
        emitted*=under;
        glow*=under;
    }
    var color=environment*shading(ao)+emitted+glow;
    if frame.sun_direction_illuminance.w>0.0 {
        color+=layered_brdf(n,v,(-frame.sun_direction_illuminance.xyz),albedo,metallic,roughness)*vec3<shading>(frame.sun_color_count.xyz)*shading(frame.sun_direction_illuminance.w)*shading(visibility*water_caustics(input.world));
    }
    if LIGHTS && frame.fill_direction_illuminance.w>0.0 { color+=fill_light(n,v,albedo,metallic,roughness); }
    if LIGHTS { add_local_lights(&color,input.clip.xy,input.world,n,v,albedo,metallic,roughness,true); }
    if baked.flags.y > 0.0 {
        color += shading(baked.flags.y * input.part.z * pow(1.0 - clamp(dot(n,v),0.0,1.0),3.0)) * mix(albedo,vec3<shading>(1.0),0.2);
    }
    if FOG { color=shaded_fog(color,input.world); }
    var alpha=1.0;
    if MODEL_DISCARD && baked.flags.x==2.0 { alpha=base.a*(1.0-fade_of(input.slot)); }
    return vec4(vec3<f32>(color),alpha);
}
@fragment fn model_fs(input:ModelVarying,@builtin(front_facing) front:bool)->@location(0) vec4<f32> {return finish(model_shade(input,front,1.0));}
@fragment fn model_fs_shadow(input:ModelVarying,@builtin(front_facing) front:bool)->@location(0) vec4<f32> {return finish(model_shade(input,front,sun_visibility(input.world,normalize(input.normal))));}

@fragment fn model_depth_fs(input:ModelVarying) {
    if faded(input.slot,input.clip.xy) { discard; }
    if baked.flags.x==1.0 && model_base(input).a < baked.emission_cutoff.w { discard; }
}
