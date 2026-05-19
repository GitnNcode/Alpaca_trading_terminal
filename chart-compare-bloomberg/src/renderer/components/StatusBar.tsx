import React from "react";

interface Props {
  pyOnline: boolean;
  hasCreds: boolean | null;
  onAuth: () => void;
  time: string;
}

export function StatusBar({ pyOnline, hasCreds, onAuth, time }: Props) {
  return (
    <div className="ccb-statusbar">
      <span className="pill">
        <span className={"dot " + (pyOnline ? "" : "err")} />
        SIDECAR {pyOnline ? "ONLINE" : "OFFLINE"}
      </span>
      <span className="pill">
        <span className={"dot " + (hasCreds ? "" : "warn")} />
        CREDENTIALS {hasCreds ? "OK" : "MISSING"}
      </span>
      <span className="pill">
        <span className="dot" />
        DATA FEED · IEX
      </span>
      <span className="ccb-spacer" />
      <span onClick={onAuth} style={{ cursor: "pointer", color: "var(--ccb-fg)" }}>[AUTH]</span>
      <span>UTC {time}</span>
      <span style={{ color: "var(--ccb-fg-muted)" }}>v0.1.0</span>
    </div>
  );
}
