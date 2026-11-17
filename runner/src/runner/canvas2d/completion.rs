//! Canvas draw completion authority (LLP 1172.019 §2 item 1, Carry).

use super::*;

impl<D: DataSource> Runner<D> {
    /// The reusable authority bound when this canvas generation first drew.
    /// A new size generation gets a new subscription; draw sequence checks
    /// remain in the canvas engine (LLP 1172.019 §2 item 1, Carry).
    pub fn canvas_draw_completion(
        &self,
        lifetime: u64,
        generation: u32,
    ) -> Option<CompletionSubscription> {
        self.canvases
            .as_ref()?
            .completion(lifetime, generation)?
            .subscription()
    }

    /// A draw's reply (LLP 1056 D4). Applied, in order, only if its stamps
    /// are the canvas's live lifetime, generation and the draw in flight;
    /// otherwise discarded whole, with the recorder state it produced.
    /// A reply that fails the structural check starts a new generation, so
    /// the recorder and the host cannot diverge. Returns why a reply was
    /// discarded.
    pub fn canvas_reply(
        &mut self,
        lifetime: u64,
        generation: u32,
        seq: u64,
        reply: DrawReply,
    ) -> Result<(), String> {
        let capability = self
            .canvases
            .as_ref()
            .and_then(|c| c.completion(lifetime, generation))
            .and_then(|binding| binding.delivery());
        match capability {
            Some(capability) => self
                .deliver(
                    capability,
                    Completion::CanvasDraw {
                        lifetime,
                        generation,
                        seq,
                        reply,
                    },
                )
                .map(|_| ())
                .map_err(|error| match error {
                    RunnerError::CanvasDraw(why) => why,
                    _ => unreachable!("canvas delivery returns a canvas error"),
                }),
            None => self.canvas_reply_now(lifetime, generation, seq, reply),
        }
    }

    pub(in crate::runner) fn canvas_reply_now(
        &mut self,
        lifetime: u64,
        generation: u32,
        seq: u64,
        reply: DrawReply,
    ) -> Result<(), String> {
        let now = self.now_ms;
        let Some(c) = self.canvases.as_mut() else {
            return Err(format!("canvas {lifetime} is gone"));
        };
        let mut lines = Vec::new();
        let result = c.reply(lifetime, generation, seq, reply, now, &mut |line| {
            lines.push(line)
        });
        for line in lines {
            self.log(line);
        }
        self.complete_canvas_captures();
        result
    }
}
