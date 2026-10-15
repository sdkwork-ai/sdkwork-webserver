from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .tunnel_machine_response import TunnelMachineResponse


@dataclass
class TunnelMachineListResponse:
    machines: List[TunnelMachineResponse]
