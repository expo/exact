//! The little vector and matrix maths the viewer needs. Matrices are column-major `[[f32; 4]; 4]` (`m[column][row]`),
//! as WGSL's `mat4x4f` and glTF's `matrix` store them.

pub type V3 = [f32; 3];
pub type M4 = [[f32; 4]; 4];

pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub fn mul(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
pub fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub fn norm(a: V3) -> V3 {
    let scale = a.into_iter().map(f32::abs).fold(0.0, f32::max);
    if scale == 0.0 {
        return [0.0; 3];
    }
    let a = a.map(|v| v / scale);
    mul(a, 1.0 / dot(a, a).sqrt())
}

pub const IDENTITY: M4 = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

pub fn mat_mul(a: M4, b: M4) -> M4 {
    let mut o = [[0.0; 4]; 4];
    for c in 0..4 {
        for r in 0..4 {
            o[c][r] = (0..4).map(|k| a[k][r] * b[c][k]).sum();
        }
    }
    o
}

/// Right-handed perspective with depth 0..1 (WebGPU clip space).
pub fn perspective(fovy: f32, aspect: f32, near: f32, far: f32) -> M4 {
    let f = 1.0 / (fovy / 2.0).tan();
    [
        [f / aspect, 0.0, 0.0, 0.0],
        [0.0, f, 0.0, 0.0],
        [0.0, 0.0, far / (near - far), -1.0],
        [0.0, 0.0, near * far / (near - far), 0.0],
    ]
}

pub fn look_at(eye: V3, at: V3, up: V3) -> M4 {
    let f = norm(sub(at, eye));
    let s = norm(cross(f, up));
    let u = cross(s, f);
    [
        [s[0], u[0], -f[0], 0.0],
        [s[1], u[1], -f[1], 0.0],
        [s[2], u[2], -f[2], 0.0],
        [-dot(s, eye), -dot(u, eye), dot(f, eye), 1.0],
    ]
}

/// glTF node transform: translation, rotation (unit quaternion x, y, z, w), scale.
pub fn trs(t: V3, q: [f32; 4], s: V3) -> M4 {
    let (x, y, z, w) = (q[0], q[1], q[2], q[3]);
    let r = [
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y + z * w),
            2.0 * (x * z - y * w),
        ],
        [
            2.0 * (x * y - z * w),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z + x * w),
        ],
        [
            2.0 * (x * z + y * w),
            2.0 * (y * z - x * w),
            1.0 - 2.0 * (x * x + y * y),
        ],
    ];
    [
        [r[0][0] * s[0], r[0][1] * s[0], r[0][2] * s[0], 0.0],
        [r[1][0] * s[1], r[1][1] * s[1], r[1][2] * s[1], 0.0],
        [r[2][0] * s[2], r[2][1] * s[2], r[2][2] * s[2], 0.0],
        [t[0], t[1], t[2], 1.0],
    ]
}

pub fn transform_point(m: &M4, p: V3) -> V3 {
    [
        m[0][0] * p[0] + m[1][0] * p[1] + m[2][0] * p[2] + m[3][0],
        m[0][1] * p[0] + m[1][1] * p[1] + m[2][1] * p[2] + m[3][1],
        m[0][2] * p[0] + m[1][2] * p[1] + m[2][2] * p[2] + m[3][2],
    ]
}

/// Clip-space position (x, y, z, w) of a world point.
pub fn project(m: &M4, p: V3) -> [f32; 4] {
    let mut o = [0.0; 4];
    for (r, v) in o.iter_mut().enumerate() {
        *v = m[0][r] * p[0] + m[1][r] * p[1] + m[2][r] * p[2] + m[3][r];
    }
    o
}

/// The normal matrix of `m`'s upper 3×3: its cofactor matrix (det × inverse transpose), sign-corrected so normals keep
/// their side under a mirroring transform. Normals still need renormalising after it.
pub fn normal_matrix(m: &M4) -> [V3; 3] {
    let a = |c: usize, r: usize| m[c][r];
    let col = |c: usize| [a(c, 0), a(c, 1), a(c, 2)];
    let (c0, c1, c2) = (col(0), col(1), col(2));
    let (n0, n1, n2) = (cross(c1, c2), cross(c2, c0), cross(c0, c1));
    let det = dot(c0, n0);
    let s = if det < 0.0 { -1.0 } else { 1.0 };
    [mul(n0, s), mul(n1, s), mul(n2, s)]
}

/// Applies the cofactor columns and renormalizes the result.
pub fn transform_normal(nm: &[V3; 3], n: V3) -> V3 {
    norm(add(
        add(mul(nm[0], n[0]), mul(nm[1], n[1])),
        mul(nm[2], n[2]),
    ))
}
