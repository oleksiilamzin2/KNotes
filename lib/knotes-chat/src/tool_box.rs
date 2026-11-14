use knotes_core::{DocumentPages, VaultReader};
use knotes_tools::{ToolError, ToolIndex, ToolSpec, VaultTools};
use serde_json::Value;

/// The tools the model may call, run here.
pub trait ToolBox {
    fn specs(&self) -> Vec<ToolSpec>;
    fn call(&self, name: &str, args: &Value) -> Result<Value, ToolError>;
}

impl<I: ToolIndex, F: VaultReader, D: DocumentPages> ToolBox for VaultTools<I, F, D> {
    fn specs(&self) -> Vec<ToolSpec> {
        VaultTools::<I, F, D>::specs()
    }

    fn call(&self, name: &str, args: &Value) -> Result<Value, ToolError> {
        VaultTools::call(self, name, args)
    }
}
