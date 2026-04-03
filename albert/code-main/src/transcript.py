from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class TranscriptStore:
    entries: list[dict[str, object]] = field(default_factory=list)
    flushed: bool = False

    def append(self, entry: dict[str, object]) -> None:
        self.entries.append(entry)
        self.flushed = False

    def compact(self, keep_last: int = 10) -> None:
        if len(self.entries) > keep_last:
            self.entries[:] = self.entries[-keep_last:]

    def replay(self) -> tuple[dict[str, object], ...]:
        return tuple(self.entries)

    def flush(self) -> None:
        self.flushed = True
