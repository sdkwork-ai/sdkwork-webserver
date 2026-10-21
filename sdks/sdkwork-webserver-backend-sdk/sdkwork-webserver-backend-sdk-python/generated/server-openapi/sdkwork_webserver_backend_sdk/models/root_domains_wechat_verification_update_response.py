from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .domain_wechat_verification_response import DomainWechatVerificationResponse


@dataclass
class RootDomainsWechatVerificationUpdateResponse:
    code: int
    data: Any
    trace_id: str
