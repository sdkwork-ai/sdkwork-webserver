from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class ImportApplicationGitSourceVersionRequest:
    version_tag: str
    repository_url: str
    git_ref: Optional[str] = None
