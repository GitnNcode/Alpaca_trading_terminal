"""Credentials store. Shared with the canonical Go app + Rust ports.

Path convention (intentionally identical across all builds so binaries can swap
without re-entering keys):
  macOS:   ~/Library/Application Support/alpaca-tui/credentials.json
  Windows: %APPDATA%\\alpaca-tui\\credentials.json
  Linux:   ~/.config/alpaca-tui/credentials.json
"""

from __future__ import annotations

import json
import os
import sys
from dataclasses import dataclass
from pathlib import Path


@dataclass
class Credentials:
    api_key: str
    api_secret: str
    base_url: str = "https://paper-api.alpaca.markets"

    def to_dict(self) -> dict[str, str]:
        return {
            "api_key": self.api_key,
            "api_secret": self.api_secret,
            "base_url": self.base_url,
        }


def credentials_dir() -> Path:
    if sys.platform == "darwin":
        return Path.home() / "Library" / "Application Support" / "alpaca-tui"
    if sys.platform.startswith("win"):
        appdata = os.environ.get("APPDATA")
        if appdata:
            return Path(appdata) / "alpaca-tui"
        return Path.home() / "AppData" / "Roaming" / "alpaca-tui"
    return Path.home() / ".config" / "alpaca-tui"


def credentials_path() -> Path:
    return credentials_dir() / "credentials.json"


def load() -> Credentials | None:
    path = credentials_path()
    if not path.exists():
        return None
    try:
        raw = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError):
        return None
    key = raw.get("api_key") or raw.get("APCA-API-KEY-ID") or ""
    secret = raw.get("api_secret") or raw.get("APCA-API-SECRET-KEY") or ""
    base = raw.get("base_url") or "https://paper-api.alpaca.markets"
    if not key or not secret:
        return None
    return Credentials(api_key=key, api_secret=secret, base_url=base)


def save(creds: Credentials) -> None:
    d = credentials_dir()
    d.mkdir(parents=True, exist_ok=True)
    path = credentials_path()
    path.write_text(json.dumps(creds.to_dict(), indent=2))
    if sys.platform != "win32":
        try:
            os.chmod(path, 0o600)
        except OSError:
            pass


def clear() -> None:
    path = credentials_path()
    if path.exists():
        path.unlink()
