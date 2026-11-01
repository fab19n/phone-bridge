import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { formatVersion } from "./lib/version";
import "./styles/global.css";

interface AppInfo {
  version: string;
  scrcpyServerVersion: string;
}

export default function App() {
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
