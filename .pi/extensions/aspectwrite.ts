import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { Type } from "typebox";
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// Anchor local assets to this extension, never to Pi's current directory.
const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const latexSubsetPath = resolve(projectRoot, "docs/latex-subset.md");

// Pi has no built-in MCP client. This small bridge starts the Rust stdio
// server only for a tool call, then shuts it down. The profile is never
// changed; an image is saved only when a destination is explicitly given.
export default function (pi: ExtensionAPI) {
  pi.registerTool({
    name: "render_latex",
    label: "Handwritten LaTeX",
    description: [
      "Render a CLOSED, strict ASCII LaTeX SUBSET as handwritten PNG; not general TeX. Unsupported syntax is rejected with a byte offset.",
      "Valid: literal Latin letters/digits and + - = < > / * . , : ; ! ?; grouped scripts x_{10}, x^{+};",
      "\\frac{a}{b}, \\sqrt{a}, \\text{plain words}, \\mathrm{H_2O}, \\unit{g}, \\left( ... \\right),",
      "\\times, \\Delta, \\alpha, \\int, \\sum, \\rightarrow, \\xrightarrow{label},",
      "spacing \\, \\; \\quad. A reaction arrow has exactly ONE braced label, NEVER an optional [below] label.",
      "Example: \\text{isoborneol }C_{10}H_{18}O\\xrightarrow{H^{+},\\,\\Delta}C_{10}H_{16}+H_2O.",
      "In top-level aligned, if any row uses &, every row must; use &\\text{...} for prose in the right column.",
      "NO preamble, $, environments except top-level aligned, packages, macros, \\ce, \\SI, \\dfrac, or arbitrary commands.",
      `For the full whitelist read ${latexSubsetPath}. Set output_path (relative to Pi's cwd, or absolute) to save a PNG for documents; download_filename saves to Downloads. Both return the image and saved path.`,
    ].join(" "),
    promptSnippet: "render_latex: CLOSED LaTeX subset; output_path saves PNG on disk for documents; download_filename saves to Downloads",
    promptGuidelines: [
      `Use render_latex for handwritten equations. It accepts a CLOSED subset, not arbitrary LaTeX; read ${latexSubsetPath} if unsure before calling. Never invent TeX commands, packages or optional reaction-arrow arguments.`,
      "For render_latex chemistry, write formulas as C_{10}H_{18}O (or \\mathrm{C_{10}H_{18}O}), prose as \\text{isoborneol}, and a labeled arrow as \\xrightarrow{H^{+},\\,\\Delta}. No [below] argument. Keep the entire request outside math-mode delimiters.",
      "In an aligned block, either omit & on every row or include it on every row; use &\\text{...} to put prose in the right column.",
      "If render_latex reports a LaTeX byte offset, revise the unsupported construct and retry; don't abandon handwritten rendering. Set output_path when the image will be referenced in a Markdown/HTML/document file; set download_filename only for Downloads. Use the returned saved path in the document. Existing files are never overwritten.",
    ],
    parameters: Type.Object({
      latex: Type.String({ maxLength: 8192 }),
      seed: Type.Optional(Type.Integer({ minimum: 0, maximum: 4294967295 })),
      download_filename: Type.Optional(Type.String({ description: "Save to Downloads under this simple PNG filename, e.g. equation.png. Mutually exclusive with output_path; never overwrites." })),
      output_path: Type.Optional(Type.String({ description: "Save PNG at this absolute path, or relative to Pi's current directory (e.g. assets/equation.png) to include in a document. Parent directories are created; existing files are never overwritten. Omit for image-only result." })),
    }),
    async execute(_id, params, signal, _update, ctx) {
      if (typeof params.latex !== "string" || !params.latex.trim()) {
        throw new Error(`render_latex requires a nonempty latex string (see ${latexSubsetPath})`);
      }
      if (Buffer.byteLength(params.latex) > 8192) throw new Error("LaTeX exceeds 8192 bytes");
      if (params.output_path !== undefined && params.download_filename !== undefined) {
        throw new Error("Choose output_path OR download_filename, not both");
      }
      if (params.output_path !== undefined &&
          (typeof params.output_path !== "string" || !params.output_path.trim() || params.output_path.length > 4096 || !params.output_path.toLowerCase().endsWith(".png"))) {
        throw new Error("output_path must be a nonempty .png file path");
      }
      if (params.download_filename !== undefined &&
          (typeof params.download_filename !== "string" || !/^[A-Za-z0-9][A-Za-z0-9._-]{0,100}\.png$/i.test(params.download_filename) || params.download_filename.includes(".."))) {
        throw new Error("download_filename must be a simple .png filename, not a path");
      }
      const profile = process.env.ASPECTWRITE_STROKES
        ? resolve(ctx.cwd, process.env.ASPECTWRITE_STROKES)
        : resolve(projectRoot, ".local/aspectwrite-strokes.json");
      const exe = process.env.ASPECTWRITE_BIN
        ? resolve(ctx.cwd, process.env.ASPECTWRITE_BIN)
        : resolve(projectRoot, "target", "debug", process.platform === "win32" ? "aspectwrite.exe" : "aspectwrite");
      const image = await new Promise<string>((ok, fail) => {
        const child = spawn(exe, ["mcp", profile], { cwd: projectRoot, windowsHide: true, stdio: ["pipe", "pipe", "pipe"] });
        let stdout = "";
        let stderr = "";
        let done = false;
        const finish = (error?: Error, data?: string) => {
          if (done) return;
          done = true;
          clearTimeout(timer);
          signal?.removeEventListener("abort", abort);
          child.kill();
          if (error) fail(error); else ok(data!);
        };
        const abort = () => finish(new Error("Rendering cancelled"));
        const timer = setTimeout(() => finish(new Error("MCP render timed out (30 seconds)")), 30_000);
        signal?.addEventListener("abort", abort, { once: true });
        if (signal?.aborted) abort();
        const send = (message: object) => {
          if (!done) child.stdin.write(JSON.stringify(message) + "\n");
        };
        child.on("error", (error) => finish(error));
        child.stdin.on("error", (error) => finish(error));
        child.on("close", (code) => finish(new Error(`MCP server exited (${code}): ${stderr || "no response"}`)));
        child.stderr.on("data", (chunk: Buffer) => { stderr = (stderr + chunk.toString()).slice(-4096); });
        child.stdout.on("data", (chunk: Buffer) => {
          stdout += chunk.toString("utf8");
          if (stdout.length > 30_000_000) return finish(new Error("MCP image exceeds 30 MB response limit"));
          let newline;
          while (!done && (newline = stdout.indexOf("\n")) >= 0) {
            const line = stdout.slice(0, newline);
            stdout = stdout.slice(newline + 1);
            try {
              const msg = JSON.parse(line);
              if (msg.id === 1) {
                if (msg.error) return finish(new Error(`MCP initialize: ${msg.error.message}`));
                send({ jsonrpc: "2.0", method: "notifications/initialized" });
                send({ jsonrpc: "2.0", id: 2, method: "tools/call", params: { name: "render_latex", arguments: { latex: params.latex, seed: params.seed } } });
              } else if (msg.id === 2) {
                if (msg.error || msg.result?.isError) {
                  return finish(new Error(msg.error?.message || msg.result?.content?.map((c: { text?: string }) => c.text).filter(Boolean).join("; ") || "MCP render failed"));
                }
                const image = msg.result?.content?.find((c: { type: string; mimeType?: string }) => c.type === "image" && c.mimeType === "image/png");
                if (!image?.data || typeof image.data !== "string") return finish(new Error("MCP returned no PNG image"));
                return finish(undefined, image.data);
              }
            } catch (error) {
              return finish(error instanceof Error ? error : new Error(String(error)));
            }
          }
        });
        send({ jsonrpc: "2.0", id: 1, method: "initialize", params: {
          protocolVersion: "2025-03-26", capabilities: {}, clientInfo: { name: "aspectwrite-pi", version: "1" },
        } });
      });
      const bytes = Buffer.from(image, "base64");
      if (bytes.length > 20_000_000 || bytes.subarray(0, 8).toString("hex") !== "89504e470d0a1a0a") {
        throw new Error("Invalid or oversized PNG from MCP server");
      }
      // Pi's installed ImageContent is {type, data, mimeType}, not the
      // {type, source} shape used by some other multimodal APIs.
      const content: Array<{ type: "text"; text: string } | { type: "image"; data: string; mimeType: string }> = [];
      const destination = params.output_path
        ? resolve(ctx.cwd, params.output_path)
        : params.download_filename ? join(homedir(), "Downloads", params.download_filename) : undefined;
      if (destination) {
        if (params.output_path) await mkdir(dirname(destination), { recursive: true });
        try {
          await writeFile(destination, bytes, { flag: "wx" }); // never overwrite user files
        } catch (error) {
          if ((error as NodeJS.ErrnoException).code === "EEXIST") {
            throw new Error(`PNG already exists at ${destination}; choose another filename (no overwrite)`);
          }
          throw error;
        }
        content.push({ type: "text", text: `Saved PNG to ${destination}` });
      }
      content.push({ type: "image", data: image, mimeType: "image/png" });
      return { content, details: { outputPath: destination } };
    },
  });
}
