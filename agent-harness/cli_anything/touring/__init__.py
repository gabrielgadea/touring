"""cli-anything-touring: CLI harness for Touring intelligence system.

Provides direct SQLite access (Tier 1, <10ms) to Touring's 3 databases,
subprocess calls to the touring binary (Tier 2, <50ms), and future MCP
bridge (Tier 3) for full 21-tool coverage.
"""

__version__ = "0.4.0"
