import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

const localArcgisCoreUrl = "http://localhost:3000/@arcgis/core/";

export default defineConfig(({ mode }) => ({
  plugins: [
    ...(mode === "arcgis-local"
      ? [
          {
            name: "local-arcgis-core",
            enforce: "pre" as const,
            resolveId(source: string) {
              if (source.startsWith("@arcgis/core/")) {
                const modulePath = source.slice("@arcgis/core/".length);
                return {
                  id: `${localArcgisCoreUrl}${modulePath.endsWith(".js") ? modulePath : `${modulePath}.js`}`,
                  external: true,
                };
              }
            },
          },
        ]
      : []),
    react(),
  ],
  optimizeDeps: {
    exclude: ["@arcgis/core", "@arcgis/map-components"],
  },
}));
