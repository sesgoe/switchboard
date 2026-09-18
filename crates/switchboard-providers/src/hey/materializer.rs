use switchboard_core::{Error, ExecutionTarget, ResolvedCredentials, ResolvedNamespace, Result};

use crate::{cli::CliRuntimeMaterializer, process_runtime::ProcessContext};

pub(crate) struct HeyCliMaterializer;

pub(crate) fn validate_namespace(namespace: &ResolvedNamespace) -> Result<()> {
    let account = &namespace.account_label;
    if account != "all" && !account.parse::<u64>().is_ok_and(|id| id > 0) {
        return Err(Error::Config(format!(
            "HEY namespace {} account must be a positive linked account ID or all; use hey account list --json to find IDs",
            namespace.id
        )));
    }
    if namespace.state_dir.is_some() {
        return Err(Error::Config(
            "HEY uses its existing CLI login; namespace state_dir is not supported".into(),
        ));
    }
    Ok(())
}

impl CliRuntimeMaterializer for HeyCliMaterializer {
    fn prepare(&self, target: &ExecutionTarget) -> Result<ProcessContext> {
        validate_namespace(&target.namespace)?;
        if !matches!(target.credentials, ResolvedCredentials::HeyCli) {
            return Err(Error::UnsupportedOperation(format!(
                "hey cli materializer does not support {} credentials",
                target.auth.kind()
            )));
        }
        let mut context = ProcessContext::new();
        context.set_env("HEY_ACCOUNT_ID", &target.namespace.account_label);
        context.set_env("HEY_NONINTERACTIVE", "1");
        // Keep the CLI's stored login, but prevent inherited overrides from changing its authority.
        context.clear_env("HEY_TOKEN");
        context.clear_env("HEY_BASE_URL");
        context.clear_env("HEY_DEBUG");
        Ok(context)
    }
}
