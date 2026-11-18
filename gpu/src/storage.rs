use crate::Module;
use std::path::Path;

impl Module {
    /// Give a canvas its session's resolved directories before its first bind.
    /// The module neither retains paths nor creates directories.
    pub fn storage(&mut self, id: u32, data: &Path, cache: &Path, temporary: &Path) -> bool {
        let Some(inst) = self.instances.get_mut(&id) else {
            self.error = "no such canvas".into();
            return false;
        };
        if inst.bound {
            self.error = "storage must precede the first bind".into();
            return false;
        }
        if [data, cache, temporary].iter().any(|p| !p.is_absolute()) {
            self.error = "storage needs three absolute directory paths".into();
            return false;
        }
        inst.surface.storage(data, cache, temporary);
        inst.drain();
        true
    }
}

#[cfg(test)]
mod tests {
    use crate::{wgpu, Frame, Module, Registry, Surface, SurfaceError, Value};
    use std::path::Path;

    #[derive(Default)]
    struct Files(String);
    impl Surface for Files {
        fn storage(&mut self, data: &Path, cache: &Path, temporary: &Path) {
            self.0 = format!(
                "{}|{}|{}",
                data.display(),
                cache.display(),
                temporary.display()
            );
        }
        fn bind(&mut self, _: &[Value], _: Option<f64>) -> Result<(), SurfaceError> {
            assert!(!self.0.is_empty(), "storage precedes the first bind");
            Ok(())
        }
        fn agent(&mut self, _: &str) -> Option<String> {
            Some(self.0.clone())
        }
        fn render(
            &mut self,
            _: &Frame,
            _: &wgpu::Device,
            _: &wgpu::Queue,
            _: &mut wgpu::CommandEncoder,
            _: &wgpu::TextureView,
            _: wgpu::TextureFormat,
        ) -> bool {
            false
        }
    }
    static REGISTRY: Registry = Registry {
        surfaces: &[("files", 0, || Box::<Files>::default())],
        shaders: &[],
    };

    #[test]
    fn storage_belongs_to_the_canvas_and_precedes_binding() {
        let mut module = Module::new(&REGISTRY);
        let a = module.create_headless("files").unwrap();
        let b = module.create_headless("files").unwrap();
        for (id, store) in [(a, "first"), (b, "second")] {
            let root = std::env::current_dir().unwrap().join(store);
            assert!(module.storage(
                id,
                &root.join("data"),
                &root.join("cache"),
                &root.join("temporary")
            ));
            assert!(module.bind(id, &[], None));
        }
        let before = module.agent(a, "").unwrap();
        assert!(before.contains("first"));
        assert!(module.agent(b, "").unwrap().contains("second"));
        assert!(!module.storage(
            a,
            Path::new("/other"),
            Path::new("/cache"),
            Path::new("/temporary")
        ));
        assert_eq!(module.take_error(), "storage must precede the first bind");
        assert_eq!(module.agent(a, "").unwrap(), before);
        module.destroy(a);
        assert!(!module.storage(
            a,
            Path::new("/data"),
            Path::new("/cache"),
            Path::new("/temporary")
        ));
        assert_eq!(module.take_error(), "no such canvas");
        let c = module.create_headless("files").unwrap();
        assert!(!module.storage(
            c,
            Path::new("relative"),
            Path::new("/cache"),
            Path::new("/temporary")
        ));
        assert_eq!(
            module.take_error(),
            "storage needs three absolute directory paths"
        );
    }
}
