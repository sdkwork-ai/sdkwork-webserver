from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class ApplicationDeploymentResponse:
    id: str
    site_id: str
    status: int
    deploy_type: int
    environment: str
    created_at: str
    source_version_id: Optional[str] = None
    version_tag: Optional[str] = None
    commit_hash: Optional[str] = None
    source_ref: Optional[str] = None
    rollback_from_deployment_id: Optional[str] = None
    artifact_drive_uri: Optional[str] = None
    artifact_size: Optional[str] = None
    artifact_hash: Optional[str] = None
    started_at: Optional[str] = None
    completed_at: Optional[str] = None
    duration_ms: Optional[str] = None
