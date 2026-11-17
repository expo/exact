//! Test-owned fonts enter before the first layout and retained worker catalog.
use super::*;

impl<D: DataSource> Presenter<D> {
    pub(crate) fn boot_with_fixture_fonts(
        plan: &[u8],
        data: D,
        viewport: (f32, f32),
        scale: f32,
        assets: PathBuf,
        choice: PainterChoice,
    ) -> Result<(Self, Option<String>), HostError> {
        Self::boot_fixture(plan, data, viewport, scale, assets, choice, None)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn boot_region_with_fixture_fonts(
        plan: &[u8],
        data: D,
        viewport: (f32, f32),
        scale: f32,
        assets: PathBuf,
        choice: PainterChoice,
        region: crate::content_region::ContentRegionRegistration,
    ) -> Result<(Self, Option<String>), HostError> {
        Self::boot_fixture(plan, data, viewport, scale, assets, choice, Some(region))
    }

    #[allow(clippy::too_many_arguments)]
    fn boot_fixture(
        plan: &[u8],
        data: D,
        viewport: (f32, f32),
        scale: f32,
        assets: PathBuf,
        choice: PainterChoice,
        region: Option<crate::content_region::ContentRegionRegistration>,
    ) -> Result<(Self, Option<String>), HostError> {
        let (mut p, error) = Self::boot_with_assets(
            PlanBytes::Copied(plan),
            data,
            viewport,
            scale,
            Assets::embedded(assets),
            PainterBoot::selected(choice),
            crate::text::fixtures::for_assets,
            None,
            "/",
            region,
        )?;
        p.measure();
        Ok((p, error))
    }
}
