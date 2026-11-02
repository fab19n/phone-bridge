import { lazy, Suspense, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { formatVersion } from "./lib/version";
import "./styles/global.css";

// The Stage 1 spikes page exists only in `--mode spikes` builds (npm run tauri:spikes). Vite replaces
// the env check with a constant, so normal builds drop the page and its chunk entirely.
const SpikesPage = import.meta.env.VITE_SPIKES === "true" ? lazy(() => import("./spikes/SpikesPage")) : null;

interface AppInfo {
  version: string;
  scrcpyServerVersion: string;
}

export default function App() {
  if (SpikesPage) {
    return (
      <Suspense fallback={null}>
        <SpikesPage />
      </Suspense>
    );
  }
  return <Home />;
}

function Home() {
  const [info, setInfo] = useState<AppInfo | null>(null);

  useEffect(() => {
    invoke<AppInfo>("app_info").then(setInfo).catch(console.error);
  }, []);

  return (
    <main style={{ padding: 24 }}>
      <h1 style={{ fontSize: 22, margin: "0 0 4px" }}>PhoneBridge</h1>
      <p style={{ color: "var(--muted)", margin: 0, fontSize: 14 }}>
        {info
          ? formatVersion(info.version, info.scrcpyServerVersion)
          : "Starting…"}
      </p>
    </main>
  );
}
