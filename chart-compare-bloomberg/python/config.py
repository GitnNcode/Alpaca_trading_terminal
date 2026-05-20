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
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import urlparse


DEFAULT_BASE_URL = "https://paper-api.alpaca.markets"

# Security-fix: explicit allowlist of trusted Alpaca trading hosts. Any
# `base_url` not on this list is rejected on load and replaced with the safe
# default. This prevents a tampered credentials file from redirecting
# authenticated requests (which carry the live API key + secret) to an
# attacker-controlled host.
_TRUSTED_HOSTS = frozenset({
    "paper-api.alpaca.markets",
    "api.alpaca.markets",
})

# Security-fix: lightweight format validation for credential material. The
# exact format is not part of Alpaca's public contract, so we use loose but
# non-trivial bounds — enough to reject obvious junk / injection attempts and
# to fail closed if a malformed file is presented, while still tolerating
# legitimate variations across paper vs. live keys.
_KEY_RE = re.compile(r"^[A-Za-z0-9]{16,64}$")
_SECRET_RE = re.compile(r"^[A-Za-z0-9/+=_\-]{32,128}$")


@dataclass
class Credentials:
    api_key: str
    api_secret: str
    base_url: str = DEFAULT_BASE_URL

    def to_dict(self) -> dict[str, str]:
        return {
            "api_key": self.api_key,
            "api_secret": self.api_secret,
            "base_url": self.base_url,
        }


def _is_trusted_base_url(url: str) -> bool:
    # Security-fix: reject non-HTTPS schemes and any host outside the
    # allowlist. Also reject URLs with credentials embedded in the netloc
    # (e.g. https://attacker@host/...) — Python's urlparse exposes the host
    # via .hostname which strips them, but presence of userinfo is a red flag.
    if not isinstance(url, str) or not url:
        return False
    try:
        parsed = urlparse(url)
    except ValueError:
        return False
    if parsed.scheme != "https":
        return False
    if parsed.username or parsed.password:
        return False
    host = (parsed.hostname or "").lower()
    return host in _TRUSTED_HOSTS


def _restrict_windows_acl(path: Path) -> None:
    # Security-fix: tighten ACLs on Windows so other local users cannot read
    # the secret. We disable inheritance and grant Full Control only to the
    # current user. `icacls` ships with every supported Windows release, so
    # this avoids pulling in a pywin32 dependency. Errors are swallowed —
    # locked-down environments may already restrict icacls itself, and we
    # prefer to leave the file usable rather than crash the app.
    user = os.environ.get("USERNAME")
    if not user:
        return
    try:
        subprocess.run(
            ["icacls", str(path), "/inheritance:r", "/grant:r", f"{user}:F"],
            check=False,
            capture_output=True,
            timeout=5,
        )
    except (OSError, subprocess.SubprocessError):
        pass


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
    # Security-fix: every field is validated before it can be used to build
    # authenticated requests. Untrusted base_url values silently fall back to
    # the safe default rather than being honored — an attacker who can write
    # the credentials file should not be able to exfiltrate the secret by
    # pointing base_url at a host they control.
    path = credentials_path()
    if not path.exists():
        return None
    try:
        raw = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError):
        return None
    if not isinstance(raw, dict):
        return None
    key = raw.get("api_key") or raw.get("APCA-API-KEY-ID") or ""
    secret = raw.get("api_secret") or raw.get("APCA-API-SECRET-KEY") or ""
    base = raw.get("base_url") or DEFAULT_BASE_URL
    if not isinstance(key, str) or not isinstance(secret, str) or not isinstance(base, str):
        return None
    if not _KEY_RE.match(key) or not _SECRET_RE.match(secret):
        return None
    if not _is_trusted_base_url(base):
        # Refuse to honor an untrusted endpoint for a credentialed request.
        base = DEFAULT_BASE_URL
    return Credentials(api_key=key, api_secret=secret, base_url=base)


def save(creds: Credentials) -> None:
    # Security-fix: validate before persisting so a buggy caller can't write
    # an attacker-controlled base_url that a future `load()` might trust
    # transitively. Mirror the same checks used on read.
    if not _KEY_RE.match(creds.api_key) or not _SECRET_RE.match(creds.api_secret):
        raise ValueError("invalid Alpaca API key/secret format")
    if not _is_trusted_base_url(creds.base_url):
        raise ValueError(f"untrusted base_url: {creds.base_url!r}")
    d = credentials_dir()
    d.mkdir(parents=True, exist_ok=True)
    path = credentials_path()
    path.write_text(json.dumps(creds.to_dict(), indent=2))
    if sys.platform == "win32":
        _restrict_windows_acl(path)
    else:
        try:
            os.chmod(path, 0o600)
        except OSError:
            pass


def clear() -> None:
    path = credentials_path()
    if path.exists():
        path.unlink()
