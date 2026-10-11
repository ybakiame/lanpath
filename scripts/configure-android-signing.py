"""Create temporary CI signing inputs without printing credentials."""
import base64
import os
from pathlib import Path

key = os.environ["ANDROID_KEY_BASE64"]
password = os.environ["ANDROID_KEY_PASSWORD"]
alias = os.environ["ANDROID_KEY_ALIAS"]
if not key or not password or not alias:
    raise ValueError("Android release signing Secrets are required")
if any(character in password + alias for character in "\n\r\\"):
    raise ValueError("Signing properties contain unsupported characters")
keystore = Path(os.environ["RUNNER_TEMP"]) / "lanpath-release.p12"
keystore.write_bytes(base64.b64decode(key, validate=True))
keystore.chmod(0o600)
properties = Path("apps/client/src-tauri/gen/android/keystore.properties")
properties.write_text(f"storeFile={keystore}\npassword={password}\nkeyAlias={alias}\n")
properties.chmod(0o600)
