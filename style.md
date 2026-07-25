# UI Style Notes

## Update actions

- `Update Software` is the normal GUI action: it uses the existing action-stack layout and updates the current checkout without deleting it.
- Hard update and hard delete are terminal-only recovery commands and do not have GUI buttons.
- `Setup & Apply` is shown with the existing suggested-action treatment and requires confirmation before opening an interactive repository setup terminal.
- Long command output is shown in the shared scrollable, monospace error dialog with a `Copy` action.
