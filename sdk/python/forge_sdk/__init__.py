"""Forge worker SDK.

Standard library only, so `pip install forge-sdk` pulls in nothing else and the
SDK works in a minimal container image.
"""

from .worker import (
    AUTHORIZATION,
    AUTHENTICATION,
    CANCELLATION,
    CONFLICT,
    DEPENDENCY_UNAVAILABLE,
    INTERNAL,
    NOT_FOUND,
    PERMANENT,
    RATE_LIMITED,
    RESOURCE_EXHAUSTED,
    RETRYABLE_BY_DEFAULT,
    TIMEOUT,
    TRANSIENT,
    VALIDATION,
    ForgeError,
    ForgeWorker,
    JobContext,
    LeaseLost,
    classify,
    __version__,
)

__all__ = [
    "ForgeWorker",
    "JobContext",
    "ForgeError",
    "LeaseLost",
    "classify",
    "RETRYABLE_BY_DEFAULT",
    "TRANSIENT",
    "DEPENDENCY_UNAVAILABLE",
    "TIMEOUT",
    "RESOURCE_EXHAUSTED",
    "RATE_LIMITED",
    "VALIDATION",
    "AUTHENTICATION",
    "AUTHORIZATION",
    "NOT_FOUND",
    "CONFLICT",
    "CANCELLATION",
    "PERMANENT",
    "INTERNAL",
    "__version__",
]