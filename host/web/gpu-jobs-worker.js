// This instance runs presentation jobs without loading a device (LLP 1182.000 D9).
export function serveJobs(scope, importGlue = url => import(url)) {
  let initialized = false, run, memory;
  return async ({ data }) => {
    if (data.init === true) {
      if (initialized || data.version !== 1) throw new Error('invalid GPU worker init');
      initialized = true;
      const glue = await importGlue(data.glue);
      ({ memory } = await glue.default({ module_or_path: data.module }));
      run = glue.exact_game_worker_run;
      scope.postMessage({ ready: true, version: 1, names: glue.exact_game_worker_names() });
      return;
    }
    if (data.run !== true || !run) throw new Error('GPU worker was not initialized');
    for (const { attempt, name, input } of data.jobs) {
      const start = scope.performance.now();
      let inRun = false, output;
      try {
        output = run(name, input, active => { inRun = active; });
      } catch (error) {
        if (!inRun || !(error instanceof WebAssembly.RuntimeError)) throw error;
        scope.postMessage({ trapped: true, attempt });
        scope.close();
        return;
      }
      scope.postMessage({
        done: true, attempt, output, ms: scope.performance.now() - start, memory: memory.buffer.byteLength,
      }, [output.buffer]);
    }
  };
}

if (typeof WorkerGlobalScope !== 'undefined' && globalThis instanceof WorkerGlobalScope) {
  const handle = serveJobs(globalThis);
  globalThis.onmessage = event => {
    // An unhandled rejection in the worker does not reach the owner's error handler.
    handle(event).catch(error => setTimeout(() => { throw error; }, 0));
  };
}
