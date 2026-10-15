import { appendFileSync } from "node:fs";
const LOG = process.env.PROBE_LOG;
export default { id: "trprobe", setup: async (ctx) => {
  let n = 0;
  await ctx.agent.transform((editor) => {
    n++;
    const b = editor.get("build");
    appendFileSync(LOG, JSON.stringify({ call: n, build: b?.permissions, plan: editor.get("plan")?.permissions }) + "\n");
  });
} };
