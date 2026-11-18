use crate::{binding::Binding, Picture, Source};
use exact_gpu::SurfaceError;
use exact_shader_uniforms::Builtins;
use std::rc::Rc;

#[derive(Default)]
enum File {
    #[default]
    None,
    Pending(String),
    Parsed(String, Rc<Source>),
    Failed(String, String),
}

#[derive(Default)]
enum Bound {
    #[default]
    Ok,
    Invalid(String),
}

#[derive(Default)]
pub(crate) struct State {
    source: File,
    binding: Bound,
    pub generation: u64,
    // Only a successful draw stamps this picture with the current generation.
    pub picture: Option<Picture>,
    pub preparing: Option<Picture>,
}

impl State {
    pub fn is_empty(&self) -> bool {
        matches!(self.source, File::None)
    }

    pub fn set_source(&mut self, path: &str) {
        self.preparing = None;
        self.binding = Bound::Ok;
        self.source = if path.is_empty() {
            self.picture = None;
            File::None
        } else {
            File::Pending(path.into())
        };
    }

    pub fn parsed(&mut self, path: &str, source: Source) {
        self.preparing = None;
        self.source = File::Parsed(path.into(), Rc::new(source));
    }

    pub fn asset_arriving(&mut self) {
        if !matches!(self.source, File::Pending(_)) {
            self.generation += 1;
        }
    }

    pub fn check_binding(&self, next: &Binding) -> Result<(), SurfaceError> {
        if let File::Parsed(path, source) = &self.source {
            if *path == next.src {
                next.pack(&source.layout, Builtins::default())?;
            }
        }
        Ok(())
    }

    pub fn bind(&mut self, arguments: &Binding) {
        let File::Parsed(_, source) = &self.source else {
            return;
        };
        self.binding = match arguments.pack(&source.layout, Builtins::default()) {
            Ok(_) => Bound::Ok,
            Err(error) => {
                self.preparing = None;
                Bound::Invalid(error.0)
            }
        };
        if matches!(self.binding, Bound::Invalid(_)) {
            return;
        }
        if let Some(picture) = self
            .preparing
            .as_mut()
            .filter(|p| Rc::ptr_eq(&p.source, source) && p.binding.src == arguments.src)
        {
            picture.binding = arguments.clone();
        } else if !self
            .picture
            .as_ref()
            .is_some_and(|p| Rc::ptr_eq(&p.source, source) && p.binding.src == arguments.src)
        {
            self.preparing = Some(Picture {
                generation: 0,
                source: source.clone(),
                binding: arguments.clone(),
                pipeline: None,
                uniform: None,
            });
        }
    }

    pub fn current(&self, picture: &Picture) -> bool {
        matches!((&self.source, &self.binding), (File::Parsed(path, source), Bound::Ok)
            if picture.binding.src == *path && Rc::ptr_eq(source, &picture.source))
    }

    pub fn bind_frame(&mut self, arguments: &Binding) {
        if self.picture.as_ref().is_some_and(|p| self.current(p)) {
            self.picture.as_mut().unwrap().binding = arguments.clone();
        }
    }

    pub fn fail(&mut self, error: String) {
        let path = self.path().to_string();
        self.preparing = None;
        self.source = File::Failed(path, error);
    }

    fn path(&self) -> &str {
        match &self.source {
            File::None => "",
            File::Pending(path) | File::Parsed(path, _) | File::Failed(path, _) => path,
        }
    }

    pub fn error(&self) -> Option<&str> {
        match (&self.source, &self.binding) {
            (File::Failed(_, error), _) | (_, Bound::Invalid(error)) => Some(error),
            _ => None,
        }
    }

    pub fn diagnostic(&self) -> String {
        let Some(message) = self.error() else {
            return String::new();
        };
        let diagnostic = message
            .split_once("does not validate: ")
            .map_or(message, |(_, error)| error);
        let location = diagnostic
            .lines()
            .find_map(|line| line.split_once("wgsl:").map(|(_, at)| at.trim()));
        let summary = diagnostic.lines().next().unwrap_or(diagnostic);
        let summary = summary.strip_prefix("error: ").unwrap_or(summary);
        match location {
            Some(at) => format!("{}:{at}: {summary}", self.path()),
            None => format!(
                "{}: {}",
                self.path(),
                diagnostic.split_whitespace().collect::<Vec<_>>().join(" ")
            ),
        }
    }

    pub fn drawn(&mut self) {
        if let Some(picture) = &mut self.picture {
            picture.generation = self.generation;
        }
    }

    pub fn frame_ready(&self) -> bool {
        matches!((&self.source, &self.binding, &self.picture),
            (File::Parsed(path, source), Bound::Ok, Some(picture))
            if picture.binding.src == *path && Rc::ptr_eq(source, &picture.source)
                && picture.generation == self.generation)
    }

    pub fn frame_presentable(&self) -> bool {
        self.picture.as_ref().is_some_and(|p| p.generation != 0)
    }
}
