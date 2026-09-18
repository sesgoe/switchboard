mod materializer;
#[cfg(test)]
mod tests;

use switchboard_core::{
    Adapter, Error, ExecutionTarget, PlannedAction, PlanningTarget, ProviderKind, Result, ToolArgument, ToolArguments,
    ToolDescriptor, ToolKind, ToolOutput, ToolRequest,
};

use crate::{
    cli::{command::CliArgsStrategy, CliProviderBackend, CliProviderCatalog},
    hey::materializer::{validate_namespace, HeyCliMaterializer},
    inventory::{embedded_inventory, CliInventory, CliInventoryNodeKind, CliOperationKind},
};

const MANIFEST: &str = include_str!("../../manifests/hey.json");

pub struct HeyAdapter {
    backend: CliProviderBackend,
    catalog: CliProviderCatalog,
    inventory: CliInventory,
}

impl HeyAdapter {
    pub fn new() -> Result<Self> {
        let inventory = embedded_inventory(ProviderKind::Hey)?;
        Ok(Self {
            backend: CliProviderBackend::new(Box::new(HeyCliMaterializer)),
            catalog: CliProviderCatalog::from_embedded(MANIFEST, &inventory)?,
            inventory,
        })
    }

    fn normalize_arguments(&self, tool: &str, args: &ToolArguments) -> Result<ToolArguments> {
        let command = self
            .catalog
            .find_command(tool)
            .ok_or_else(|| Error::UnsupportedTool(tool.into()))?;
        if args.iter().next().is_none()
            && matches!(command.executable.as_ref().map(|spec| &spec.args),
                Some(CliArgsStrategy::RawInventory { prefix }) if !prefix.is_empty())
        {
            // The shared raw transport requires a token; `--` preserves native output defaults.
            return Ok(ToolArguments::new(vec![ToolArgument::option("argv", "--")?]));
        }
        Ok(args.clone())
    }

    fn validate_command(&self, action: &PlannedAction) -> Result<()> {
        let spec = self
            .catalog
            .find_command(action.tool.as_str())
            .and_then(|command| command.executable.as_ref())
            .ok_or_else(|| Error::UnsupportedTool(action.tool.to_string()))?;
        let argv = spec.args.build_args(action)?;
        let command = self
            .inventory
            .commands
            .iter()
            .filter(|command| command.node_kind == CliInventoryNodeKind::Operation)
            .filter(|command| argv.starts_with(&command.path))
            .max_by_key(|command| command.path.len())
            .ok_or_else(|| {
                Error::InvalidArguments(
                    "unsupported HEY command; use a cataloged data command with command names before flags".into(),
                )
            })?;
        if action.kind == ToolKind::Read && command.operation_kind != CliOperationKind::Read {
            return Err(Error::InvalidArguments(format!(
                "{} changes state; use hey.cli.write or its write leaf tool",
                command.command
            )));
        }
        for arg in &argv {
            if matches!(arg.split('=').next(), Some("--account" | "--base-url")) {
                return Err(Error::InvalidArguments(
                    "HEY account and server overrides are not allowed; select the configured namespace with --ns"
                        .into(),
                ));
            }
        }
        Ok(())
    }
}

impl Adapter for HeyAdapter {
    fn provider(&self) -> ProviderKind {
        ProviderKind::Hey
    }

    fn tools(&self) -> &[ToolDescriptor] {
        self.catalog.tools()
    }

    fn plan(
        &self,
        target: &PlanningTarget,
        request: &ToolRequest,
        descriptor: &ToolDescriptor,
    ) -> Result<PlannedAction> {
        validate_namespace(&target.namespace)?;
        let mut request = request.clone();
        request.args = self.normalize_arguments(request.tool.as_str(), &request.args)?;
        let command = self
            .catalog
            .find_command(request.tool.as_str())
            .ok_or_else(|| Error::UnsupportedTool(request.tool.to_string()))?;
        let summary = command.summarize.summarize(&target.namespace, &request)?;
        let action = PlannedAction::new(&request, target, descriptor.kind, summary, descriptor.backend);
        self.validate_command(&action)?;
        Ok(action)
    }

    fn execute(&self, target: &ExecutionTarget, action: &PlannedAction) -> Result<ToolOutput> {
        self.validate_command(action)?;
        let spec = self
            .catalog
            .find_command(action.tool.as_str())
            .and_then(|command| command.executable.as_ref())
            .ok_or_else(|| Error::UnsupportedTool(action.tool.to_string()))?;
        self.backend.execute(target, action, spec)
    }
}
