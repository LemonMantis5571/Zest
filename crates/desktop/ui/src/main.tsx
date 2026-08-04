import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { ErrorBoundary } from "./components/ErrorBoundary.tsx";
import "./index.css";
import App from "./App.tsx";
import { markStartup, measureStartup } from "./lib/startupPerf.ts";

markStartup("ui-module");

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  </StrictMode>
);

markStartup("root-rendered");
measureStartup("root-rendered", "ui-module");
