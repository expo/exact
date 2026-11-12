// Clustered point and spot lights (lights.rs): records of 20 words, then a
// 16 x 9 x 24 grid of (first index word, count), then the clusters' light lists.
// A record: position, range; colour, intensity; direction, the cone's outer and
// inner cosines; shadow layer, matrix word, texel per metre (shadow_sample.wgsl);
// decay; the local shadow filter's three words (lights.rs `filter_words`).
@group(0) @binding(8) var<storage, read> light_words: array<u32>;
const PI = 3.141592653589793;
// Directions and the lobes' scalars in f32; colour in `shading` (frame.wgsl).
fn brdf(n: vec3<f32>, v: vec3<f32>, l: vec3<f32>, base: vec3<shading>, metallic: f32,
        roughness: f32) -> vec3<shading> {
    let nl = max(dot(n, l), 0.0);
    if MATTE { return base * shading(nl) / PI; }
    let nv = max(dot(n, v), 0.0001);
    let sum = v + l;
    let h = sum * inverseSqrt(max(dot(sum, sum), 0.000001));
    let nh = max(dot(n, h), 0.0);
    let vh = max(dot(v, h), 0.0);
    let a = roughness * roughness;
    let a2 = a * a;
    let d0 = nh * nh * (a2 - 1.0) + 1.0;
    let distribution = a2 / (PI * d0 * d0);
    // Height-correlated Smith-GGX visibility, including 1/(4 NoL NoV).
    let gv = nl * sqrt(nv * nv * (1.0 - a2) + a2);
    let gl = nv * sqrt(nl * nl * (1.0 - a2) + a2);
    let visibility = 0.5 / max(gv + gl, 0.00001);
    let f0 = mix(vec3<shading>(0.04), base, shading(metallic));
    // (1 - vh)^5 by multiplication: pow is two transcendental operations.
    let f = 1.0 - vh;
    let f2 = f * f;
    let fresnel = f0 + (vec3<shading>(1.0) - f0) * shading(f2 * f2 * f);
    let diffuse = (vec3<shading>(1.0) - fresnel) * shading(1.0 - metallic) * base / PI;
    let peak = shading(min(distribution * visibility, SHADING_LIMIT));
    return (diffuse + peak * fresnel) * shading(nl);
}
// A clearcoat and a sheen over the base lobe (`MaterialData::clearcoat`,
// `sheen_color`), compiled only into the model pipelines that draw them
// (`MODEL_LAYERED`); elsewhere `layered_brdf` is `brdf`. The model shader sets
// `surface_layer` before it lights a fragment: the coat's normal is the
// geometric one, since no coat normal map applies (KHR_materials_clearcoat,
// "Clearcoat": the base's normal map does not reach the coat).
override LAYERED: bool = false;
struct SurfaceLayer {
    coat: f32, coat_roughness: f32, sheen_roughness: f32, sheen: vec3<f32>, normal: vec3<f32>,
}
var<private> surface_layer: SurfaceLayer;
// KHR_materials_clearcoat, "Layering": the coat's Fresnel at NdotV with F0 0.04
// (ior 1.5), which weights the coat and attenuates the layers under it.
fn coat_fresnel(nv: f32) -> f32 {
    let f = 1.0 - nv;
    let f2 = f * f;
    return 0.04 + 0.96 * f2 * f2 * f;
}
// KHR_materials_sheen, "Sheen visibility": the Charlie visibility's curve fit.
fn sheen_l(x: f32, alpha: f32) -> f32 {
    let t = (1.0 - alpha) * (1.0 - alpha);
    let a = mix(21.5473, 25.3245, t);
    let b = mix(3.82987, 3.32435, t);
    let c = mix(0.19823, 0.16801, t);
    let d = mix(-1.97760, -1.27393, t);
    let e = mix(-4.32054, -4.85967, t);
    return a / (1.0 + b * pow(x, c)) + d * x + e;
}
fn sheen_lambda(cos_theta: f32, alpha: f32) -> f32 {
    if cos_theta < 0.5 { return exp(sheen_l(cos_theta, alpha)); }
    return exp(2.0 * sheen_l(0.5, alpha) - sheen_l(1.0 - cos_theta, alpha));
}
// The sheen's directional albedo E(NdotV), which KHR_materials_sheen leaves to a
// table: ln E fitted by least squares (weighted by E²) to this lobe's albedo,
// integrated numerically for roughness 0.07 to 1 and NdotV 0.02 to 1, as a
// quadratic in NdotV whose coefficients are cubics in 1 / roughness. Within
// about 0.04 of the integral for roughness ≥ 0.2 and NdotV ≥ 0.15; nearer
// grazing or smoother, up to about 0.13.
fn sheen_albedo(nv: f32, roughness: f32) -> f32 {
    let x = 1.0 / roughness;
    let c0 = -0.16762 + x * (0.02951 + x * (0.0052 - 0.00044 * x));
    let c1 = 0.42056 + x * (-2.67688 + x * (0.29206 - 0.00684 * x));
    let c2 = -0.97888 + x * (1.56655 + x * (-0.20615 - 0.02498 * x));
    return min(exp(c0 + nv * (c1 + nv * c2)), 1.0);
}
// The base lobe under a sheen, then under a clearcoat, lit from `l`.
fn layered_brdf(n: vec3<f32>, v: vec3<f32>, l: vec3<f32>, base: vec3<shading>, metallic: f32,
        roughness: f32) -> vec3<shading> {
    let lobe = brdf(n, v, l, base, metallic, roughness);
    if !LAYERED || MATTE { return lobe; }
    let nl = max(dot(n, l), 0.0001);
    let nv = max(dot(n, v), 0.0001);
    let sum = v + l;
    let h = sum * inverseSqrt(max(dot(sum, sum), 0.000001));
    let nh = max(dot(n, h), 0.0);
    // KHR_materials_sheen, "Sheen distribution" (Charlie, alpha = roughness²) and
    // "Sheen visibility" (Charlie); "Albedo-scaling technique", its simpler form,
    // scales the base by 1 - max3(sheenColor) E(NdotV).
    let alpha = surface_layer.sheen_roughness * surface_layer.sheen_roughness;
    let inv = 1.0 / alpha;
    let distribution = (2.0 + inv) * pow(max(1.0 - nh * nh, 0.0000001), inv * 0.5) / (2.0 * PI);
    // The visibility's 1/(4 NdotV NdotL) times NdotL, as `brdf`'s lobes are.
    let sheen_visibility = 1.0 / ((1.0 + sheen_lambda(nv, alpha) + sheen_lambda(nl, alpha)) * 4.0 * nv);
    let sheen = vec3<shading>(surface_layer.sheen) * shading(min(distribution * sheen_visibility, SHADING_LIMIT));
    let tallest = max(surface_layer.sheen.r, max(surface_layer.sheen.g, surface_layer.sheen.b));
    let scaling = shading(1.0 - tallest * sheen_albedo(nv, surface_layer.sheen_roughness));
    // KHR_materials_clearcoat, "Clearcoat BRDF": GGX with the coat's roughness and
    // the height-correlated Smith visibility of `brdf`, about the coat's normal;
    // "Layering": mix(material, clearcoat_brdf, clearcoat × fresnel).
    let nc = surface_layer.normal;
    let cl = max(dot(nc, l), 0.0001);
    let cv = max(dot(nc, v), 0.0001);
    let ch = max(dot(nc, h), 0.0);
    let a = surface_layer.coat_roughness * surface_layer.coat_roughness;
    let a2 = a * a;
    let d0 = ch * ch * (a2 - 1.0) + 1.0;
    let gv = cl * sqrt(cv * cv * (1.0 - a2) + a2);
    let gl = cv * sqrt(cl * cl * (1.0 - a2) + a2);
    let coat = min(a2 / (PI * d0 * d0) * 0.5 / max(gv + gl, 0.00001), SHADING_LIMIT) * max(dot(nc, l), 0.0);
    let fc = surface_layer.coat * coat_fresnel(cv);
    return (lobe * scaling + sheen * shading(f32(dot(n, l) > 0.0))) * shading(1.0 - fc) + vec3<shading>(shading(coat * fc));
}
fn light_f(at: u32) -> f32 { return bitcast<f32>(light_words[at]); }
fn light_v(at: u32) -> vec3<f32> { return vec3(light_f(at), light_f(at + 1u), light_f(at + 2u)); }
// The fragment's cluster: its screen tile and exponential view-depth slice.
fn light_cluster(frag: vec2<f32>, world: vec3<f32>) -> u32 {
    let tile = min(vec2<u32>(max(frag * vec2(16.0, 9.0) / frame.logical_size.xy, vec2(0.0))), vec2(15u, 8u));
    let depth = max(dot(frame.view_depth, vec4(world, 1.0)), 1e-6);
    let slice = u32(clamp(floor(log2(depth / frame.lights_info.y) * frame.lights_info.z), 0.0, 23.0));
    return (slice * 9u + tile.y) * 16u + tile.x;
}
// A spot's cone: full inside the inner cosine, smoothly zero at the outer.
fn light_cone(at: u32, l: vec3<f32>) -> f32 {
    let outer = light_f(at + 11u);
    if outer < -1.5 { return 1.0; }
    return smoothstep(outer, light_f(at + 12u), dot(-l, light_v(at + 8u)));
}
// A light's intensity at squared distance `d2`, and the divisor left to the
// shading float: intensity / d^decay. The default decay 2 divides by d² in
// the shading float, as it always has. Another divides in f32, where d^decay
// neither overflows nor vanishes as it can in f16; 0 skips the pow.
fn light_falloff(at: u32, d2: f32) -> vec2<f32> {
    let decay = light_f(at + 16u);
    let intensity = light_f(at + 7u);
    if decay == 2.0 { return vec2(intensity, d2); }
    if decay == 0.0 { return vec2(intensity, 1.0); }
    return vec2(intensity / pow(d2, decay * 0.5), 1.0);
}
// Add each of the fragment's cluster lights to `color`, in frame order. The
// model shader's historical evaluation order is kept for its pixels.
fn add_local_lights(color: ptr<function, vec3<shading>>, frag: vec2<f32>, world: vec3<f32>,
        n: vec3<f32>, v: vec3<f32>, base: vec3<shading>, metallic: f32, roughness: f32, model: bool) {
    if frame.lights_info.x == 0.0 { return; }
    let cell = u32(frame.lights_info.w) + light_cluster(frag, world) * 2u;
    let first = light_words[cell];
    let count = light_words[cell + 1u];
    for (var k = 0u; k < count; k++) {
        let at = light_words[first + k] * 20u;
        let delta = light_v(at) - world;
        let distance2 = max(dot(delta, delta), 0.0001);
        let range = max(light_f(at + 3u), 0.0001);
        let ratio2 = distance2 / (range * range);
        let window = max(1.0 - ratio2 * ratio2, 0.0);
        let l = delta * inverseSqrt(distance2);
        var cone = light_cone(at, l);
        // Shadow maps are read only where the light reaches.
        // A layered fragment's coat may face the light where its mapped normal does not.
        let facing = dot(n, l) > 0.0 || (LAYERED && dot(surface_layer.normal, l) > 0.0);
        if window > 0.0 && cone > 0.0 && facing {
            cone *= light_visibility(at, world, n, l);
        }
        // Nothing to add where the window or cone is zero (in f16 an
        // overflowed product times zero would not be zero).
        if window <= 0.0 || cone <= 0.0 { continue; }
        let tint = vec3<shading>(light_v(at + 4u));
        let lit = light_falloff(at, distance2);
        let intensity = shading(lit.x);
        let w = shading(window);
        let falloff = shading(lit.y);
        let c = shading(cone);
        if model {
            *color += layered_brdf(n, v, l, base, metallic, roughness) * tint * intensity * w * w / falloff * c;
        } else {
            let radiance = tint * intensity * w * w / falloff;
            *color += layered_brdf(n, v, l, base, metallic, roughness) * radiance * c;
        }
    }
}
// The second directional light, unshadowed.
fn fill_light(n: vec3<f32>, v: vec3<f32>, base: vec3<shading>, metallic: f32, roughness: f32) -> vec3<shading> {
    let l = (-frame.fill_direction_illuminance.xyz);
    return layered_brdf(n, v, l, base, metallic, roughness) * vec3<shading>(frame.fill_color.xyz)
        * shading(frame.fill_direction_illuminance.w);
}
