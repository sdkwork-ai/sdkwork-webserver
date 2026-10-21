from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class PluginResponse:
    id: str
    plugin_key: str
    display_name: str
    summary: str
    version: str
    category_id: str
    supported_host_tools: List[str]
    contributed_capabilities: List[str]
    source_kind: str
    status: str
    created_at: str
    updated_at: str
    git_repository: Optional[str] = None
    git_ref: Optional[str] = None
    artifact_ref: Optional[str] = None
    checksum_sha256: Optional[str] = None
    archive_file_name: Optional[str] = None
