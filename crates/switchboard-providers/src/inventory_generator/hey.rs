use std::collections::HashSet;

use serde::Deserialize;
use switchboard_core::{Error, Result};

use crate::{
    inventory::{CliInventory, CliInventoryCommand, CliInventoryNodeKind, CliOperationKind, CliUndoSupport},
    inventory_generator::{build_help_args, render_command, CliInventoryTarget},
};

#[derive(Deserialize)]
struct CommandCatalog {
    ok: bool,
    data: Vec<CatalogCommand>,
}

#[derive(Deserialize)]
struct CatalogCommand {
    name: String,
    path: String,
    short: String,
    #[serde(default)]
    subcommands: Vec<CatalogCommand>,
}

pub(crate) fn parse_catalog(target: &CliInventoryTarget, json: &str) -> Result<CliInventory> {
    let catalog: CommandCatalog =
        serde_json::from_str(json).map_err(|error| Error::Config(format!("invalid HEY command catalog: {error}")))?;
    if !catalog.ok {
        return Err(Error::Execution("HEY command catalog reported failure".into()));
    }

    let mut records = Vec::new();
    let mut visited = HashSet::new();
    let roots = catalog
        .data
        .into_iter()
        .filter(|command| supported_path(&command.path))
        .collect::<Vec<_>>();
    records.push(CliInventoryCommand {
        path: Vec::new(),
        command: target.program.clone(),
        summary: "HEY mail and calendar CLI".into(),
        usage: None,
        help_args: build_help_args(target, &[]),
        node_kind: CliInventoryNodeKind::Group,
        operation_kind: CliOperationKind::Unknown,
        undo_support: CliUndoSupport::Unknown,
        subcommands: roots.iter().map(|command| command.name.clone()).collect(),
    });
    for command in roots {
        walk_catalog(target, command, &[], &mut visited, &mut records)?;
    }
    records.sort_by(|left, right| left.path.cmp(&right.path));
    for record in &mut records {
        record.subcommands.sort();
    }
    Ok(CliInventory {
        provider: target.provider.clone(),
        program: target.program.clone(),
        commands: records,
    })
}

fn walk_catalog(
    target: &CliInventoryTarget,
    command: CatalogCommand,
    parent: &[String],
    visited: &mut HashSet<Vec<String>>,
    records: &mut Vec<CliInventoryCommand>,
) -> Result<()> {
    let mut path = parent.to_vec();
    path.push(command.name.clone());
    if command.name.is_empty()
        || command.name.starts_with('-')
        || command.name.chars().any(char::is_whitespace)
        || path.join(" ") != command.path
        || !visited.insert(path.clone())
    {
        return Err(Error::Config(format!(
            "invalid or duplicate HEY command path: {}",
            command.path
        )));
    }

    let subcommands = command
        .subcommands
        .into_iter()
        .filter(|child| supported_path(&child.path))
        .collect::<Vec<_>>();
    // Search executes a query itself as well as exposing the filters subcommand.
    let node_kind = if subcommands.is_empty() || command.path == "search" {
        CliInventoryNodeKind::Operation
    } else {
        CliInventoryNodeKind::Group
    };
    let operation_kind = if node_kind == CliInventoryNodeKind::Group {
        CliOperationKind::Unknown
    } else {
        classify_operation(&command.path)
    };
    records.push(CliInventoryCommand {
        path: path.clone(),
        command: render_command(&target.program, &path),
        summary: command.short,
        usage: None,
        help_args: build_help_args(target, &path),
        node_kind,
        operation_kind,
        undo_support: if operation_kind == CliOperationKind::Read {
            CliUndoSupport::None
        } else {
            CliUndoSupport::Unknown
        },
        subcommands: subcommands.iter().map(|child| child.name.clone()).collect(),
    });
    for child in subcommands {
        walk_catalog(target, child, &path, visited, records)?;
    }
    Ok(())
}

fn supported_path(path: &str) -> bool {
    match path.split(' ').next().unwrap_or_default() {
        "auth" => matches!(path, "auth" | "auth status"),
        "account" => matches!(path, "account" | "account list"),
        "attachment" | "box" | "bubble" | "bulk-reply" | "bundle" | "calendar" | "clip" | "collection" | "compose"
        | "contact" | "draft" | "event" | "forward" | "habit" | "ignore" | "journal" | "label" | "move" | "reply"
        | "screener" | "search" | "seen" | "set-aside" | "share" | "snippet" | "spam" | "stop-ignoring" | "thread"
        | "timetrack" | "todo" | "trash" | "unseen" | "unshare" | "workflow" => true,
        _ => false,
    }
}

