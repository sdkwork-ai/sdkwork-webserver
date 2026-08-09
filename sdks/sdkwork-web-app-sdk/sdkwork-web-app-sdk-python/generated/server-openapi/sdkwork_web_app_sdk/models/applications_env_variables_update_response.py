from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any

if TYPE_CHECKING:
    from .env_variable_response import EnvVariableResponse


@dataclass
class ApplicationsEnvVariablesUpdateResponse:
    code: int
    data: Any
    trace_id: str
