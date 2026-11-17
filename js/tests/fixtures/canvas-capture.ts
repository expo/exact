const API = "https://context.test";
(globalThis as any).exact = {
  abi: 1, appId: "test.context", grants: "net.fetch https://context.test\nfs.write app:/tmp",
  answer: (_source: string, [seed]: number[]) => seed ? captureHttp() : { text: "" },
};

async function captureHttp(): Promise<{ text: string }> {
  const http = fetch(`${API}/slow`, { exactIndependentHttp: { maxResponseBytes: 1024 } }).then(r => r.text());
  const capture = (globalThis as any).captureCanvas("ink", "app:/tmp/ink.png");
  const [text, image] = await Promise.all([http, capture]);
  return { text: `${text}/${image.path}` };
}
