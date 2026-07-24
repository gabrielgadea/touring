"""Setup for cli-anything-touring harness."""

from setuptools import find_namespace_packages, setup

setup(
    name="cli-anything-touring",
    version="0.4.0",
    description="CLI harness for Touring intelligence system — direct SQLite + subprocess access",
    author="Gabriel Gadea",
    python_requires=">=3.10",
    packages=find_namespace_packages(include=["cli_anything.*"]),
    install_requires=[
        "typer>=0.9.0",
    ],
    entry_points={
        "console_scripts": [
            "cli-anything-touring=cli_anything.touring.touring_cli:main",
        ],
    },
)
