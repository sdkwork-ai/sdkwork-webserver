from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class UpdatePluginRequest:
    display_name: Optional[str] = None
    summary: Optional[str] = None
    version: Optional[str] = None
    category_id: Optional[str] = None
    supported_host_tools: Optional[List[str]] = None
    contributed_capabilities: Optional[List[str]] = None
    git_repository: Optional[str] = None
    git_ref: Optional[str] = None
    artifact_ref: Optional[str] = None
    checksum_sha256: Optional[str] = None
    archive_file_name: Optional[str] = None
    status: Optional[str] = None
