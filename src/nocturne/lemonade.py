"""A small client for the Lemonade OpenAI-compatible API.

The client uses only the standard library so the scaffold runs without extra
packages. Swap it for the official OpenAI client when the project grows.
"""

from __future__ import annotations

import base64
import json
import mimetypes
import urllib.error
import urllib.request
import uuid
from pathlib import Path
from typing import Any


class LemonadeError(RuntimeError):
    """Raised when the local Lemonade server cannot serve a request."""


class LemonadeClient:
    def __init__(
        self,
        base_url: str = "http://localhost:13305/api/v1",
        api_key: str = "lemonade",
        timeout: int = 180,
    ):
        self.base_url = base_url.rstrip("/")
        self.api_key = api_key
        self.timeout = timeout

    def chat(
        self,
        messages: list[dict[str, Any]],
        model: str,
        temperature: float = 0.2,
        max_tokens: int = 1024,
    ) -> str:
        payload = {
            "model": model,
            "messages": messages,
            "temperature": temperature,
            "max_tokens": max_tokens,
        }
        data = self._post_json("/chat/completions", payload)
        return data["choices"][0]["message"]["content"]

    def transcribe(self, path: str | Path, model: str = "Whisper-Large-v3-Turbo") -> str:
        path = Path(path)
        boundary = f"----nocturne{uuid.uuid4().hex}"
        mime = mimetypes.guess_type(path.name)[0] or "application/octet-stream"
        body = bytearray()

        def field(name: str, value: str) -> None:
            body.extend(f"--{boundary}\r\n".encode())
            body.extend(
                f'Content-Disposition: form-data; name="{name}"\r\n\r\n'.encode()
            )
            body.extend(value.encode())
            body.extend(b"\r\n")

        field("model", model)
        body.extend(f"--{boundary}\r\n".encode())
        body.extend(
            (
                f'Content-Disposition: form-data; name="file"; filename="{path.name}"\r\n'
                f"Content-Type: {mime}\r\n\r\n"
            ).encode()
        )
        body.extend(path.read_bytes())
        body.extend(b"\r\n")
        body.extend(f"--{boundary}--\r\n".encode())

        raw = self._request(
            "/audio/transcriptions",
            data=bytes(body),
            headers={
                "Content-Type": f"multipart/form-data; boundary={boundary}",
                "Authorization": f"Bearer {self.api_key}",
            },
        )
        return json.loads(raw)["text"]

    def speak(
        self,
        text: str,
        model: str = "kokoro-v1",
        voice: str = "af_heart",
        response_format: str = "wav",
    ) -> bytes:
        payload = {
            "model": model,
            "input": text,
            "voice": voice,
            "response_format": response_format,
        }
        return self._request(
            "/audio/speech",
            data=json.dumps(payload).encode("utf-8"),
            headers={
                "Content-Type": "application/json",
                "Authorization": f"Bearer {self.api_key}",
            },
        )

    def generate_image(
        self, prompt: str, model: str = "Flux-2-Klein-9B-GGUF", size: str = "1024x1024"
    ) -> bytes:
        data = self._post_json(
            "/images/generations",
            {"model": model, "prompt": prompt, "size": size, "response_format": "b64_json"},
        )
        return base64.b64decode(data["data"][0]["b64_json"])

    def _post_json(self, path: str, payload: dict[str, Any]) -> dict[str, Any]:
        raw = self._request(
            path,
            data=json.dumps(payload).encode("utf-8"),
            headers={
                "Content-Type": "application/json",
                "Authorization": f"Bearer {self.api_key}",
            },
        )
        return json.loads(raw)

    def _request(
        self,
        path: str,
        data: bytes | None = None,
        headers: dict[str, str] | None = None,
    ) -> bytes:
        url = f"{self.base_url}{path}"
        request = urllib.request.Request(url, data=data, headers=headers or {})
        try:
            with urllib.request.urlopen(request, timeout=self.timeout) as response:
                return response.read()
        except urllib.error.HTTPError as exc:
            body = exc.read().decode("utf-8", "replace")
            raise LemonadeError(f"{exc.code} {url}: {body}") from exc
        except urllib.error.URLError as exc:
            raise LemonadeError(f"cannot reach {url}: {exc.reason}") from exc
