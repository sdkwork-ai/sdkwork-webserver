from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class CreateApplicationDeploymentRequest:
    """Deployment source command. Git deployments (deployType 2) require an HTTPS sourceRef and may omit artifact fields. Other deployment types require artifactDriveUri, artifactSize, and artifactHash together."""
    source_version_id: Optional[str] = None
    deploy_type: Optional[int] = None
    environment: Optional[str] = None
    version_tag: Optional[str] = None
    commit_hash: Optional[str] = None
    source_ref: Optional[str] = None
    artifact_drive_uri: Optional[str] = None
    artifact_size: Optional[str] = None
    artifact_hash: Optional[str] = None
