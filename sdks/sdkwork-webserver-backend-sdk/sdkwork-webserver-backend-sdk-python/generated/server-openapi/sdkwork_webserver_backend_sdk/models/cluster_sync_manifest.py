from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class ClusterSyncManifest:
    cluster_id: str
    kind: str
    revision: str
    sha256: str
    payload: Dict[str, Any]
    created_at: str
