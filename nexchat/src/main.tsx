import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import { AppShellErrorBoundary } from "@/ui/AppShellErrorBoundary";
import { initCapacitorShell } from "@/capacitor/init";
import {
  clearChunkReloadGuard,
  ensureFreshBundle,
  installChunkLoadRecovery,
} from "@/capacitor/versionCheck";
import { IntlProvider } from "@/i18n";
import "./styles.css";

installChunkLoadRecovery();

async function boot(): Promise<void> {
  await ensureFreshBundle(import.meta.env.BASE_URL);
  await initCapacitorShell();
  ReactDOM.createRoot(document.getElementById("root")!).render(
    <React.StrictMode>
      <IntlProvider>
        <AppShellErrorBoundary>
          <App />
        </AppShellErrorBoundary>
      </IntlProvider>
    </React.StrictMode>,
  );
  clearChunkReloadGuard();
}

void boot();
