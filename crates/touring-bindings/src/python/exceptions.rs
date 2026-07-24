//! Custom Python exceptions for the claude_learning_kernel module.
//!
//! Provides domain-specific exception types for clearer error reporting
//! from Rust to Python consumers.

use pyo3::create_exception;
use pyo3::exceptions::PyException;

// ACO-specific exceptions
create_exception!(
    claude_learning_kernel,
    AcoGraphError,
    PyException,
    "Error in ACO graph operations (cycles, missing nodes, etc.)."
);
create_exception!(
    claude_learning_kernel,
    AcoValidationError,
    PyException,
    "Error in ACO validation (contracts, dependencies)."
);

// AST-specific exceptions
create_exception!(
    claude_learning_kernel,
    AstParseError,
    PyException,
    "Error parsing source code via tree-sitter."
);
create_exception!(
    claude_learning_kernel,
    AstSurgeryError,
    PyException,
    "Error performing AST surgery (body replacement)."
);

// General
create_exception!(
    claude_learning_kernel,
    SerializationError,
    PyException,
    "Error serializing/deserializing data."
);
