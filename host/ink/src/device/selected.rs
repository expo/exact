use super::*;

/// The boot choice; both layers consume the same core's frames and resources.
#[whichever::dispatch(from = false)]
pub enum Selected {
    #[dispatch(deref)]
    Wgpu(Box<wgpu::Wgpu>),
    #[cfg(windows)]
    #[dispatch(deref)]
    D3d11(Box<d3d11::D3d11>),
    #[cfg(exact_ink_metal)]
    #[dispatch(deref)]
    Metal(Box<metal::Metal>),
}

#[whichever::dispatch]
pub enum Buffer {
    Wgpu(wgpu::Buffer),
    #[cfg(windows)]
    D3d11(d3d11::Buffer),
    #[cfg(exact_ink_metal)]
    Metal(metal::Buffer),
}

#[whichever::dispatch]
impl Selected {
    #[cfg(target_os = "macos")]
    #[dispatch(*)]
    pub fn attach_layer(&mut self, layer: apple::Layer) -> Result<()>;

    #[cfg(target_os = "macos")]
    #[dispatch(*)]
    pub fn has_layer(&self) -> bool;

    #[cfg(target_os = "macos")]
    #[dispatch(*)]
    pub fn present(&mut self, texture: TextureId) -> Result<f64>;

    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_env() -> Result<Self> {
        let value = std::env::var("EXACT_INK_DEVICE").ok();
        Self::open(value.as_deref())
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn open(value: Option<&str>) -> Result<Self> {
        #[cfg(windows)]
        if let Some(warp) = direct_d3d11(value) {
            return d3d11::D3d11::open_with_fallback(warp).map(|d| Self::D3d11(Box::new(d)));
        }
        match value {
            #[cfg(not(windows))]
            None => wgpu::Wgpu::new().map(Into::into),
            Some("wgpu") => wgpu::Wgpu::new().map(Into::into),
            Some("metal") => {
                #[cfg(exact_ink_metal)]
                {
                    metal::Metal::new().map(|d| Self::Metal(Box::new(d)))
                }
                #[cfg(not(exact_ink_metal))]
                {
                    Err(Error::Unsupported(
                        "direct Metal requires an Apple target built with the Metal toolchain",
                    ))
                }
            }
            _ => Err(Error::Invalid(
                "EXACT_INK_DEVICE must be wgpu, metal, d3d11 or warp".into(),
            )),
        }
    }
}

impl From<wgpu::Wgpu> for Selected {
    fn from(device: wgpu::Wgpu) -> Self {
        Self::Wgpu(Box::new(device))
    }
}

/// A buffer belongs to the layer that made it, so `upload_buffer` is written
/// here; every other method is forwarded to the selected layer.
#[whichever::dispatch]
impl Device for Selected {
    type Buffer = Buffer;
    fn upload_buffer(&mut self, buffer: &Buffer, offset: u64, data: &[u8]) -> Result<Submission> {
        match (self, buffer) {
            (Self::Wgpu(d), Buffer::Wgpu(b)) => d.upload_buffer(b, offset, data),
            #[cfg(exact_ink_metal)]
            (Self::Metal(d), Buffer::Metal(b)) => d.upload_buffer(b, offset, data),
            #[cfg(windows)]
            (Self::D3d11(d), Buffer::D3d11(b)) => d.upload_buffer(b, offset, data),
            #[cfg(any(exact_ink_metal, windows))]
            _ => Err(Error::Invalid(
                "buffer belongs to another device layer".into(),
            )),
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[test]
fn unspecified_device_uses_the_platform_default() {
    let device = Selected::open(None).unwrap();
    #[cfg(windows)]
    assert!(matches!(device, Selected::D3d11(_)));
    #[cfg(not(windows))]
    assert!(matches!(device, Selected::Wgpu(_)));
    assert!(matches!(
        Selected::open(Some("wgpu")).unwrap(),
        Selected::Wgpu(_)
    ));
}
