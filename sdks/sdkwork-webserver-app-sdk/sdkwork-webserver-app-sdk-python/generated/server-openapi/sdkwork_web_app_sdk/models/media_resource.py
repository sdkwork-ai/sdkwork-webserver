from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .media_checksum import MediaChecksum


@dataclass
class MediaResource:
    kind: str
    source: str
    id: Optional[str] = None
    url: Optional[str] = None
    public_url: Optional[str] = None
    uri: Optional[str] = None
    object_blob_id: Optional[str] = None
    file_name: Optional[str] = None
    mime_type: Optional[str] = None
    size_bytes: Optional[str] = None
    checksum: Optional[MediaChecksum] = None
    width: Optional[int] = None
    height: Optional[int] = None
    duration_seconds: Optional[float] = None
    alt_text: Optional[str] = None
    title: Optional[str] = None
    metadata: Optional[Dict[str, Any]] = None
