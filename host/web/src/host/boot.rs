//! Boot entry points for a browser page.

use super::*;

impl<D: DataSource> Host<D> {
    /// Boot from plan bytes: decode (a validation pass), boot the runner, and
    /// produce the first batch, which creates the whole tree.
    pub fn boot(
        plan_bytes: &[u8],
        data: D,
        viewport: exact_runner::Viewport,
        launch: &str,
    ) -> Result<(Host<D>, String), HostError> {
        Host::boot_delivered(plan_bytes, data, None, Vec::new(), None, viewport, launch)
    }

    /// Boot with the page's snapshot of the app's kept secrets (LLP 1018
    /// D6): what `localStorage` holds under `exact.secret.<name>`, read by
    /// the glue before boot; the runner keeps the granted names. `facts` supplies
    /// the date, place and clock before the first settlement (LLP 1027.000.000 D2).
    pub fn boot_stored(
        plan_bytes: &[u8],
        data: D,
        snapshot: Vec<(String, Vec<u8>)>,
        viewport: exact_runner::Viewport,
        launch: &str,
        facts: exact_runner::time::Launch,
    ) -> Result<(Host<D>, String), HostError> {
        exact_runner::time::launching_with(facts, || {
            Host::boot_delivered(plan_bytes, data, None, snapshot, None, viewport, launch)
        })
    }

    /// Boot carrying an earlier host's state (the dev loop's reload, LLP
    /// 1007 §6): slots by name where their types still fit, settled
    /// resources where their arguments still match, the clock. Carried state
    /// is never why a boot fails — what no longer fits starts fresh.
    pub fn boot_with(
        plan_bytes: &[u8],
        data: D,
        carried: Option<&Carried>,
        viewport: exact_runner::Viewport,
        launch: &str,
    ) -> Result<(Host<D>, String), HostError> {
        Host::boot_delivered(
            plan_bytes,
            data,
            carried,
            Vec::new(),
            None,
            viewport,
            launch,
        )
    }

    /// Boot knowing what this wasm was built as (LLP 1030 D7): `compat` is
    /// the archive's `compat.json`, which the `delivery` resource answers
    /// from. The other three boots are this one with nothing to say.
    pub fn boot_delivered(
        plan_bytes: &[u8],
        data: D,
        carried: Option<&Carried>,
        snapshot: Vec<(String, Vec<u8>)>,
        compat: Option<&str>,
        viewport: exact_runner::Viewport,
        launch: &str,
    ) -> Result<(Host<D>, String), HostError> {
        Host::boot_linked(
            HostLinks::ALL,
            plan_bytes,
            data,
            carried,
            snapshot,
            compat,
            viewport,
            launch,
        )
    }

    /// [`Host::boot_delivered`], with what the artifact links (LLP 1047 D3).
    #[allow(clippy::too_many_arguments)]
    pub fn boot_linked(
        links: HostLinks<D>,
        plan_bytes: &[u8],
        data: D,
        carried: Option<&Carried>,
        snapshot: Vec<(String, Vec<u8>)>,
        compat: Option<&str>,
        viewport: exact_runner::Viewport,
        launch: &str,
    ) -> Result<(Host<D>, String), HostError> {
        let plan = decode_plan(plan_bytes).map_err(HostError::Plan)?;
        crate::link::admit(&plan)?;
        let kernel = browser_kernel();
        // @ref LLP 1039 D3 — both host facts precede the first settlement.
        let delivery = compat.map_or_else(Default::default, |json| {
            exact_runner::Delivery::default().with_compat(json)
        });
        let mut runner = Runner::boot_with_delivery_linked(
            crate::link::runner_links(),
            plan,
            data,
            kernel,
            carried,
            snapshot,
            delivery,
            viewport,
            launch,
        )
        .map_err(HostError::Runner)?;
        runner.measure_unless_production(compat, false); // LLP 1079 D1: the browser lays out
        runner.admit_live(compat);
        Host::open(links, runner, launch, Batch::new(), Default::default())
    }
}
