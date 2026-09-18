# HEY

Switchboard wraps the installed [HEY CLI](https://github.com/basecamp/hey-cli)
using its stored login. The checked-in command inventory and CI target
[HEY CLI v1.5.0](https://github.com/basecamp/hey-cli/releases/tag/v1.5.0).
The provider offers raw read/write tools and individual tools generated from
the native command catalog. It has no curated mail or calendar layer yet.

## Configure an existing login

Ensure `hey` is on `PATH`. An existing HEY login needs no new credentials or
secret references in Switchboard. If authentication needs recovery, run
`hey auth login` interactively with the user present, outside Switchboard.
Switchboard does not perform unattended login.

Add this namespace to your Switchboard configuration:

```toml
[namespace.hey.personal]
provider = "hey"
account = "all"
default_read = true
```

Omitting `auth` selects CLI-managed authentication (`hey_cli`). `account` must
be `all` or a positive linked mail account ID. Start with `all` and discover
IDs through the provider:

```sh
switchboard doctor --ns hey.personal
switchboard hey.cli.auth.status --ns hey.personal --json
switchboard hey.cli.account.list --ns hey.personal --json
```

To select one linked mail account, replace `all` with its ID, for example
`account = "12345"`. Switchboard passes this selection as `HEY_ACCOUNT_ID`.
These namespaces filter linked mail accounts within one stored login. They
do not represent independent logins and do not isolate calendar, todo, habit,
journal, or time tracking data. HEY namespaces do not support `state_dir`.

An explicit shared auth entry is optional:

```toml
[auth.hey_login]
provider = "hey"
kind = "hey_cli"
account = "existing login"

[namespace.hey.work]
provider = "hey"
account = "12345"
auth = "hey_login"
```

Here `auth.account` is a descriptive label; `namespace.account` controls the
linked mail account selection. `switchboard auth check` does not currently
support HEY identity verification. `hey.cli.auth.status` reports native login
status, which is not proof of a particular linked mail account's identity.

## Discover and read

```sh
switchboard tools list --provider hey
switchboard tools describe hey.cli.search --full
switchboard hey.cli.box.list --ns hey.personal --json
switchboard hey.cli.search --ns hey.personal \
  --argv-json '["quarterly planning","--json"]' --json
switchboard hey.cli.thread.read --ns hey.personal \
  --argv-json '["12345","--json"]' --json
```

Leaf tools supply their native command path and accept calls with no native
arguments, such as `hey.cli.box.list`. HEY normally returns JSON when captured;
commands such as `timetrack export` retain their native output format. Include
native `--json` explicitly when needed. Switchboard's own `--json` selects its
output envelope.

The root raw read tool takes the complete native command:

```sh
switchboard hey.cli.read --ns hey.personal --json -- box list --json
```

## Prepare and apply a draft

Writes create a planned Switchboard operation by default. For an email draft:

```sh
switchboard hey.cli.compose --ns hey.personal \
  --argv-json '["--to","person@example.com","--subject","Hello","-m","Draft body","--draft"]' \
  --json
```

The native `--draft` inside the argument array tells HEY to save an unsent email
when the operation is applied. Switchboard planning itself does not create
the remote draft. Inspect the returned operation, then approve and apply it
when that exact draft creation is authorized:

```sh
switchboard op show OPERATION_ID
switchboard op approve OPERATION_ID --actor human
switchboard op apply OPERATION_ID --json
switchboard hey.cli.draft.list --ns hey.personal --json
switchboard hey.cli.draft.show --ns hey.personal \
  --argv-json '["DRAFT_ID","--json"]' --json
```

Read back the draft to confirm its contents and unsent state. Approval to
create a draft is not approval to send it. Keep operation IDs; inspect uncertain
operations before retrying. Raw writes do not have automatic readback or undo.

The equivalent root raw write call is:

```sh
switchboard hey.cli.write --ns hey.personal --json -- \
  compose --to person@example.com --subject Hello -m 'Draft body' --draft
```

## Provider boundaries

Both root and leaf tools enforce the inventory's read/write classification.
The read tool rejects writes. Unknown commands are rejected. Put native command
names before flags when using root raw tools.

The inventory excludes local administration, login, token export, interactive
UI, `watch`, and MCP server commands. Native `--account` and `--base-url`
overrides are rejected; select the configured account with Switchboard's
`--ns`. These flag names are reserved argv tokens, including when used as
literal text; encode such values as `--message=--account`, for example.
The provider runs noninteractively and clears inherited token and server
overrides while retaining the CLI's stored login.
