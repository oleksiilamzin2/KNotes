use knotes_core::{DocumentPages, FolderRulesStore, VaultReader};
use knotes_tools::{ToolError, ToolIndex, ToolSpec, VaultTools};
use serde_json::Value;

/// The tools the model may call, run here.
pub trait ToolBox {
    fn specs(&self) -> Vec<ToolSpec>;
    fn call(&self, name: &str, args: &Value) -> Result<Value, ToolError>;
    /// What the model is told about the vault before it calls them.
    fn overview(&self) -> String {
        String::new()
    }
    /// Whether the user keeps `path` from the AI (then it is not even named to the model).
    fn excluded(&self, _path: &str) -> bool {
        false
    }
    /// The vault's spaces in a sentence, told each turn (ADR 0030); empty when it has none.
    fn spaces(&self) -> String {
        String::new()
    }
}

impl<I: ToolIndex, F: VaultReader + FolderRulesStore, D: DocumentPages> ToolBox for VaultTools<I, F, D> {
    fn specs(&self) -> Vec<ToolSpec> {
        VaultTools::<I, F, D>::specs()
    }

    fn call(&self, name: &str, args: &Value) -> Result<Value, ToolError> {
        VaultTools::call(self, name, args)
    }

    fn overview(&self) -> String {
        VaultTools::overview(self)
    }

    fn excluded(&self, path: &str) -> bool {
        VaultTools::excluded(self, path)
    }

    fn spaces(&self) -> String {
        VaultTools::spaces_line(self)
    }
}
