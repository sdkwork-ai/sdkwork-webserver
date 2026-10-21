from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class UpsertDomainWechatVerificationRequest:
    file_name: str
    content: str