fn classify_operation(path: &str) -> CliOperationKind {
    // Names and summaries are not safety contracts: share creates a public link,
    // and contact unbundle mutates mail despite its "List" summary.
    match path {
        "account list"
        | "attachment list"
        | "auth status"
        | "box list"
        | "box view"
        | "bubble list"
        | "bulk-reply preview"
        | "bundle view"
        | "calendar list"
        | "clip list"
        | "collection list"
        | "collection view"
        | "contact list"
        | "contact note show"
        | "contact show"
        | "contact threads"
        | "draft list"
        | "draft show"
        | "event day"
        | "event list"
        | "event week"
        | "habit list"
        | "journal list"
        | "journal read"
        | "label list"
        | "label view"
        | "screener history"
        | "screener list"
        | "search"
        | "search filters"
        | "set-aside group list"
        | "set-aside group view"
        | "set-aside view"
        | "snippet list"
        | "thread read"
        | "timetrack categories"
        | "timetrack current"
        | "timetrack list"
        | "todo list"
        | "workflow list"
        | "workflow view" => CliOperationKind::Read,
        _ => CliOperationKind::Write,
    }
}

#[cfg(test)]
mod tests {
    use switchboard_core::ProviderKind;

    use super::{parse_catalog, CatalogCommand, CommandCatalog};
    use crate::{
        inventory::{CliInventoryNodeKind, CliOperationKind},
        inventory_generator::{CliHelpStrategy, CliInventoryTarget},
    };

    const CATALOG: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/help/hey-commands.json"
    ));

    fn target() -> CliInventoryTarget {
        CliInventoryTarget {
            provider: ProviderKind::Hey,
            program: "hey".into(),
            output_path: "ignored.json".into(),
            help_strategy: CliHelpStrategy::HeyCommandsJson,
        }
    }

    #[test]
    fn parses_real_catalog_with_conservative_operation_classification() {
        let inventory = parse_catalog(&target(), CATALOG).expect("catalog should parse");
        for path in [
            vec!["search"],
            vec!["search", "filters"],
            vec!["thread", "read"],
            vec!["event", "day"],
            vec!["auth", "status"],
        ] {
            let command = inventory.command(&path).expect("read should exist");
            assert_eq!(command.operation_kind, CliOperationKind::Read);
            assert_eq!(command.node_kind, CliInventoryNodeKind::Operation);
        }
        for path in [
            vec!["share"],
            vec!["contact", "unbundle"],
            vec!["attachment", "save"],
            vec!["timetrack", "export"],
            vec!["compose"],
            vec!["draft", "send"],
        ] {
            assert_eq!(
                inventory.command(&path).expect("write should exist").operation_kind,
                CliOperationKind::Write
            );
        }
        for path in [
            vec!["auth", "token"],
            vec!["auth", "login"],
            vec!["account", "use"],
            vec!["config"],
            vec!["setup"],
            vec!["mcp"],
            vec!["watch"],
            vec!["tui"],
            vec!["upgrade"],
        ] {
            assert!(inventory.command(&path).is_none(), "excluded command: {path:?}");
        }
    }

    #[test]
    fn unknown_operations_are_writes_even_with_read_sounding_summaries() {
        let mut catalog: CommandCatalog = serde_json::from_str(CATALOG).expect("catalog should parse");
        let contact = catalog
            .data
            .iter_mut()
            .find(|command| command.name == "contact")
            .expect("catalog should contain contact commands");
        contact.subcommands.push(CatalogCommand {
            name: "preview-new".into(),
            path: "contact preview-new".into(),
            short: "List contact information".into(),
            subcommands: Vec::new(),
        });
        let mut records = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let contact = catalog
            .data
            .into_iter()
            .find(|command| command.name == "contact")
            .expect("catalog should retain the extended contact subtree");
        super::walk_catalog(&target(), contact, &[], &mut visited, &mut records)
            .expect("extended contact subtree should produce inventory records");
        let command = records
            .iter()
            .find(|command| command.path == ["contact", "preview-new"])
            .expect("inventory should contain the new contact operation");
        assert_eq!(command.operation_kind, CliOperationKind::Write);
    }
}
