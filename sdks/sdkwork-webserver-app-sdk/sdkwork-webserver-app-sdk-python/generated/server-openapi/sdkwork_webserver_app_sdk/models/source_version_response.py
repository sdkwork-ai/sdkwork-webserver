from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .source_version_config_snapshot import SourceVersionConfigSnapshot


@dataclass
class SourceVersionResponse:
    id: str
    application_id: str
    version_tag: str
    source_type: str
    artifact_drive_uri: str
    artifact_size: str
    artifact_hash: str
    config_snapshot: SourceVersionConfigSnapshot
    status: int
    retained: bool
    created_at: str
    source_ref: Optional[str] = None
    commit_hash: Optional[str] = None
