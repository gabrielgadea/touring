//! ContextRuntime implementation for HookRuntime.

use super::traits::ContextRuntime;
use crate::runtime::HookRuntime;
use crate::{IntentClassifier, PIIScanner};

impl ContextRuntime for HookRuntime {
    fn classifier(&self) -> &IntentClassifier {
        &self.ctx.classifier
    }

    fn pii_scanner(&self) -> &PIIScanner {
        &self.ctx.pii_scanner
    }

    fn context_injection_file(&self) -> Option<&String> {
        self.context_injection_file.as_ref()
    }

    fn set_context_injection_file(&mut self, file_path: Option<String>) {
        self.context_injection_file = file_path;
    }
}
