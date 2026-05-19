import React, { useState } from "react";
import { api } from "../lib/api";

interface Props {
  onClose: () => void;
  onSaved: () => void;
}

export function CredentialsModal({ onClose, onSaved }: Props) {
  const [key, setKey] = useState("");
  const [secret, setSecret] = useState("");
  const [baseUrl, setBaseUrl] = useState("https://paper-api.alpaca.markets");
  const [err, setErr] = useState("");
  const [busy, setBusy] = useState(false);

  const save = async () => {
    setBusy(true);
    setErr("");
    try {
      await api.setCredentials(key.trim(), secret.trim(), baseUrl.trim());
      onSaved();
    } catch (e) {
      setErr(String(e instanceof Error ? e.message : e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="ccb-modal-bg" onClick={(e) => e.target === e.currentTarget && onClose()}>
      <div className="ccb-modal">
        <div className="hdr">AUTH · ALPACA API CREDENTIALS</div>
        <div className="body">
          <label>APCA-API-KEY-ID</label>
          <input value={key} onChange={(e) => setKey(e.target.value)} spellCheck={false} autoFocus />
          <label>APCA-API-SECRET-KEY</label>
          <input type="password" value={secret} onChange={(e) => setSecret(e.target.value)} spellCheck={false} />
          <label>BASE URL</label>
          <input value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} spellCheck={false} />
          <div className="err">{err}</div>
          <div style={{ color: "var(--ccb-fg-muted)", fontSize: 11, letterSpacing: "0.04em" }}>
            Stored at <span style={{ color: "var(--ccb-amber)" }}>~/Library/Application Support/alpaca-tui/credentials.json</span>
            (shared with the canonical Go terminal and Rust ports — credentials swap freely between builds).
          </div>
          <div className="actions">
            <button onClick={onClose}>CANCEL</button>
            <button className="active" onClick={save} disabled={busy || !key || !secret}>SAVE &lt;GO&gt;</button>
          </div>
        </div>
      </div>
    </div>
  );
}
