"""Axion SDK exceptions."""


class AxionError(Exception):
    """Base client error."""


class AxionTransportError(AxionError):
    """The server could not be reached after the configured attempts."""


class AxionAPIError(AxionError):
    """A structured non-success response from Axion."""

    def __init__(self, status_code: int, code: str, message: str) -> None:
        super().__init__(f"{code}: {message} (HTTP {status_code})")
        self.status_code = status_code
        self.code = code
        self.message = message


class AxionNotFoundError(AxionAPIError):
    """A collection, vector, or route was not found."""


class AxionConflictError(AxionAPIError):
    """A create operation conflicts with existing state."""


class AxionValidationError(AxionAPIError):
    """Axion rejected a request as invalid."""
