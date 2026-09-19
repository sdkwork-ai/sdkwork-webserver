from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class ServerOperationResult:
    operation_id: str
    exit_code: Optional[int] = None
    timed_out: Optional[bool] = None
    stdout: Optional[str] = None
    stderr: Optional[str] = None
    stdout_truncated: Optional[bool] = None
    stderr_truncated: Optional[bool] = None
    pid: Optional[int] = None
    pid_file: Optional[str] = None
    log_file: Optional[str] = None
    stopped: Optional[bool] = None
    message: Optional[str] = None
