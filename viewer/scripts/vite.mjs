import { spawn } from "node:child_process";
import { copyFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const commandArguments = process.argv.slice(2);
const localMode = commandArguments.includes("--local");
const buildMode = commandArguments.includes("build");
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

viteProcess.on("exit", async (exitCode) => {
  if (exitCode !== 0) {
    process.exitCode = exitCode ?? 1;
    return;
  }

  if (buildMode) {
    try {
      await copyFile(
        new URL("../THIRD_PARTY_NOTICES.txt", import.meta.url),
        new URL("../dist/THIRD_PARTY_NOTICES.txt", import.meta.url),
      );
    } catch (error) {
      console.error("Failed to copy THIRD_PARTY_NOTICES.txt into dist.", error);
      process.exitCode = 1;
    }
  }
});
