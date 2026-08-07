import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

const commandArguments = process.argv.slice(2);
const localMode = commandArguments.includes("--local");
const viteArguments = commandArguments.filter(
  (argument) => argument !== "--" && argument !== "--local",
);
const viteEntrypoint = fileURLToPath(
  new URL("../node_modules/vite/bin/vite.js", import.meta.url),
);

if (localMode) {
  viteArguments.push("--mode", "arcgis-local");
}

const viteProcess = spawn(
  process.execPath,
  [viteEntrypoint, ...viteArguments],
  { stdio: "inherit" },
);

viteProcess.on("exit", (exitCode) => {
  process.exitCode = exitCode ?? 1;
});
