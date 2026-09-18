use switchboard_core::{
    Adapter, AuthSecretRefs, Error, ExecutionMode, ExecutionTarget, PlannedAction, PlanningTarget, ProviderKind,
    ResolvedAuth, ResolvedCredentials, ResolvedNamespace, ToolArgument, ToolKind, ToolRequest,
};

use crate::{
    cli::CliRuntimeMaterializer,
    hey::{materializer::HeyCliMaterializer, HeyAdapter},
};

fn target() -> PlanningTarget {
    PlanningTarget {
        namespace: ResolvedNamespace::new("hey.personal", ProviderKind::Hey, "42", "hey_login", false, None)
            .expect("namespace"),
        auth: ResolvedAuth::new("hey_login", "stored login", AuthSecretRefs::HeyCli).expect("auth"),
    }
}

fn plan(adapter: &HeyAdapter, tool: &str, argv: &[&str]) -> switchboard_core::Result<PlannedAction> {
    let request = ToolRequest::new(
        tool,
        "hey.personal",
        ExecutionMode::Auto,
        argv.iter()
            .map(|arg| ToolArgument::option("argv", *arg))
            .collect::<switchboard_core::Result<Vec<_>>>()?,
    )?;
    let descriptor = adapter.find_tool(&request.tool).expect("registered tool");
    adapter.plan(&target(), &request, descriptor)
}

#[test]
fn reads_and_writes_follow_native_command_semantics() {
    let adapter = HeyAdapter::new().expect("adapter");
    for argv in [
        vec!["box", "list"],
        vec!["thread", "read", "123"],
        vec!["search", "invoice"],
        vec!["search", "filters"],
        vec!["event", "day"],
    ] {
        assert_eq!(
            plan(&adapter, "hey.cli.read", &argv).expect("read").kind,
            ToolKind::Read
        );
    }
    for argv in [
        vec!["compose", "--draft", "-m", "Hello"],
        vec!["draft", "send", "123"],
        vec!["share", "123"],
        vec!["seen", "123"],
        vec!["attachment", "save", "123:1"],
        vec!["timetrack", "export", "--output", "/tmp/time.csv"],
    ] {
        assert!(matches!(
            plan(&adapter, "hey.cli.read", &argv),
            Err(Error::InvalidArguments(_))
        ));
        assert_eq!(
            plan(&adapter, "hey.cli.write", &argv).expect("write").kind,
            ToolKind::Write
        );
    }
}

#[test]
fn raw_commands_cannot_escape_the_supported_catalog_or_namespace() {
    let adapter = HeyAdapter::new().expect("adapter");
    for argv in [
        vec!["auth", "token"],
        vec!["auth", "login"],
        vec!["config", "trust-local"],
        vec!["watch"],
        vec!["tui"],
        vec!["upgrade"],
        vec!["account", "use", "99"],
        vec!["box", "list", "--account=99"],
        vec!["box", "list", "--account", "99"],
        vec!["box", "list", "--base-url", "https://example.com"],
        vec!["unknown"],
    ] {
        assert!(
            matches!(plan(&adapter, "hey.cli.write", &argv), Err(Error::InvalidArguments(_))),
            "{argv:?}"
        );
    }
}

#[test]
fn leaf_commands_support_no_arguments_and_preserve_explicit_native_arguments() {
    let adapter = HeyAdapter::new().expect("adapter");
    let action = plan(&adapter, "hey.cli.box.list", &[]).expect("no-argument leaf");
    assert_eq!(action.args.values("argv").collect::<Vec<_>>(), vec!["--"]);
    let export = plan(&adapter, "hey.cli.timetrack.export", &[]).expect("native CSV export");
    assert_eq!(export.args.values("argv").collect::<Vec<_>>(), vec!["--"]);
    assert!(plan(&adapter, "hey.cli.compose", &["--draft", "--message=--account"]).is_ok());
    let action = plan(&adapter, "hey.cli.compose", &["--draft", "-m", "Hello"]).expect("draft");
    assert_eq!(action.kind, ToolKind::Write);
    assert_eq!(
        action.args.values("argv").collect::<Vec<_>>(),
        vec!["--draft", "-m", "Hello"]
    );
}

#[test]
fn cli_runtime_pins_linked_account_and_reuses_stored_login_without_prompts() {
    let planning = target();
    let mut execution = ExecutionTarget {
        namespace: planning.namespace,
        auth: planning.auth,
        credentials: ResolvedCredentials::HeyCli,
    };
    let runtime = HeyCliMaterializer.prepare(&execution).expect("materialize");
    assert_eq!(runtime.env().get("HEY_ACCOUNT_ID").map(String::as_str), Some("42"));
    assert_eq!(runtime.env().get("HEY_NONINTERACTIVE").map(String::as_str), Some("1"));
    for key in ["HEY_TOKEN", "HEY_BASE_URL", "HEY_DEBUG"] {
        assert!(runtime.cleared_env().contains(key));
    }
    execution.namespace.account_label = "all".into();
    assert!(HeyCliMaterializer.prepare(&execution).is_ok());
    for account in ["personal", "0", "-1", "42,43"] {
        execution.namespace.account_label = account.into();
        assert!(HeyCliMaterializer.prepare(&execution).is_err());
    }
    execution.namespace.account_label = "42".into();
    execution.namespace.state_dir = Some("/tmp/hey-isolation-not-supported".into());
    assert!(HeyCliMaterializer.prepare(&execution).is_err());
    execution.namespace.state_dir = None;
    execution.credentials = ResolvedCredentials::GitHubCli;
    assert!(HeyCliMaterializer.prepare(&execution).is_err());
}

#[test]
fn probes_and_executes_real_hey_cli_without_accessing_mail() {
    let adapter = HeyAdapter::new().expect("adapter");
    let action = plan(&adapter, "hey.cli.auth.status", &["--help"]).expect("plan");
    let planning = target();
    let execution = ExecutionTarget {
        namespace: planning.namespace,
        auth: planning.auth,
        credentials: ResolvedCredentials::HeyCli,
    };
    let output = adapter.execute(&execution, &action).expect("real HEY CLI execution");
    #[derive(serde::Deserialize)]
    struct Fields {
        cli_version: String,
        stdout_text: String,
        argv: Vec<String>,
    }
    let fields: Fields =
        serde_json::from_value(serde_json::to_value(output.fields).expect("serialize")).expect("native help output");
    assert!(fields.cli_version.contains("hey version"));
    assert!(fields.stdout_text.contains("hey auth status"));
    assert_eq!(fields.argv, vec!["auth", "status", "--help"]);
}
