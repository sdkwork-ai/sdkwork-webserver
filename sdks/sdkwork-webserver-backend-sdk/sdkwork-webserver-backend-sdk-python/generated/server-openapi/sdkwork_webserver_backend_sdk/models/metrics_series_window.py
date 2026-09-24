from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Optional, List, Dict, Any


@dataclass
class MetricsSeriesWindow:
    """The window the per-day series was cut against, as the server resolved it. Reported because the request's bounds are optional: a surface that labelled the series from its own guess at the default would name a period the points do not cover. Deliberately separate from the card windows, which answer a different question and may not coincide with this one."""
    date_from: str
    date_to: str
