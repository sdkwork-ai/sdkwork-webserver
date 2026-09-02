from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .sdk_work_async_data import SdkWorkAsyncData


@dataclass
class CertificatesRenewResponse202:
    code: int
    data: Any
    trace_id: str
